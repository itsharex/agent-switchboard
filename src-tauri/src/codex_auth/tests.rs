use super::{bindings, contracts::*, identity, manager, store};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::json;
use std::fs;

fn jwt(value: serde_json::Value) -> String {
    format!("h.{}.s", URL_SAFE_NO_PAD.encode(value.to_string()))
}
fn tokens(user: &str, workspace: &str) -> Tokens {
    Tokens {
        id_token: jwt(
            json!({"sub":user,"email":format!("{user}@example.test"),"https://api.openai.com/auth":{"chatgpt_account_id":workspace,"chatgpt_plan_type":"team"}}),
        ),
        access_token: jwt(json!({"exp":4102444800_i64})),
        refresh_token: format!("refresh-secret-{user}"),
    }
}
fn insert(root: &std::path::Path, user: &str, workspace: &str) -> String {
    let _guard = store::lock().unwrap();
    manager::upsert(
        root,
        tokens(user, workspace),
        None,
        chrono::Utc::now().timestamp_millis(),
        None,
    )
    .unwrap()
}
fn native(tokens: &Tokens, at: i64) -> String {
    json!({"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{
        "id_token":tokens.id_token,"access_token":tokens.access_token,"refresh_token":tokens.refresh_token},
        "last_refresh":chrono::DateTime::from_timestamp_millis(at).unwrap().to_rfc3339()}).to_string()
}
#[test]
fn different_users_in_one_workspace_remain_distinct_and_renderer_never_gets_tokens() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let first = insert(root, "first", "shared");
    let second = insert(root, "second", "shared");
    assert_ne!(first, second);
    let view = manager::list_accounts(root).unwrap();
    let encoded = serde_json::to_string(&view).unwrap();
    for secret in [
        "refresh-secret-",
        "accessToken",
        "idToken",
        "refreshToken",
        "shared",
    ] {
        assert!(!encoded.contains(secret), "{secret}");
    }
    assert!(!format!("{:?}", tokens("first", "shared")).contains("secret"));
    assert_eq!(view.accounts.len(), 2);
    assert!(view.accounts.iter().all(|a| !a.is_default));
}
#[test]
fn native_binding_and_explicit_account_do_not_drift_with_default() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let first = insert(root, "one", "work");
    let second = insert(root, "two", "work");
    let profile = uuid::Uuid::new_v4().to_string();
    assert_eq!(
        bindings::binding(root, &profile).unwrap(),
        AccountSelection::Native
    );
    let view = manager::list_accounts(root).unwrap();
    let view = manager::set_default(root, Some(&first), &view.revision).unwrap();
    let view = bindings::set_binding(
        root,
        &profile,
        AccountSelection::Account { id: first.clone() },
        &view.revision,
    )
    .unwrap();
    let view = manager::set_default(root, Some(&second), &view.revision).unwrap();
    assert_eq!(
        bindings::binding(root, &profile).unwrap(),
        AccountSelection::Account { id: first.clone() }
    );
    assert!(manager::delete_account(root, &first, &view.revision)
        .unwrap_err()
        .contains("解绑"));
    let view =
        bindings::set_binding(root, &profile, AccountSelection::Native, &view.revision).unwrap();
    manager::delete_account(root, &first, &view.revision).unwrap();
    assert_eq!(
        bindings::binding(root, &profile).unwrap(),
        AccountSelection::Native
    );
}
#[test]
fn stale_revisions_and_deleted_reauthentication_targets_never_recreate_accounts() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let before = manager::list_accounts(root).unwrap();
    let id = insert(root, "one", "work");
    assert!(manager::set_default(root, Some(&id), &before.revision).is_err());
    let view = manager::list_accounts(root).unwrap();
    manager::delete_account(root, &id, &view.revision).unwrap();
    let _guard = store::lock().unwrap();
    assert!(
        manager::upsert(root, tokens("one", "work"), Some(&id), 1, None)
            .unwrap_err()
            .contains("已被删除")
    );
    assert!(store::load(root).unwrap().0.accounts.is_empty());
}
#[test]
fn reauthentication_preserves_id_and_rejects_a_different_user_or_workspace() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let id = insert(root, "one", "work");
    let _guard = store::lock().unwrap();
    for token in [tokens("other", "work"), tokens("one", "other")] {
        assert!(manager::upsert(root, token, Some(&id), 1, None).is_err());
    }
    assert_eq!(
        manager::upsert(root, tokens("one", "work"), Some(&id), 1, None).unwrap(),
        id
    );
    assert_eq!(store::load(root).unwrap().0.accounts[0].generation, 2);
}
#[test]
fn cli_refresh_is_adopted_only_for_the_same_identity_and_never_downgrades() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let id = insert(root, "one", "work");
    let auth = root.join("isolated-auth.json");
    let _guard = store::lock().unwrap();
    let account = store::load(root).unwrap().0.accounts.remove(0);
    let newer = account.updated_at + 5000;
    let mut rotated = tokens("one", "work");
    rotated.refresh_token = "new-generation".into();
    fs::write(&auth, native(&rotated, newer)).unwrap();
    let adopted =
        manager::valid_account_with(root, Some(&id), &auth, newer, |_| panic!("still valid"))
            .unwrap();
    assert_eq!(adopted.tokens.refresh_token, "new-generation");
    assert_eq!(adopted.generation, 2);
    fs::write(&auth, native(&tokens("other", "work"), newer + 1)).unwrap();
    let account =
        manager::valid_account_with(root, Some(&id), &auth, newer, |_| panic!("still valid"))
            .unwrap();
    assert_eq!(account.tokens.refresh_token, "new-generation");
    fs::write(&auth, native(&tokens("one", "work"), newer - 1)).unwrap();
    let account =
        manager::valid_account_with(root, Some(&id), &auth, newer, |_| panic!("still valid"))
            .unwrap();
    assert_eq!(account.generation, 2);
}
#[test]
fn expired_tokens_refresh_once_and_persist_without_writing_live_auth() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let id = insert(root, "one", "work");
    let auth = root.join("native-auth.json");
    let _guard = store::lock().unwrap();
    let (mut file, revision) = store::load(root).unwrap();
    file.accounts[0].expires_at = 1;
    store::save(root, &file, &revision).unwrap();
    let account = manager::valid_account_with(root, Some(&id), &auth, 100_000, |token| {
        let mut rotated = token.clone();
        rotated.refresh_token = "rotated".into();
        Ok(rotated)
    })
    .unwrap();
    assert_eq!(account.generation, 2);
    assert_eq!(account.tokens.refresh_token, "rotated");
    assert!(!auth.exists());
    manager::valid_account_with(root, Some(&id), &auth, 100_000, |_| {
        panic!("must use persisted token")
    })
    .unwrap();
}
#[test]
fn malformed_identity_and_apikey_native_auth_are_rejected_without_writes() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let auth = root.join("auth.json");
    let revision = manager::list_accounts(root).unwrap().revision;
    for text in ["broken", "{}", r#"{"OPENAI_API_KEY":"key"}"#] {
        fs::write(&auth, text).unwrap();
        assert!(manager::import_native(root, &auth, &revision).is_err());
        assert_eq!(fs::read_to_string(&auth).unwrap(), text);
        assert!(!root.join("codex/accounts.json").exists());
    }
    let mut token = tokens("one", "work");
    token.id_token = jwt(json!({"sub":"one"}));
    assert!(identity::identity(&token).is_err());
}
#[test]
fn corrupted_account_store_is_not_silently_replaced() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::create_dir(root.join("codex")).unwrap();
    let path = root.join("codex/accounts.json");
    fs::write(&path, "broken").unwrap();
    assert!(manager::list_accounts(root).is_err());
    assert_eq!(fs::read_to_string(path).unwrap(), "broken");
}

#[test]
fn shared_native_refresh_is_synchronized_without_changing_api_key_mode_or_unowned_keys() {
    let directory = tempfile::tempdir().unwrap(); let root = directory.path();
    let id = insert(root, "one", "work"); let auth = root.join("auth.json");
    let _guard = store::lock().unwrap();
    let (mut file, revision) = store::load(root).unwrap();
    file.accounts[0].expires_at = 1;
    let mut live: serde_json::Value = serde_json::from_str(&native(&file.accounts[0].tokens, 1)).unwrap();
    live["auth_mode"] = json!("apikey"); live["OPENAI_API_KEY"] = json!("third-party-key"); live["host_note"] = json!("keep"); live["tokens"]["native_extra"] = json!("also keep");
    fs::write(&auth, live.to_string()).unwrap(); store::save(root, &file, &revision).unwrap();
    let account = manager::valid_account_with(root, Some(&id), &auth, 100_000, |token| {
        let mut next = token.clone(); next.refresh_token = "rotated-shared-chain".into(); Ok(next)
    }).unwrap();
    assert!(account.native_sync.is_none()); assert!(account.native_sync_error.is_none());
    let after: serde_json::Value = serde_json::from_str(&fs::read_to_string(&auth).unwrap()).unwrap();
    assert_eq!(after["tokens"]["refresh_token"], "rotated-shared-chain");
    assert_eq!(after["auth_mode"], "apikey"); assert_eq!(after["OPENAI_API_KEY"], "third-party-key"); assert_eq!(after["host_note"], "keep"); assert_eq!(after["tokens"]["native_extra"], "also keep");
    assert!(!root.join("config.toml").exists());
    assert!(!asb_switch::list_backups(&asb_switch::FsIo, &root.join("codex/auth-backups")).is_empty());
}
#[test]
fn refresh_never_replaces_a_concurrent_native_login_and_retains_the_fresh_managed_tokens() {
    let directory = tempfile::tempdir().unwrap(); let root = directory.path();
    let id = insert(root, "one", "work"); let auth = root.join("auth.json");
    let _guard = store::lock().unwrap(); let (mut file, revision) = store::load(root).unwrap();
    file.accounts[0].expires_at = 1; fs::write(&auth, native(&file.accounts[0].tokens, 1)).unwrap(); store::save(root, &file, &revision).unwrap();
    let external = native(&tokens("other", "other-work"), 200_000);
    let refreshed = manager::valid_account_with(root, Some(&id), &auth, 100_000, |token| {
        fs::write(&auth, &external).unwrap();
        let mut next = token.clone(); next.refresh_token = "fresh-managed-token".into(); Ok(next)
    }).unwrap();
    assert_eq!(refreshed.tokens.refresh_token, "fresh-managed-token");
    assert!(refreshed.native_sync.is_none()); assert_eq!(fs::read_to_string(&auth).unwrap(), external);
}
#[test]
fn native_sync_retries_from_its_persisted_intent_after_a_lock_failure() {
    let directory = tempfile::tempdir().unwrap(); let root = directory.path();
    let id = insert(root, "one", "work"); let auth = root.join("auth.json");
    let _guard = store::lock().unwrap(); let (mut file, revision) = store::load(root).unwrap();
    file.accounts[0].expires_at = 1; fs::write(&auth, native(&file.accounts[0].tokens, 1)).unwrap(); store::save(root, &file, &revision).unwrap();
    let config = root.join("config.toml");
    assert!(matches!(asb_switch::lockfile::acquire(&asb_switch::FsIo, &config, "fixture"), asb_switch::lockfile::AcquireOutcome::Acquired));
    let first = manager::valid_account_with(root, Some(&id), &auth, 100_000, |token| { let mut next = token.clone(); next.refresh_token = "rotated-once".into(); Ok(next) }).unwrap();
    assert!(first.native_sync.is_some()); assert!(first.native_sync_error.is_some());
    asb_switch::lockfile::release(&asb_switch::FsIo, &config).unwrap();
    let retried = manager::valid_account_with(root, Some(&id), &auth, 100_000, |_| panic!("must reuse the committed fresh tokens")).unwrap();
    assert!(retried.native_sync.is_none()); assert!(retried.native_sync_error.is_none());
    assert_eq!(serde_json::from_str::<serde_json::Value>(&fs::read_to_string(auth).unwrap()).unwrap()["tokens"]["refresh_token"], "rotated-once");
}
