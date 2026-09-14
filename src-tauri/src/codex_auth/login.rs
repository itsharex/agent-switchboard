use super::{
    contracts::{AccountsView, Tokens},
    manager, store,
};
use crate::official_login::codex;
use serde::Serialize;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Instant,
};

#[derive(Clone)]
struct Session {
    id: String,
    device_id: String,
    user_code: String,
    started: Instant,
    reauthenticate: Option<String>,
}
fn sessions() -> &'static Mutex<HashMap<PathBuf, Session>> {
    static SESSIONS: OnceLock<Mutex<HashMap<PathBuf, Session>>> = OnceLock::new();
    SESSIONS.get_or_init(|| Mutex::new(HashMap::new()))
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AccountLoginStart {
    pub session_id: String,
    pub user_code: String,
    pub verification_url: String,
}
#[derive(Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub(crate) enum LoginPoll {
    Pending,
    Completed { accounts: AccountsView },
    Failed { message: String },
}

pub(crate) fn start_login(
    root: &Path,
    reauthenticate: Option<String>,
) -> Result<AccountLoginStart, String> {
    let _guard = store::lock()?;
    let mut sessions = sessions().lock().map_err(|_| "Codex 登录会话锁不可用")?;
    if sessions
        .get(root)
        .is_some_and(|s| s.started.elapsed().as_secs() < 600)
    {
        return Err("已有进行中的 Codex 账号登录，请先完成或取消".into());
    }
    if let Some(id) = &reauthenticate {
        if !store::load(root)?.0.accounts.iter().any(|a| &a.id == id) {
            return Err("重认证目标账号不存在".into());
        }
    }
    let device = codex::request_device_code(&codex::CodexOAuthEndpoints::default())?;
    let id = uuid::Uuid::new_v4().to_string();
    let result = AccountLoginStart {
        session_id: id.clone(),
        user_code: device.user_code.clone(),
        verification_url: codex::DEVICE_VERIFICATION_URL.into(),
    };
    sessions.insert(
        root.to_path_buf(),
        Session {
            id,
            device_id: device.device_auth_id,
            user_code: device.user_code,
            started: Instant::now(),
            reauthenticate,
        },
    );
    Ok(result)
}
pub(crate) fn cancel_login(root: &Path, session_id: &str) -> Result<(), String> {
    let mut sessions = sessions().lock().map_err(|_| "Codex 登录会话锁不可用")?;
    if sessions
        .get(root)
        .is_some_and(|session| session.id == session_id)
    {
        sessions.remove(root);
    }
    Ok(())
}
pub(crate) fn poll_login(root: &Path, session_id: &str) -> Result<LoginPoll, String> {
    advance(root, session_id, |session| {
        codex::poll_login(
            &codex::CodexOAuthEndpoints::default(),
            &session.device_id,
            &session.user_code,
        )
    })
}
fn advance(
    root: &Path,
    session_id: &str,
    poll: impl FnOnce(&Session) -> Result<codex::CodexPollOutcome, String>,
) -> Result<LoginPoll, String> {
    let session = sessions()
        .lock()
        .map_err(|_| "Codex 登录会话锁不可用")?
        .get(root)
        .cloned()
        .ok_or("尚未开始 Codex 账号登录")?;
    if session.id != session_id {
        return Err("Codex 登录会话已被替换".into());
    }
    // No session/store lock is held during network I/O: cancellation and
    // account deletion must win before the response is committed.
    let result = if session.started.elapsed().as_secs() < 600 {
        poll(&session)
    } else {
        Err("Codex 账号登录已过期，请重新开始".into())
    };
    let _guard = store::lock()?;
    let mut sessions = sessions().lock().map_err(|_| "Codex 登录会话锁不可用")?;
    if sessions.get(root).map(|s| &s.id) != Some(&session.id) {
        return Ok(LoginPoll::Failed {
            message: "Codex 账号登录已取消或被替换".into(),
        });
    }
    match result {
        Ok(codex::CodexPollOutcome::Pending) => Ok(LoginPoll::Pending),
        Ok(codex::CodexPollOutcome::Completed { tokens, .. }) => {
            sessions.remove(root);
            let tokens = Tokens {
                id_token: tokens
                    .id_token
                    .ok_or("Codex 登录缺少 id_token，请重新认证")?,
                access_token: tokens.access_token,
                refresh_token: tokens.refresh_token,
            };
            manager::upsert(
                root,
                tokens,
                session.reauthenticate.as_deref(),
                chrono::Utc::now().timestamp_millis(),
                None,
            )?;
            let (file, revision) = store::load(root)?;
            Ok(LoginPoll::Completed {
                accounts: store::view(&file, revision),
            })
        }
        Err(message) => {
            sessions.remove(root);
            Ok(LoginPoll::Failed { message })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_during_network_poll_prevents_any_account_write() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        sessions().lock().unwrap().insert(
            root.to_path_buf(),
            Session {
                id: "session".into(),
                device_id: "device".into(),
                user_code: "code".into(),
                started: Instant::now(),
                reauthenticate: None,
            },
        );
        let result = advance(root, "session", |_| {
            cancel_login(root, "session").unwrap();
            Ok(codex::CodexPollOutcome::Completed {
                tokens: crate::official_login::credentials::CodexTokens {
                    id_token: Some("not-committed".into()),
                    access_token: "not-committed".into(),
                    refresh_token: "not-committed".into(),
                },
                account_id: None,
            })
        })
        .unwrap();
        assert!(matches!(result, LoginPoll::Failed { .. }));
        assert!(!root.join("codex/accounts.json").exists());
    }
    #[test]
    fn old_login_nonce_cannot_cancel_or_poll_the_replacement() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        sessions().lock().unwrap().insert(
            root.to_path_buf(),
            Session {
                id: "new-session".into(),
                device_id: "device".into(),
                user_code: "code".into(),
                started: Instant::now(),
                reauthenticate: None,
            },
        );
        cancel_login(root, "old-session").unwrap();
        assert_eq!(
            sessions().lock().unwrap().get(root).unwrap().id,
            "new-session"
        );
        assert!(advance(root, "old-session", |_| panic!(
            "must reject before the network request"
        ))
        .is_err());
        cancel_login(root, "new-session").unwrap();
    }
}
