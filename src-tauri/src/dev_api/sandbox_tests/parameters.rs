use super::*;

pub(super) fn active_parameter_save(sandbox: &Sandbox, providers: &Providers) {
    let api = &sandbox.api;
    let before_b = api.provider_record(&providers.codex_b);
    let active = api.provider_record(&providers.codex_a);
    let mut draft = draft_from_profile(&active["profile"]);
    draft["parameters"]["settings"]["model_reasoning_effort"] =
        json!({"mode":"explicit","value":"xhigh"});
    let before_config = read(&sandbox.config);
    let before_auth = read(&sandbox.auth);
    let prepared = api.prepare_edit(&active, draft);
    assert_eq!(prepared["kind"], "saveAndApply");
    assert_eq!(read(&sandbox.config), before_config);
    let saved = api.commit_profile_save(&prepared, true);
    assert!(read(&sandbox.config).contains("model_reasoning_effort = \"xhigh\""));
    assert_eq!(read(&sandbox.auth), before_auth);
    assert_eq!(api.provider_record(&providers.codex_b), before_b);
    let restore = api.prepare_edit(&saved, draft_from_profile(&providers.codex_a));
    assert_eq!(restore["kind"], "saveAndApply");
    api.commit_profile_save(&restore, true);
    check_codex(
        &sandbox.config,
        &sandbox.auth,
        "a",
        &sandbox.secret,
        &sandbox.oauth,
        "high",
    );
}

pub(super) fn client_settings_invalidate_preview(sandbox: &Sandbox, providers: &Providers) {
    let api = &sandbox.api;
    let before = read(&sandbox.config);
    let preview = api.preview(&providers.codex_b);
    api.save_client("codex", "approval_policy", json!("never"));
    api.rejected(
        "execute_switch",
        Api::switch_args(&providers.codex_b, &preview, true),
        "preview-stale",
    );
    assert_eq!(read(&sandbox.config), before);
    api.switch(&providers.codex_b, &sandbox.secret_b);
    assert!(read(&sandbox.config).contains("approval_policy = \"never\""));
    api.save_client("codex", "approval_policy", json!("on-request"));
}

pub(super) fn automatic_values_remove_previous_provider_parameters(
    sandbox: &Sandbox,
    providers: &Providers,
) {
    let api = &sandbox.api;
    for (app, profile, key) in [
        ("codex", &providers.codex_b, "model_reasoning_effort"),
        ("claude", &providers.claude_b, "effortLevel"),
    ] {
        let record = api.provider_record(profile);
        let mut draft = draft_from_profile(&record["profile"]);
        draft["parameters"]["settings"][key] = json!({"mode":"automatic"});
        let prepared = api.prepare_edit(&record, draft);
        api.commit_profile_save(&prepared, prepared["kind"] == "saveAndApply");
        api.switch(profile, &sandbox.secret_b);
        if app == "codex" {
            assert!(!read(&sandbox.config).contains("model_reasoning_effort"));
        } else {
            let settings: Value = serde_json::from_str(&read(&sandbox.settings)).unwrap();
            assert!(settings.get("effortLevel").is_none());
        }
    }
    api.switch(&providers.codex_a, &sandbox.secret);
    api.switch(&providers.claude_a, &sandbox.secret);
    check_codex(
        &sandbox.config,
        &sandbox.auth,
        "a",
        &sandbox.secret,
        &sandbox.oauth,
        "high",
    );
    check_claude(&sandbox.settings, "a", &sandbox.secret);
}

pub(super) fn subagent_runtime_settings_are_independent(sandbox: &Sandbox, providers: &Providers) {
    let api = &sandbox.api;
    let before = read(&sandbox.config);
    let snapshot = api.ok("get_codex_subagent_settings", json!({}));
    assert_eq!(snapshot["settings"]["enabled"]["mode"], "automatic");
    let settings = json!({
        "enabled": { "mode": "explicit", "value": true },
        "maxConcurrentThreadsPerSession": { "mode": "explicit", "value": 3 },
        "interruptMessage": { "mode": "explicit", "value": false },
    });
    let preview = api.ok(
        "preview_codex_subagent_settings_command",
        json!({ "settings": settings, "expectedHash": snapshot["configHash"] }),
    );
    assert_eq!(read(&sandbox.config), before);
    api.rejected(
        "apply_codex_subagent_settings",
        json!({
            "plan": {
                "settings": settings,
                "expectedHash": snapshot["configHash"],
                "expectedTargetExisted": snapshot["fileExists"],
                "renderedHash": preview["renderedHash"],
            },
            "confirmWrite": false,
        }),
        "write-not-confirmed",
    );
    let saved = api.ok(
        "apply_codex_subagent_settings",
        json!({
            "plan": {
                "settings": settings,
                "expectedHash": snapshot["configHash"],
                "expectedTargetExisted": snapshot["fileExists"],
                "renderedHash": preview["renderedHash"],
            },
            "confirmWrite": true,
        }),
    );
    assert_eq!(saved["settings"]["enabled"]["value"], true);
    let written = read(&sandbox.config);
    for expected in [
        "enabled = true",
        "max_concurrent_threads_per_session = 3",
        "interrupt_message = false",
        "[mcp_servers.audit]",
    ] {
        assert!(written.contains(expected), "missing {expected}");
    }
    api.switch(&providers.codex_a, &sandbox.secret);
    assert!(
        read(&sandbox.config).contains("enabled = true"),
        "provider projection must preserve separately owned runtime keys"
    );
}
