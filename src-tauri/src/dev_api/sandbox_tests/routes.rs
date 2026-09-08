use super::*;

pub(super) fn cross_protocol(sandbox: &Sandbox) {
    let config = &sandbox.config;
    let auth = &sandbox.auth;
    let settings = &sandbox.settings;
    let api = &sandbox.api;
    // Every client can select each non-native upstream protocol. The real
    // command dispatcher must install a loopback-only endpoint and a distinct
    // local capability token; the configured upstream endpoint and credential
    // must never reach the client files or status projection.
    let codex_chat_secret = uuid::Uuid::new_v4().to_string();
    let codex_anthropic_secret = uuid::Uuid::new_v4().to_string();
    let claude_chat_secret = uuid::Uuid::new_v4().to_string();
    let claude_responses_secret = uuid::Uuid::new_v4().to_string();
    let codex_chat = api.create(
        "codex",
        "codex-chat",
        false,
        &codex_chat_secret,
        "chatCompletions",
    );
    let codex_anthropic = api.create(
        "codex",
        "codex-anthropic",
        false,
        &codex_anthropic_secret,
        "anthropicMessages",
    );
    let claude_chat = api.create(
        "claude",
        "claude-chat",
        false,
        &claude_chat_secret,
        "chatCompletions",
    );
    let claude_responses = api.create(
        "claude",
        "claude-responses",
        false,
        &claude_responses_secret,
        "responses",
    );
    for (profile, secret) in [
        (&codex_chat, &codex_chat_secret),
        (&codex_anthropic, &codex_anthropic_secret),
    ] {
        api.switch(profile, secret);
        check_routed_codex(config, auth, secret);
        check_active_profile(api, "codex", profile);
    }
    for (profile, secret) in [
        (&claude_chat, &claude_chat_secret),
        (&claude_responses, &claude_responses_secret),
    ] {
        api.switch(profile, secret);
        check_routed_claude(settings, secret);
        check_active_profile(api, "claude", profile);
    }
}

pub(super) fn return_to_original(sandbox: &Sandbox, providers: &Providers) {
    let config = &sandbox.config;
    let auth = &sandbox.auth;
    let secret = sandbox.secret.as_str();
    let oauth = sandbox.oauth.as_str();
    let api = &sandbox.api;
    let codex_a = &providers.codex_a;
    // Switching back to A restores A's own parameters after using B and routed providers.
    api.switch(codex_a, secret);
    check_codex(config, auth, "a", secret, oauth, "high");
    check_active_profile(api, "codex", codex_a);
    // Cloud restore replaces the whole profile store. It must not invalidate
    // the fingerprints of either active local route before their clients have
    // been switched away from the gateway.
    api.rejected(
        "restore_cloud_backup",
        json!({
            "accountPassword": "unused-in-sandbox",
            "backupPassword": "unused-in-sandbox",
            "confirmWrite": true,
        }),
        "gateway-route-active",
    );
}

pub(super) fn official_routes(sandbox: &Sandbox) {
    let root = &sandbox.root;
    let config = &sandbox.config;
    let auth = &sandbox.auth;
    let settings = &sandbox.settings;
    let secret = sandbox.secret.as_str();
    let oauth = sandbox.oauth.as_str();
    let api = &sandbox.api;
    // Official routes remove custom endpoints/keys while retaining host data and OAuth.
    let official_codex = api.create("codex", "official", true, secret, "responses");
    let official_claude = api.create("claude", "official", true, secret, "anthropicMessages");
    api.switch(&official_codex, secret);
    api.switch(&official_claude, secret);
    assert!(!read(config).contains("openai_base_url"));
    let auth_value: Value = serde_json::from_str(&read(auth)).unwrap();
    assert_eq!(auth_value["auth_mode"], "chatgpt");
    assert!(auth_value["tokens"]["access_token"] == oauth);
    assert!(auth_value["OPENAI_API_KEY"].is_null());
    let claude_value: Value = serde_json::from_str(&read(settings)).unwrap();
    assert!(claude_value["env"].get("ANTHROPIC_AUTH_TOKEN").is_none());
    assert!(claude_value["env"].get("ANTHROPIC_BASE_URL").is_none());
    let backups = api.ok("list_backups", json!({}));
    assert!(!backups.as_array().unwrap().is_empty());
    for backup in backups.as_array().unwrap() {
        assert!(Path::new(backup["backupPath"].as_str().unwrap()).starts_with(root));
    }
}

pub(super) fn restore_absence(sandbox: &Sandbox, providers: &Providers) {
    let home = &sandbox.home;
    let codex = &sandbox.codex;
    let claude = &sandbox.claude;
    let config = &sandbox.config;
    let auth = &sandbox.auth;
    let settings = &sandbox.settings;
    let secret = sandbox.secret.as_str();
    let api = &sandbox.api;
    let codex_a = &providers.codex_a;
    let claude_a = &providers.claude_a;
    // Account setup is separate; projection and undo own configuration only.
    fs::remove_file(auth).unwrap();
    api.rejected(
        "preview_switch",
        json!({"profileId": codex_a["id"]}),
        "codex-official-login-required",
    );
    assert!(!auth.exists());
    write(auth, &sandbox.initial_auth);
    for path in [config, settings] {
        fs::remove_file(path).unwrap();
    }
    api.switch(codex_a, secret);
    api.switch(claude_a, secret);
    assert!(config.exists() && auth.exists() && settings.exists());
    for client in ["codex", "claude"] {
        api.ok(
            "undo_last_switch",
            json!({"target":client,"confirmWrite":true}),
        );
    }
    assert!(!config.exists() && !settings.exists());
    assert_eq!(read(auth), sandbox.initial_auth);
    if *codex != home.join(".codex") {
        assert_eq!(read(&home.join(".codex/config.toml")), "default sentinel");
    }
    if *claude != home.join(".claude") {
        assert_eq!(
            read(&home.join(".claude/settings.json")),
            "default sentinel"
        );
    }
}
