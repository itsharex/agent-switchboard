use super::*;
use asb_core::contracts::{ClaudeModelSettings, ModelOptions, ProviderDraft, ProviderProfile};

fn provider(local: &LocalState, name: &str, url: String, haiku: &str) -> ProviderProfile {
    let mut profile = sandbox_profile(
        local,
        AppKind::Claude,
        name,
        url,
        format!("key-{name}"),
        UpstreamProtocol::ChatCompletions,
    );
    profile.model_options = Some(ModelOptions::Claude(ClaudeModelSettings {
        haiku_model: Some(haiku.into()),
        ..Default::default()
    }));
    let mut value = serde_json::to_value(&profile).unwrap();
    value.as_object_mut().unwrap().remove("id");
    let draft: ProviderDraft = serde_json::from_value(value).unwrap();
    let record = local
        .configuration()
        .find_provider_record(&profile.id)
        .unwrap();
    local
        .configuration()
        .update_provider(&profile.id, draft, &record.file_hash)
        .unwrap()
        .profile
}

fn upstream() -> (String, thread::JoinHandle<(String, Value)>) {
    let server = Server::http(("127.0.0.1", 0)).unwrap();
    let base = endpoint(&server);
    let task = thread::spawn(move || {
        let mut request = server
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .expect("request");
        let key = request
            .headers()
            .iter()
            .find(|h| h.field.equiv("authorization"))
            .unwrap()
            .value
            .to_string();
        let mut body = String::new();
        request.as_reader().read_to_string(&mut body).unwrap();
        request.respond(Response::from_string(json!({"id":"reply", "object":"chat.completion", "model":"served", "choices":[{"index":0,"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}).to_string()).with_header(content_type("application/json"))).unwrap();
        (key, serde_json::from_str(&body).unwrap())
    });
    (base, task)
}

#[test]
fn claude_hot_switch_uses_the_new_role_map_with_the_same_running_client_capability() {
    let dir = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(dir.path().join("state"));
    let (first_url, first_task) = upstream();
    let (second_url, second_task) = upstream();
    let first = provider(&local, "first", first_url, "haiku-first");
    let second = provider(&local, "second", second_url, "haiku-second");
    let gateway = GatewayController::start(&local);
    let first = gateway
        .project_with_candidates(
            &local,
            &SwitchPlan::direct(first, default_client_settings(AppKind::Claude)),
        )
        .unwrap();
    let second = gateway
        .project_with_candidates(
            &local,
            &SwitchPlan::direct(second, default_client_settings(AppKind::Claude)),
        )
        .unwrap();
    let token = first.plan.client_api_key().to_string();
    assert_eq!(token, second.plan.client_api_key());
    for projection in [&first, &second] {
        gateway.commit(projection, || Ok(())).unwrap();
        let response = Client::builder().no_proxy().timeout(Duration::from_secs(5)).build().unwrap()
            .post(format!("{}/v1/messages", gateway.configured_base_url())).bearer_auth(&token)
            .header(CONTENT_TYPE, "application/json").body(json!({"model":"asb-claude-haiku", "max_tokens":16, "messages":[{"role":"user","content":"hello"}]}).to_string()).send().unwrap();
        assert_eq!(response.status().as_u16(), 200);
        response.bytes().unwrap();
    }
    let (key, body) = first_task.join().unwrap();
    assert_eq!(key, "Bearer key-first");
    assert_eq!(body["model"], "haiku-first");
    let (key, body) = second_task.join().unwrap();
    assert_eq!(key, "Bearer key-second");
    assert_eq!(body["model"], "haiku-second");
    gateway.shutdown();
}

#[test]
fn failed_claude_commit_never_exposes_the_uncommitted_route() {
    let dir = tempfile::tempdir().unwrap();
    let local = LocalState::from_root(dir.path().join("state"));
    let first = provider(&local, "first", "http://127.0.0.1:18080".into(), "first");
    let second = provider(&local, "second", "http://127.0.0.1:18081".into(), "second");
    let first_id = first.id.clone();
    let gateway = GatewayController::start(&local);
    let first = gateway
        .project_with_candidates(
            &local,
            &SwitchPlan::direct(first, default_client_settings(AppKind::Claude)),
        )
        .unwrap();
    let second = gateway
        .project_with_candidates(
            &local,
            &SwitchPlan::direct(second, default_client_settings(AppKind::Claude)),
        )
        .unwrap();
    gateway.commit(&first, || Ok(())).unwrap();
    let (entered, ready) = std::sync::mpsc::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let writer = gateway.clone();
    let commit = thread::spawn(move || {
        writer.commit(&second, || {
            entered.send(()).unwrap();
            wait.recv().unwrap();
            Err("fixture record failure".into())
        })
    });
    ready.recv_timeout(Duration::from_secs(2)).unwrap();
    let (sent, read) = std::sync::mpsc::channel();
    let observer = gateway.clone();
    let reader = thread::spawn(move || {
        sent.send(observer.active_profile_id_for(AppKind::Claude))
            .unwrap()
    });
    assert!(read.recv_timeout(Duration::from_millis(100)).is_err());
    release.send(()).unwrap();
    assert!(commit.join().unwrap().is_err());
    assert_eq!(
        read.recv_timeout(Duration::from_secs(2)).unwrap(),
        Some(first_id)
    );
    reader.join().unwrap();
    gateway.shutdown();
}
