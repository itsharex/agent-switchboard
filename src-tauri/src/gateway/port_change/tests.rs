//! Focused regression coverage for the endpoint-only port transaction lives
//! here so its recovery and listener invariants stay next to the journal.

use super::*;
use asb_core::adapter;
use asb_core::contracts::{
    ConfigWriteRecord, ExplicitMaxOutputTokens, ProviderDraft, RouteMode, SwitchPlan,
    UpstreamProtocol, WriteOperation,
};
use asb_core::ownership::default_client_settings;
use std::fs;
use tiny_http::Server;
use uuid::Uuid;

fn claude_draft(model: &str) -> ProviderDraft {
    ProviderDraft {
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Claude),
        app: AppKind::Claude,
        route_mode: RouteMode::Custom,
        name: "测试中转".to_string(),
        base_url: Some("http://127.0.0.1:18080".to_string()),
        api_key: "test-upstream-key".to_string(),
        upstream_protocol: Some(UpstreamProtocol::ChatCompletions),
        responses_options: None,
        max_output_tokens: ExplicitMaxOutputTokens::none(),
        model: Some(model.to_string()),
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

fn codex_draft(model: &str) -> ProviderDraft {
    ProviderDraft {
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Codex),
        app: AppKind::Codex,
        route_mode: RouteMode::Custom,
        name: "测试中转".to_string(),
        base_url: Some("http://127.0.0.1:18080".to_string()),
        api_key: "test-upstream-key".to_string(),
        upstream_protocol: Some(UpstreamProtocol::ChatCompletions),
        responses_options: None,
        max_output_tokens: ExplicitMaxOutputTokens::none(),
        model: Some(model.to_string()),
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

fn activate_claude(controller: &GatewayController, local: &LocalState, model: &str) -> String {
    let record = local
        .configuration()
        .create_provider(claude_draft(model))
        .unwrap();
    let plan = SwitchPlan::direct(
        record.profile.clone(),
        default_client_settings(AppKind::Claude),
    );
    let projection = controller.project(&plan).unwrap();
    let target = local.target(AppKind::Claude).unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(&target, adapter::render("{}", &projection.plan).unwrap()).unwrap();
    controller.commit(&projection, || Ok(())).unwrap();
    projection.plan.client_api_key().to_string()
}

fn activate_codex(controller: &GatewayController, local: &LocalState, model: &str) {
    let record = local
        .configuration()
        .create_provider(codex_draft(model))
        .unwrap();
    let plan = SwitchPlan::direct(
        record.profile.clone(),
        default_client_settings(AppKind::Codex),
    );
    let projection = controller.project(&plan).unwrap();
    let target = local.target(AppKind::Codex).unwrap();
    let auth_target = LocalState::codex_auth_path().unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::create_dir_all(auth_target.parent().unwrap()).unwrap();
    fs::write(&target, adapter::render("", &projection.plan).unwrap()).unwrap();
    fs::write(
        &auth_target,
        r#"{"auth_mode":"chatgpt","tokens":{"access_token":"sandbox-access","refresh_token":"sandbox-refresh","id_token":"sandbox-id"}}"#,
    )
    .unwrap();
    controller.commit(&projection, || Ok(())).unwrap();
}

fn prepared_plan(
    controller: &GatewayController,
    local: &LocalState,
    preparations: &PortChangePreparations,
) -> GatewayPortChangePlan {
    for _ in 0..20 {
        let probe = Server::http(("127.0.0.1", 0)).unwrap();
        let port = probe.server_addr().to_ip().unwrap().port();
        drop(probe);
        match prepare(controller, local, preparations, port) {
            Ok(plan) => return plan,
            Err(error) if error.contains("端口已被占用") => continue,
            Err(error) => panic!("prepare failed: {error}"),
        }
    }
    panic!("could not reserve a test listener port")
}

#[test]
fn commit_stops_the_old_listener_socket() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let controller = GatewayController::start(&local);
    let old_port = controller.configured_port();
    activate_claude(&controller, &local, "model-a");
    let preparations = PortChangePreparations::default();
    let plan = prepared_plan(&controller, &local, &preparations);

    commit(&controller, &local, &preparations, &plan.preparation_id).unwrap();

    let released = (0..50).any(|_| match Server::http(("127.0.0.1", old_port)) {
        Ok(server) => {
            drop(server);
            true
        }
        Err(_) => {
            std::thread::sleep(std::time::Duration::from_millis(20));
            false
        }
    });
    assert!(released, "the replaced listener must release its old port");
    controller.shutdown();
}

#[test]
fn activation_after_preview_invalidates_the_port_change() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let controller = GatewayController::start(&local);
    let preparations = PortChangePreparations::default();
    let plan = prepared_plan(&controller, &local, &preparations);

    activate_claude(&controller, &local, "model-a");

    let error = commit(&controller, &local, &preparations, &plan.preparation_id)
        .expect_err("a client activated after preview must invalidate it");
    assert!(error.contains("预览后变化"));
    let target = local.target(AppKind::Claude).unwrap();
    assert!(fs::read_to_string(target)
        .unwrap()
        .contains(&format!("127.0.0.1:{}", plan.from_port)));
    controller.shutdown();
}

#[test]
fn endpoint_change_preserves_the_user_selected_model() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let controller = GatewayController::start(&local);
    activate_claude(&controller, &local, "model-a");
    let target = local.target(AppKind::Claude).unwrap();
    let user_model = "user-selected-model";
    let current = fs::read_to_string(&target)
        .unwrap()
        .replace("model-a", user_model);
    fs::write(&target, current).unwrap();
    let preparations = PortChangePreparations::default();
    let plan = prepared_plan(&controller, &local, &preparations);

    commit(&controller, &local, &preparations, &plan.preparation_id).unwrap();

    let changed = fs::read_to_string(target).unwrap();
    assert!(changed.contains(user_model));
    assert!(changed.contains(&format!("127.0.0.1:{}", plan.to_port)));
    controller.shutdown();
}

#[test]
fn codex_endpoint_change_keeps_its_auth_snapshot_untouched() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let controller = GatewayController::start(&local);
    activate_codex(&controller, &local, "model-a");
    let target = local.target(AppKind::Codex).unwrap();
    let edited = fs::read_to_string(&target)
        .unwrap()
        .replace("model-a", "user-selected-model");
    fs::write(&target, edited).unwrap();
    let auth_target = LocalState::codex_auth_path().unwrap();
    let auth_before = fs::read_to_string(&auth_target).unwrap();
    let preparations = PortChangePreparations::default();
    let plan = prepared_plan(&controller, &local, &preparations);

    commit(&controller, &local, &preparations, &plan.preparation_id).unwrap();

    let changed = fs::read_to_string(target).unwrap();
    assert!(changed.contains("user-selected-model"));
    assert!(changed.contains(&format!("127.0.0.1:{}/codex/", plan.to_port)));
    assert_eq!(fs::read_to_string(auth_target).unwrap(), auth_before);
    controller.shutdown();
}

#[test]
fn missing_gateway_state_with_a_dependent_client_requires_repair() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let target = local.target(AppKind::Claude).unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    fs::write(
        target,
        r#"{"env":{"ANTHROPIC_BASE_URL":"http://127.0.0.1:53176","ANTHROPIC_AUTH_TOKEN":"asb_local_deadbeef"}}"#,
    )
    .unwrap();

    let controller = GatewayController::start(&local);
    assert_eq!(
        controller.observe(&local).status,
        GatewayStatusKind::NeedsRepair
    );
    controller.shutdown();
}

#[test]
fn prepared_recovery_restores_config_hash_and_removes_its_history_fact() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let target = local.target(AppKind::Claude).unwrap();
    fs::create_dir_all(target.parent().unwrap()).unwrap();
    let before = r#"{"model":"user-model","env":{"ANTHROPIC_BASE_URL":"http://127.0.0.1:41000","ANTHROPIC_AUTH_TOKEN":"asb_local_before"}}"#;
    let after = r#"{"model":"user-model","env":{"ANTHROPIC_BASE_URL":"http://127.0.0.1:41001","ANTHROPIC_AUTH_TOKEN":"asb_local_before"}}"#;
    fs::write(&target, after).unwrap();
    let mut state = fresh_state();
    state.port = 41001;
    write_state(&local.gateway_state_path(), &state).unwrap();
    let id = Uuid::new_v4().to_string();
    let directory = backup_dir(&local, &id).unwrap();
    fs::create_dir_all(&directory).unwrap();
    fs::write(directory.join(backup_file_for(AppKind::Claude)), before).unwrap();
    let record = ConfigWriteRecord {
        app: AppKind::Claude,
        profile_id: Some("p1".to_string()),
        profile_name: Some("测试中转".to_string()),
        content_hash: hash_bytes(after.as_bytes()),
        backup_id: "port-change-test".to_string(),
        at: "2026-09-07T00:00:00Z".to_string(),
        operation: WriteOperation::GatewayPortChange,
    };
    local
        .configuration()
        .record_config_write(record.clone())
        .unwrap();
    let journal = PortChangeJournal {
        version: JOURNAL_VERSION,
        id,
        from_port: 41000,
        to_port: 41001,
        stage: JournalStage::Prepared,
        clients: vec![JournalClient {
            app: AppKind::Claude,
            before_hash: hash_bytes(before.as_bytes()),
            after_hash: hash_bytes(after.as_bytes()),
            backup_file: backup_file_for(AppKind::Claude).to_string(),
            history: Some(record),
        }],
    };
    write_journal(&local, &journal).unwrap();

    assert!(
        recover_pending(&local, &local.gateway_state_path(), &mut state)
            .unwrap()
            .is_none()
    );
    assert_eq!(fs::read_to_string(target).unwrap(), before);
    assert_eq!(state.port, 41000);
    assert_eq!(
        local
            .configuration()
            .latest_config_write(AppKind::Claude)
            .unwrap(),
        None
    );
    assert!(
        asb_switch::list_backups(&asb_switch::io::FsIo, &local.backup_dir())
            .into_iter()
            .any(|record| record.reason == "gateway-port-rollback")
    );
    assert!(!journal_path(&local).exists());
}

#[test]
fn cancelling_a_preview_releases_its_reserved_socket() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let controller = GatewayController::start(&local);
    let preparations = PortChangePreparations::default();
    let plan = prepared_plan(&controller, &local, &preparations);

    assert!(preparations.cancel(&plan.preparation_id).unwrap());
    let released = (0..50).any(|_| match Server::http(("127.0.0.1", plan.to_port)) {
        Ok(server) => {
            drop(server);
            true
        }
        Err(_) => {
            std::thread::sleep(std::time::Duration::from_millis(20));
            false
        }
    });
    assert!(released, "cancelling must release the prepared port");
    controller.shutdown();
}

#[test]
fn discarding_a_precommit_recovery_aligns_the_saved_port_with_the_live_listener() {
    let _client_paths = crate::test_client_paths::redirect_client_paths();
    let directory = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(directory.path().join("state"));
    let controller = GatewayController::start(&local);
    let from_port = controller.configured_port();
    let preparations = PortChangePreparations::default();
    let plan = prepared_plan(&controller, &local, &preparations);
    preparations.cancel(&plan.preparation_id).unwrap();
    let journal = PortChangeJournal {
        version: JOURNAL_VERSION,
        id: Uuid::new_v4().to_string(),
        from_port,
        to_port: plan.to_port,
        stage: JournalStage::Prepared,
        clients: vec![],
    };
    write_journal(&local, &journal).unwrap();
    controller.persist_port(plan.to_port).unwrap();
    controller.block_port_change(blocked(&journal, "需要保留当前客户端配置"));

    assert_eq!(
        controller.observe(&local).status,
        GatewayStatusKind::RecoveryBlocked
    );

    discard_blocked(&controller, &local).unwrap();

    assert_eq!(controller.configured_port(), from_port);
    assert_eq!(
        controller.listening().map(|listener| listener.port()),
        Some(from_port)
    );
    assert!(!journal_path(&local).exists());
    controller.shutdown();
}

#[test]
fn journal_validation_rejects_ambiguous_or_incomplete_commits() {
    let client = JournalClient {
        app: AppKind::Claude,
        before_hash: "a".repeat(64),
        after_hash: "b".repeat(64),
        backup_file: backup_file_for(AppKind::Claude).to_string(),
        history: None,
    };
    let mut journal = PortChangeJournal {
        version: JOURNAL_VERSION,
        id: Uuid::new_v4().to_string(),
        from_port: 41000,
        to_port: 41001,
        stage: JournalStage::Prepared,
        clients: vec![client.clone(), client.clone()],
    };
    assert!(validate_journal(&journal).is_err());

    journal.clients.truncate(1);
    journal.stage = JournalStage::Committed;
    assert!(validate_journal(&journal).is_err());

    journal.clients[0].history = Some(ConfigWriteRecord {
        app: AppKind::Claude,
        profile_id: Some("p1".to_string()),
        profile_name: Some("测试中转".to_string()),
        content_hash: journal.clients[0].after_hash.clone(),
        backup_id: "backup-1".to_string(),
        at: "2026-09-07T00:00:00Z".to_string(),
        operation: WriteOperation::GatewayPortChange,
    });
    assert!(validate_journal(&journal).is_ok());
}
