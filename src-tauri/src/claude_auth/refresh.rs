//! Preserve a rotated refresh credential across a recoverable local persistence failure.

use super::{contracts::AccountFile, oauth, store, ClaudeAccount, ClaudeAuth, Runtime};

pub(super) struct PendingRefresh {
    source_revision: String,
    refreshed: ClaudeAccount,
}

fn credential_revision(account: &ClaudeAccount) -> String {
    asb_switch::sha256_hex(&serde_json::json!({"provider":account.provider,"access":account.access_token,
        "refresh":account.refresh_token,"expires":account.expires_at_ms,"upstream":account.upstream_account_id}).to_string())
}

pub(super) fn resolve(
    auth: &ClaudeAuth,
    runtime: &mut Runtime,
    file: &mut AccountFile,
    index: usize,
    file_hash: &str,
    now: i64,
) -> Result<(), String> {
    let original = &file.accounts[index];
    let id = original.id.clone();
    let revision = credential_revision(original);
    let candidate = runtime
        .pending_refresh
        .get(&id)
        .filter(|pending| pending.source_revision == revision)
        .map(|pending| pending.refreshed.clone());
    let mut refreshed = match candidate {
        Some(refreshed) => refreshed,
        None if original
            .expires_at_ms
            .is_some_and(|expiry| expiry > now.saturating_add(60_000)) =>
        {
            runtime.pending_refresh.remove(&id);
            return Ok(());
        }
        None => oauth::refresh(original, &auth.token_url(original.provider)?, now)?,
    };
    // Label edits do not invalidate a still-uncommitted rotated credential.
    refreshed.label = original.label.clone();
    runtime.pending_refresh.insert(
        id.clone(),
        PendingRefresh {
            source_revision: revision,
            refreshed: refreshed.clone(),
        },
    );
    file.accounts[index] = refreshed;
    if let Err(error) = store::save(&auth.root, file, file_hash) {
        return Err(format!(
            "{error}；刷新后的凭据暂存于当前进程，请修复后重试，退出前完成保存"
        ));
    }
    runtime.pending_refresh.remove(&id);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claude_auth::{quota::selection, TestEndpoints};
    use asb_core::claude_auth::ClaudeAuthProvider;
    use std::time::Duration;

    #[test]
    fn a_rotated_token_survives_a_blocked_account_file_and_retries_without_refreshing_again() {
        let dir = tempfile::tempdir().unwrap();
        let path = store::path(dir.path());
        let saved = dir.path().join("original-accounts.json");
        assert!(path.starts_with(dir.path()) && saved.starts_with(dir.path()));
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let base = format!("http://{}", server.server_addr());
        let auth = ClaudeAuth::with_endpoints(
            dir.path(),
            TestEndpoints {
                github_api: base.clone(),
                oauth_token: base.clone(),
                upstream: base,
            },
        );
        auth.save_account(
            ClaudeAccount {
                id: "account".into(),
                label: "label".into(),
                provider: ClaudeAuthProvider::CodexOauth,
                access_token: "expired-access".into(),
                refresh_token: Some("old-refresh".into()),
                expires_at_ms: Some(1),
                upstream_account_id: Some("workspace".into()),
                github_domain: None,
            },
            &auth.view().unwrap().file_hash,
            true,
        )
        .unwrap();
        let block_path = path.clone();
        let backup = saved.clone();
        let task = std::thread::spawn(move || {
            let request = server
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .unwrap();
            std::fs::rename(&block_path, &backup).unwrap();
            std::fs::create_dir(&block_path).unwrap();
            request.respond(tiny_http::Response::from_string(serde_json::json!({"access_token":"new-access","refresh_token":"rotated-refresh","expires_in":3600}).to_string())).unwrap();
        });
        let options = selection(ClaudeAuthProvider::CodexOauth, Some("account"));
        assert!(auth
            .resolve(&options)
            .err()
            .expect("blocked store")
            .contains("暂存"));
        task.join().unwrap();
        std::fs::remove_dir(&path).unwrap();
        std::fs::rename(&saved, &path).unwrap();
        let resolved = auth.resolve(&options).unwrap().unwrap();
        assert_eq!(resolved.access_token, "new-access");
        assert!(std::fs::read_to_string(path)
            .unwrap()
            .contains("rotated-refresh"));
    }
}
