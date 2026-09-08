use super::*;

pub(super) struct Providers {
    pub(super) codex_a: Value,
    pub(super) codex_b: Value,
    pub(super) claude_a: Value,
    pub(super) claude_b: Value,
}

pub(super) fn create(sandbox: &Sandbox) -> Providers {
    let secret = sandbox.secret.as_str();
    let secret_b = sandbox.secret_b.as_str();
    let api = &sandbox.api;
    // Client preferences remain global; every provider supplies its own complete parameters.
    let codex_client = api.save_client("codex", "approval_policy", json!("on-request"));
    assert_eq!(
        codex_client["settings"]["settings"]["approval_policy"]["value"],
        "on-request"
    );
    assert!(codex_client["settings"]["settings"]
        .get("model_reasoning_effort")
        .is_none());
    let codex_a = api.create("codex", "a", false, secret, "responses");
    let codex_b = api.create("codex", "b", false, secret_b, "responses");
    let claude_a = api.create("claude", "a", false, secret, "anthropicMessages");
    let claude_b = api.create("claude", "b", false, secret_b, "anthropicMessages");

    Providers {
        codex_a,
        codex_b,
        claude_a,
        claude_b,
    }
}

pub(super) fn inactive_saves(sandbox: &Sandbox, providers: &Providers) {
    let config = &sandbox.config;
    let auth = &sandbox.auth;
    let settings = &sandbox.settings;
    let initial_codex = sandbox.initial_codex;
    let initial_claude = sandbox.initial_claude;
    let initial_auth = sandbox.initial_auth.as_str();
    let api = &sandbox.api;
    let codex_a = &providers.codex_a;
    let codex_b = &providers.codex_b;
    // Every save path starts with the same preparation. A new profile and an
    // unchanged edit write neither client config nor a switch backup.
    let codex_a_record = api.provider_record(codex_a);
    let no_change = api.prepare_edit(&codex_a_record, draft_from_profile(codex_a));
    assert_eq!(no_change["kind"], "noChange");
    assert!(no_change["preview"].is_null());
    api.commit_profile_save(&no_change, false);

    // Editing an inactive provider's effective field persists only its own
    // profile. Restoring the original draft uses the same path again.
    let codex_b_record = api.provider_record(codex_b);
    let mut inactive_draft = draft_from_profile(codex_b);
    inactive_draft["model"] = json!("codex-b-temporary");
    let inactive_save = api.prepare_edit(&codex_b_record, inactive_draft);
    assert_eq!(inactive_save["kind"], "saveOnly");
    assert!(inactive_save["preview"].is_null());
    let inactive_saved = api.commit_profile_save(&inactive_save, false);
    assert_eq!(inactive_saved["profile"]["model"], "codex-b-temporary");
    let inactive_restore = api.prepare_edit(&inactive_saved, draft_from_profile(codex_b));
    assert_eq!(inactive_restore["kind"], "saveOnly");
    api.commit_profile_save(&inactive_restore, false);
    assert_eq!(read(config), initial_codex);
    assert_eq!(read(settings), initial_claude);
    assert!(read(auth) == initial_auth);
}

pub(super) fn preview_and_switch(sandbox: &Sandbox, providers: &Providers) {
    let config = &sandbox.config;
    let auth = &sandbox.auth;
    let secret = sandbox.secret.as_str();
    let oauth = sandbox.oauth.as_str();
    let initial_codex = sandbox.initial_codex;
    let initial_auth = sandbox.initial_auth.as_str();
    let api = &sandbox.api;
    let codex_a = &providers.codex_a;
    let before = api.preview(codex_a);
    assert!(!before.to_string().contains(oauth));
    assert!(
        before["content"]
            .as_str()
            .is_some_and(|content| content.contains("model_reasoning_effort = \"high\"")),
        "Codex preview must project the saved provider reasoning preference"
    );
    assert!(
        before["content"]
            .as_str()
            .is_some_and(|content| content.contains("web_search = \"live\"")),
        "Codex preview must project the saved provider web-search preference"
    );
    assert_eq!(read(config), initial_codex);
    assert!(read(auth) == initial_auth);
    api.rejected(
        "execute_switch",
        Api::switch_args(codex_a, &before, false),
        "write-not-confirmed",
    );
    assert_eq!(read(config), initial_codex);
    let codex_a_outcome = api.switch(codex_a, secret);
    assert!(
        codex_a_outcome["preview"]["changes"]
            .as_array()
            .is_some_and(|changes| changes.iter().any(|change| {
                change["key"] == "model_reasoning_effort" && change["after"] == "high"
            })),
        "the committed Codex projection must retain the previewed reasoning value"
    );
    assert_eq!(
        codex_a_outcome["finalHash"],
        asb_switch::sha256_hex(&read(config)),
        "the Codex file must still equal the executor's committed candidate"
    );
    check_codex(config, auth, "a", secret, oauth, "high");
}

pub(super) fn active_save(sandbox: &Sandbox, providers: &Providers) {
    let config = &sandbox.config;
    let auth = &sandbox.auth;
    let secret = sandbox.secret.as_str();
    let oauth = sandbox.oauth.as_str();
    let api = &sandbox.api;
    let codex_a = &providers.codex_a;
    // An active effective edit requires the explicit confirmation and is
    // applied atomically with the provider file. A rejected confirmation
    // leaves both client files untouched and consumes the old preparation.
    let active_record = api.provider_record(codex_a);
    let mut active_draft = draft_from_profile(codex_a);
    active_draft["model"] = json!("codex-a-temporary");
    let active_save = api.prepare_edit(&active_record, active_draft.clone());
    assert_eq!(active_save["kind"], "saveAndApply");
    assert!(active_save["preview"].is_object());
    let before_confirmed_save = read(config);
    let before_confirmed_auth = read(auth);
    api.rejected(
        "commit_profile_save",
        json!({"preparationId": active_save["preparationId"], "confirmWrite": false}),
        "write-not-confirmed",
    );
    api.rejected(
        "commit_profile_save",
        json!({"preparationId": active_save["preparationId"], "confirmWrite": true}),
        "profile-save-stale",
    );
    assert_eq!(read(config), before_confirmed_save);
    assert_eq!(read(auth), before_confirmed_auth);
    let active_save = api.prepare_edit(&active_record, active_draft);
    let active_saved = api.commit_profile_save(&active_save, true);
    assert_eq!(active_saved["profile"]["model"], "codex-a-temporary");
    assert!(read(config).contains("model = \"codex-a-temporary\""));
    check_active_profile(api, "codex", codex_a);
    let active_restore = api.prepare_edit(&active_saved, draft_from_profile(codex_a));
    assert_eq!(active_restore["kind"], "saveAndApply");
    api.commit_profile_save(&active_restore, true);
    check_codex(config, auth, "a", secret, oauth, "high");
}

pub(super) fn metadata_save(sandbox: &Sandbox, providers: &Providers) {
    let config = &sandbox.config;
    let auth = &sandbox.auth;
    let api = &sandbox.api;
    let codex_a = &providers.codex_a;
    // Metadata on an active provider is persisted without re-projecting the
    // client files.
    let metadata_record = api.provider_record(codex_a);
    let mut metadata_draft = draft_from_profile(codex_a);
    metadata_draft["notes"] = json!("metadata only");
    let metadata_save = api.prepare_edit(&metadata_record, metadata_draft);
    assert_eq!(metadata_save["kind"], "saveOnly");
    let before_metadata_config = read(config);
    let before_metadata_auth = read(auth);
    api.commit_profile_save(&metadata_save, false);
    assert_eq!(read(config), before_metadata_config);
    assert_eq!(read(auth), before_metadata_auth);
}
