//! Request-local provider failures shared by probes and gateway transports.
//! Bodies are bounded and redacted here; callers must not persist them in logs.

mod redact;

use reqwest::header::HeaderMap;
use serde::Serialize;
use serde_json::Value;
use std::error::Error;
use std::io::Read;

pub(crate) use redact::redact_text;
pub(crate) const MAX_DIAGNOSTIC_BODY_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProviderFailureKind {
    Dns,
    Tls,
    Network,
    Timeout,
    WebsocketUnsupported,
    Endpoint,
    Authentication,
    RequestParameters,
    ModelNotFound,
    RateLimit,
    Upstream,
    ResponseParse,
    StreamParse,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderDiagnostic {
    pub(crate) kind: ProviderFailureKind,
    pub(crate) endpoint: String,
    pub(crate) transport: &'static str,
    pub(crate) status: Option<u16>,
    pub(crate) request_id: Option<String>,
    /// Upstream throttling guidance is safe to expose and must survive the
    /// gateway's protocol-specific error envelope.
    pub(crate) retry_after: Option<String>,
    pub(crate) body: Option<String>,
    pub(crate) body_truncated: bool,
    pub(crate) message: String,
}

impl ProviderDiagnostic {
    pub(crate) fn new(kind: ProviderFailureKind, endpoint: &str, message: &str) -> Self {
        Self {
            kind,
            endpoint: redact::endpoint(endpoint),
            transport: "http",
            status: None,
            request_id: None,
            retry_after: None,
            body: None,
            body_truncated: false,
            message: message.to_string(),
        }
    }

    pub(crate) fn summary(&self) -> String {
        let mut parts = vec![self.message.clone(), self.endpoint.clone()];
        if let Some(status) = self.status {
            parts.push(format!("HTTP {status}"));
        }
        if let Some(id) = &self.request_id {
            parts.push(format!("request id: {id}"));
        }
        let mut output = parts.join("；");
        if let Some(body) = &self.body {
            output.push_str("\n上游响应：");
            output.push_str(body);
        }
        if self.body_truncated {
            output.push_str("\n（上游响应已截断）");
        }
        output
    }

    pub(crate) fn error_value(&self) -> Value {
        serde_json::json!({
            "type": "provider_error",
            "code": self.kind,
            "message": self.summary(),
            "diagnostic": self,
        })
    }
}

pub(crate) fn request_id(headers: &HeaderMap) -> Option<String> {
    [
        "x-request-id",
        "request-id",
        "request_id",
        "x-amzn-requestid",
        "x-amz-request-id",
        "x-ms-request-id",
        "cf-ray",
    ]
    .into_iter()
    .find_map(|name| {
        headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    })
}

pub(crate) fn http_diagnostic(
    endpoint: &str,
    status: u16,
    headers: &HeaderMap,
    body: &[u8],
    truncated: bool,
    secrets: &[&str],
) -> ProviderDiagnostic {
    let limited = &body[..body.len().min(MAX_DIAGNOSTIC_BODY_BYTES + 1)];
    let text = String::from_utf8_lossy(limited);
    let truncated = truncated || body.len() > MAX_DIAGNOSTIC_BODY_BYTES;
    let kind = http_failure_kind(status, &text);
    let mut result = ProviderDiagnostic::new(kind, endpoint, failure_message(kind));
    result.endpoint = redact_text(&result.endpoint, secrets);
    result.status = Some(status);
    result.request_id = request_id(headers).map(|value| redact_text(&value, secrets));
    result.retry_after = headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| redact_text(value, secrets));
    let mut sanitized = redact::body(&text, secrets, truncated);
    let mut boundary = sanitized.len().min(MAX_DIAGNOSTIC_BODY_BYTES);
    while !sanitized.is_char_boundary(boundary) {
        boundary -= 1;
    }
    result.body_truncated = truncated || sanitized.len() > boundary;
    sanitized.truncate(boundary);
    result.body = (!limited.is_empty()).then_some(sanitized);
    result
}

/// Inspect status and headers before decoding: even a broken or non-UTF-8
/// error body remains an HTTP failure with the original endpoint and id.
pub(crate) fn read_http_diagnostic(
    mut response: reqwest::blocking::Response,
    secrets: &[&str],
) -> ProviderDiagnostic {
    let endpoint = response.url().to_string();
    let status = response.status().as_u16();
    let headers = response.headers().clone();
    let mut body = Vec::new();
    let read = response
        .by_ref()
        .take(MAX_DIAGNOSTIC_BODY_BYTES as u64 + 1)
        .read_to_end(&mut body);
    let mut diagnostic = http_diagnostic(&endpoint, status, &headers, &body, false, secrets);
    if read.is_err() {
        diagnostic.message.push_str("；读取上游错误响应时连接中断");
        diagnostic.body_truncated = true;
    }
    diagnostic
}

/// One transport diagnostic for both request failures and response-body read
/// failures. `reqwest::Error` contributes its exact timeout fact; all other
/// transport errors use the same causal-chain classifier.
pub(crate) fn network_diagnostic(
    endpoint: &str,
    error: &(dyn Error + 'static),
) -> ProviderDiagnostic {
    let kind = error
        .downcast_ref::<reqwest::Error>()
        .filter(|request_error| request_error.is_timeout())
        .map(|_| ProviderFailureKind::Timeout)
        .unwrap_or_else(|| network_failure_kind(error));
    ProviderDiagnostic::new(kind, endpoint, failure_message(kind))
}

pub(crate) fn network_failure_kind(error: &dyn Error) -> ProviderFailureKind {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(error) = source {
        text.push(' ');
        text.push_str(&error.to_string());
        source = error.source();
    }
    let text = text.to_ascii_lowercase();
    let contains = |needles: &[&str]| needles.iter().any(|needle| text.contains(needle));
    if contains(&["timed out", "timeout", "time out"]) {
        ProviderFailureKind::Timeout
    } else if contains(&[
        "dns",
        "failed to lookup",
        "name or service not known",
        "nodename nor servname",
        "name resolution",
        "no such host",
    ]) {
        ProviderFailureKind::Dns
    } else if contains(&[
        "certificate",
        "invalid peer",
        "unknown issuer",
        "tls",
        "ssl",
    ]) {
        ProviderFailureKind::Tls
    } else {
        ProviderFailureKind::Network
    }
}

fn http_failure_kind(status: u16, body: &str) -> ProviderFailureKind {
    use ProviderFailureKind::*;
    match status {
        401 | 403 => Authentication,
        429 => RateLimit,
        400 | 404 | 422 if is_model_failure(body) => ModelNotFound,
        400 | 409 | 422 => RequestParameters,
        404 | 405 | 410 | 300..=399 => Endpoint,
        _ => Upstream,
    }
}

fn is_model_failure(body: &str) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    let error = value.get("error").unwrap_or(&value);
    for name in ["code", "type"] {
        if error.get(name).and_then(Value::as_str).is_some_and(|code| {
            matches!(
                code,
                "model_not_found" | "invalid_model" | "model_not_available" | "unknown_model"
            )
        }) {
            return true;
        }
    }
    let message = error
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_lowercase();
    (message.contains("model")
        && [
            "not found",
            "does not exist",
            "not exist",
            "unknown model",
            "not available",
        ]
        .iter()
        .any(|phrase| message.contains(phrase)))
        || (message.contains("模型") && message.contains("不存在"))
}

fn failure_message(kind: ProviderFailureKind) -> &'static str {
    use ProviderFailureKind::*;
    match kind {
        Dns => "域名解析失败（DNS）",
        Tls => "TLS 握手或证书校验失败",
        Network => "连接上游服务失败",
        Timeout => "上游请求超时",
        WebsocketUnsupported => "上游不支持 Responses WebSocket",
        Endpoint => "上游 API endpoint 路径无效",
        Authentication => "上游认证失败",
        RequestParameters => "上游拒绝了请求参数",
        ModelNotFound => "上游模型不存在或不可用",
        RateLimit => "上游请求限流",
        Upstream => "Provider 上游错误",
        ResponseParse => "上游响应解析失败",
        StreamParse => "上游流式响应解析失败",
    }
}

#[cfg(test)]
mod tests;
