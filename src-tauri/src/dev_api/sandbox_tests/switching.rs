use super::*;

pub(super) fn claude_active_save(sandbox: &Sandbox, providers: &Providers) {
    let settings = &sandbox.settings;
    let secret = sandbox.secret.as_str();
    let api = &sandbox.api;
    let claude_a = &providers.claude_a;
    // Claude follows the same active save-and-apply transaction and restores
    // its original draft before the wider switch workflow continues.
    let claude_record = api.provider_record(claude_a);
    let mut claude_draft = draft_from_profile(claude_a);
    claude_draft["model"] = json!("claude-a-temporary");
    let claude_save = api.prepare_edit(&claude_record, claude_draft);
    assert_eq!(claude_save["kind"], "saveAndApply");
    let claude_saved = api.commit_profile_save(&claude_save, true);
    assert_eq!(claude_saved["profile"]["model"], "claude-a-temporary");
    assert_eq!(
        serde_json::from_str::<Value>(&read(settings)).unwrap()["model"],
        "claude-a-temporary"
    );
    let claude_restore = api.prepare_edit(&claude_saved, draft_from_profile(claude_a));
    assert_eq!(claude_restore["kind"], "saveAndApply");
    api.commit_profile_save(&claude_restore, true);
    check_claude(settings, "a", secret);
}

pub(super) fn switch_and_undo(sandbox: &Sandbox, providers: &Providers) {
    let config = &sandbox.config;
    let auth = &sandbox.auth;
    let settings = &sandbox.settings;
    let secret = sandbox.secret.as_str();
    let secret_b = sandbox.secret_b.as_str();
    let oauth = sandbox.oauth.as_str();
    let api = &sandbox.api;
    let codex_b = &providers.codex_b;
    let claude_b = &providers.claude_b;
    let a_config = read(config);
    let a_auth = read(auth);
    let a_settings = read(settings);
    api.switch(codex_b, secret_b);
    api.switch(claude_b, secret_b);
    check_codex(config, auth, "b", secret_b, oauth, "high");
    check_claude(settings, "b", secret_b);
    assert!(!read(auth).contains(secret));
    assert!(!read(settings).contains(secret));

    for (client, profile) in [("codex", codex_b), ("claude", claude_b)] {
        let status = api.ok("config_status", json!({}));
        let status = status
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["app"] == client)
            .unwrap();
        assert_eq!(status["activeProfileId"], profile["id"]);
        assert_eq!(status["syntaxOk"], true);
        api.ok(
            "undo_last_switch",
            json!({"target":client,"confirmWrite":true}),
        );
    }
    assert_eq!(read(config), a_config);
    assert!(read(auth) == a_auth);
    assert!(read(settings) == a_settings);
}

pub(super) fn stale_parameter_preview(sandbox: &Sandbox, providers: &Providers) {
    let config = &sandbox.config;
    let settings = &sandbox.settings;
    let secret_b = sandbox.secret_b.as_str();
    let api = &sandbox.api;
    let codex_b = &providers.codex_b;
    let claude_b = &providers.claude_b;
    let a_config = read(&sandbox.config);
    // Editing B parameters invalidates B preview while A keeps its independent intent.
    let stale = api.preview(codex_b);
    let record = api.provider_record(codex_b);
    let mut draft = draft_from_profile(&record["profile"]);
    draft["parameters"]["settings"]["model_reasoning_effort"] =
        json!({"mode":"explicit","value":"xhigh"});
    let prepared = api.prepare_edit(&record, draft);
    assert_eq!(prepared["kind"], "saveOnly");
    api.commit_profile_save(&prepared, false);
    api.rejected(
        "execute_switch",
        Api::switch_args(codex_b, &stale, true),
        "preview-stale",
    );
    assert_eq!(read(config), a_config);
    api.switch(codex_b, secret_b);
    assert!(read(config).contains("model_reasoning_effort = \"xhigh\""));
    let stale = api.preview(claude_b);
    let changed = format!("{}\n", read(settings));
    write(settings, &changed);
    api.rejected(
        "execute_switch",
        Api::switch_args(claude_b, &stale, true),
        "external-change",
    );
    assert!(read(settings) == changed);
}
