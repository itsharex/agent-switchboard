//! Claude 供应商候选端点的批量延迟测试（对齐上游 speedtest）。每个地址只做一次
//! 无凭据、不读响应体的可达性探测；并发有界，结果按输入顺序返回，失败逐条具名。

use super::error::{blocking, CommandError};
use crate::probe::ProbeResult;
use serde::Serialize;

const MAX_URLS: usize = 32;
const MAX_CONCURRENCY: usize = 6;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeEndpointLatency {
    pub url: String,
    pub result: Option<ProbeResult>,
    pub error: Option<String>,
}

fn validate(urls: &[String]) -> Result<(), CommandError> {
    if urls.is_empty() {
        return Err(CommandError::new(
            "claude-endpoint-test-invalid",
            "没有要测试的端点地址",
        ));
    }
    if urls.len() > MAX_URLS {
        return Err(CommandError::new(
            "claude-endpoint-test-invalid",
            format!("一次最多测试 {MAX_URLS} 个端点"),
        ));
    }
    Ok(())
}

/// Probes every URL with the shared read-only probe. Order is preserved so
/// the caller can align results with the list it sent.
#[tauri::command]
pub(crate) async fn test_claude_endpoints(
    urls: Vec<String>,
) -> Result<Vec<ClaudeEndpointLatency>, CommandError> {
    validate(&urls)?;
    blocking(move || {
        let mut results: Vec<Option<ClaudeEndpointLatency>> = vec![None; urls.len()];
        for chunk in urls
            .iter()
            .enumerate()
            .collect::<Vec<_>>()
            .chunks(MAX_CONCURRENCY)
        {
            let outcomes: Vec<(usize, ClaudeEndpointLatency)> = std::thread::scope(|scope| {
                let handles: Vec<_> = chunk
                    .iter()
                    .map(|(index, url)| {
                        let url = (*url).clone();
                        let index = *index;
                        scope.spawn(move || (index, probe_one(url)))
                    })
                    .collect();
                handles
                    .into_iter()
                    .map(|handle| handle.join().expect("probe thread"))
                    .collect()
            });
            for (index, outcome) in outcomes {
                results[index] = Some(outcome);
            }
        }
        Ok(results.into_iter().flatten().collect())
    })
    .await
}

fn probe_one(url: String) -> ClaudeEndpointLatency {
    let trimmed = url.trim().to_string();
    if trimmed.is_empty() {
        return ClaudeEndpointLatency {
            url,
            result: None,
            error: Some("端点地址不能为空".into()),
        };
    }
    match crate::probe::probe(&trimmed) {
        Ok(result) => ClaudeEndpointLatency {
            url: trimmed,
            result: Some(result),
            error: None,
        },
        Err(error) => ClaudeEndpointLatency {
            url: trimmed,
            result: None,
            error: Some(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_rejects_empty_and_oversized_batches() {
        assert_eq!(
            validate(&[]).unwrap_err().code,
            "claude-endpoint-test-invalid"
        );
        let many: Vec<String> = (0..=MAX_URLS).map(|i| format!("http://h{i}")).collect();
        assert_eq!(
            validate(&many).unwrap_err().code,
            "claude-endpoint-test-invalid"
        );
        assert!(validate(&["http://127.0.0.1:1".to_string()]).is_ok());
    }

    #[test]
    fn a_blank_or_malformed_url_is_reported_in_place_without_a_network_call() {
        let blank = probe_one("   ".into());
        assert!(blank.result.is_none());
        assert_eq!(blank.error.as_deref(), Some("端点地址不能为空"));
        let malformed = probe_one("ftp://relay.internal".into());
        assert!(malformed.result.is_none());
        assert!(malformed.error.is_some());
    }

    #[test]
    fn results_keep_the_input_order_across_concurrent_chunks() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let address = server.server_addr().to_string();
        let count = MAX_CONCURRENCY + 2;
        let task = std::thread::spawn(move || {
            for _ in 0..count {
                if let Ok(Some(request)) = server.recv_timeout(std::time::Duration::from_secs(10)) {
                    let _ = request.respond(tiny_http::Response::from_string("ok"));
                }
            }
        });
        let urls: Vec<String> = (0..count)
            .map(|i| format!("http://{address}/{i}"))
            .collect();
        let outcomes = tauri::async_runtime::block_on(test_claude_endpoints(urls.clone())).unwrap();
        task.join().unwrap();
        let echoed: Vec<&str> = outcomes.iter().map(|o| o.url.as_str()).collect();
        assert_eq!(echoed, urls.iter().map(String::as_str).collect::<Vec<_>>());
        assert!(outcomes
            .iter()
            .all(|o| o.result.as_ref().is_some_and(|r| r.status == Some(200))));
    }
}
