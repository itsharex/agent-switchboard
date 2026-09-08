use super::*;

pub(super) fn unwritten_save(sandbox: &Sandbox, providers: &Providers) {
    let config = &sandbox.config;
    let api = &sandbox.api;
    let state = &sandbox.state;
    let codex_a = &providers.codex_a;
    let before_metadata_config = read(&sandbox.config);
    // The durable marker is discarded when a process stopped before the
    // provider file changed, and otherwise deterministically finishes the
    // already confirmed provider update on the next startup/write boundary.
    let recovery_record = api.provider_record(codex_a);
    state
        .configuration()
        .begin_profile_save(&crate::config_store::PendingProfileSave {
            profile_id: codex_a["id"].as_str().unwrap().to_string(),
            app: asb_core::contracts::AppKind::Codex,
            previous_file_hash: recovery_record["fileHash"].as_str().unwrap().to_string(),
        })
        .unwrap();
    api.save_client("codex", "approval_policy", json!("on-request"));
    assert!(state
        .configuration()
        .pending_profile_save()
        .unwrap()
        .is_none());
    assert_eq!(read(config), before_metadata_config);
}

pub(super) fn confirmed_save(sandbox: &Sandbox, providers: &Providers) {
    let config = &sandbox.config;
    let auth = &sandbox.auth;
    let settings = &sandbox.settings;
    let secret = sandbox.secret.as_str();
    let oauth = sandbox.oauth.as_str();
    let initial_claude = sandbox.initial_claude;
    let api = &sandbox.api;
    let state = &sandbox.state;
    let codex_a = &providers.codex_a;
    let claude_a = &providers.claude_a;
    let recovery_record = sandbox.api.provider_record(&providers.codex_a);
    let mut interrupted_draft = draft_from_profile(codex_a);
    interrupted_draft["model"] = json!("codex-a-recovered");
    let (profile_app, original_file) = state
        .configuration()
        .load_provider_file(codex_a["id"].as_str().unwrap())
        .unwrap();
    let candidate = asb_core::ProviderProfile::from_draft(
        codex_a["id"].as_str().unwrap().to_string(),
        serde_json::from_value(interrupted_draft.clone()).unwrap(),
    );
    let confirmation = api.prepare_edit(&recovery_record, interrupted_draft.clone());
    crate::commands::switching::save_profile_preimage(
        state,
        profile_app,
        &original_file,
        &candidate,
        confirmation["preview"]["contentHash"].as_str().unwrap(),
        confirmation["preview"]["renderedHash"].as_str().unwrap(),
    )
    .unwrap();

    state
        .configuration()
        .begin_profile_save(&crate::config_store::PendingProfileSave {
            profile_id: codex_a["id"].as_str().unwrap().to_string(),
            app: asb_core::contracts::AppKind::Codex,
            previous_file_hash: recovery_record["fileHash"].as_str().unwrap().to_string(),
        })
        .unwrap();
    state
        .configuration()
        .update_provider(
            codex_a["id"].as_str().unwrap(),
            serde_json::from_value(interrupted_draft).unwrap(),
            recovery_record["fileHash"].as_str().unwrap(),
        )
        .unwrap();
    api.save_client("codex", "approval_policy", json!("on-request"));
    assert!(state
        .configuration()
        .pending_profile_save()
        .unwrap()
        .is_none());
    assert!(read(config).contains("model = \"codex-a-recovered\""));
    let recovered_record = api.provider_record(codex_a);
    let recovered_restore = api.prepare_edit(&recovered_record, draft_from_profile(codex_a));
    assert_eq!(recovered_restore["kind"], "saveAndApply");
    api.commit_profile_save(&recovered_restore, true);
    check_codex(config, auth, "a", secret, oauth, "high");
    assert_eq!(read(settings), initial_claude);
    api.switch(claude_a, secret);
    check_claude(settings, "a", secret);
}
