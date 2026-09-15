use super::*;
use crate::gateway::codex::policy::{self, CodexGatewayPolicy};
use asb_core::contracts::CodexManagedAuth;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use serde_json::json;

fn jwt(value: serde_json::Value) -> String {
    format!("h.{}.s", URL_SAFE_NO_PAD.encode(value.to_string()))
}

/// A full official-takeover fixture: redirected client paths, a seeded
/// managed account imported from a native `auth.json`, an official Codex
/// plan bound to it, and a running gateway whose policy enables takeover.
struct Fixture {
    _paths: crate::test_client_paths::ClientPathGuard,
    _directory: tempfile::TempDir,
    state: LocalState,
    gateway: GatewayController,
    managed: CodexManagedAuth,
    access_token: String,
    #[allow(dead_code)]
    id_token: String,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.gateway.shutdown();
    }
}
impl Fixture {
    fn new(takeover: bool) -> Self {
        let paths = crate::test_client_paths::redirect_client_paths();
        let directory = tempfile::tempdir().unwrap();
        let auth_path = LocalState::codex_auth_path().unwrap();
        let id_token = jwt(json!({
            "sub": "one",
            "email": "one@example.test",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "work",
                "chatgpt_plan_type": "team"
            }
        }));
        let access_token = jwt(json!({
            "exp": 4102444800_i64,
            "https://api.openai.com/auth": {"chatgpt_account_id": "work"}
        }));
        let native = json!({
            "auth_mode": "chatgpt",
            "OPENAI_API_KEY": null,
            "tokens": {
                "id_token": id_token,
                "access_token": access_token,
                "refresh_token": "refresh-secret-one"
            },
            "last_refresh": chrono::Utc::now().to_rfc3339()
        });
        std::fs::write(&auth_path, native.to_string()).unwrap();
        let state = LocalState::from_root(directory.path().join("state"));
        let view = crate::codex_auth::list_accounts(state.root()).unwrap();
        crate::codex_auth::import_native(state.root(), &auth_path, &view.revision).unwrap();
        let view = crate::codex_auth::list_accounts(state.root()).unwrap();
        assert_eq!(view.accounts.len(), 1);
        let managed = CodexManagedAuth {
            managed_id: view.accounts[0].id.clone(),
            account_id: "work".into(),
            subject: "one".into(),
            id_token: id_token.clone(),
            access_token: access_token.clone(),
            refresh_token: "refresh-secret-one".into(),
            generation: view.accounts[0].generation,
            last_refresh: chrono::Utc::now().to_rfc3339(),
        };
        let policy = CodexGatewayPolicy {
            takeover,
            ..Default::default()
        };
        policy::save(state.root(), &policy).unwrap();
        let gateway = GatewayController::start(&state);
        Self {
            _paths: paths,
            _directory: directory,
            state,
            gateway,
            managed,
            access_token,
            id_token,
        }
    }

    fn official_plan(&self) -> SwitchPlan {
        SwitchPlan::direct(official_profile(), default_client_settings(AppKind::Codex))
            .with_codex_managed_auth(self.managed.clone())
    }

    fn policy(&self) -> CodexGatewayPolicy {
        policy::load(&self.state.root()).unwrap().0
    }
}

fn official_profile() -> asb_core::contracts::ProviderProfile {
    asb_core::contracts::ProviderProfile {
        authentication: None,
        id: uuid::Uuid::new_v4().to_string(),
        parameters: asb_core::ownership::default_provider_parameters(AppKind::Codex),
        app: AppKind::Codex,
        route_mode: RouteMode::Official,
        name: "Codex 官方登录".to_string(),
        model: None,
        claude_fragment: Default::default(),
        base_url: None,
        connection: Default::default(),
        api_key: String::new(),
        upstream_protocol: None,
        responses_options: None,
        max_output_tokens: None.into(),
        model_options: None,
        notes: None,
        website_url: None,
        display: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

fn routed_route(projection: &crate::gateway::GatewayProjection) -> ActiveRoute {
    match &projection.activation {
        crate::gateway::GatewayActivation::Routed(route) => route.clone(),
        crate::gateway::GatewayActivation::Direct { .. } => {
            panic!("takeover must produce a routed activation")
        }
    }
}

#[test]
fn official_takeover_projects_a_routed_managed_route_without_failover_candidates() {
    let fixture = Fixture::new(true);
    let projection = fixture
        .gateway
        .project_codex_official_takeover(fixture.official_plan(), &fixture.policy())
        .unwrap();
    assert!(projection.plan.is_gateway());
    assert!(projection.plan.codex_managed_auth().is_some());
    assert!(projection.candidate_routes.is_empty(), "官方账号绝不入队");
    let warning = projection.warning().expect("takeover warning");
    assert!(warning.contains("官方"));
    assert!(warning.contains("不参与自动故障转移"));
    let route = routed_route(&projection);
    assert_eq!(
        route.upstream_base_url,
        crate::codex_auth::OFFICIAL_CODEX_BASE
    );
    assert_eq!(route.api_key, "", "凭据按次解析，绝不驻留路由");
    let bound = route.codex_account.as_ref().expect("bound account");
    assert_eq!(
        bound.managed_id,
        projection
            .plan
            .codex_managed_auth()
            .expect("projected plan keeps the binding")
            .managed_id
    );
    assert_eq!(bound.account_id, "work");

    // Committed official route: the built-in openai provider keeps the
    // gateway entry in `openai_base_url` and that endpoint is the revision.
    fixture.gateway.commit(&projection, || Ok(())).unwrap();
    let base = fixture.gateway.configured_base_url();
    let endpoint = route.client_endpoint(&base);
    let live = fixture
        .gateway
        .inner
        .routes
        .read()
        .expect("route lock")
        .get(&AppKind::Codex)
        .cloned()
        .expect("committed official route");
    // The plan the executor would render must install exactly that entry.
    let rendered =
        asb_core::adapter::render("", &projection.plan).expect("official takeover client render");
    let document = rendered
        .parse::<toml_edit::DocumentMut>()
        .expect("rendered client document");
    assert_eq!(
        document
            .get(asb_core::ownership::CODEX_PROVIDER_BASE_URL_KEY)
            .and_then(|value| value.as_str()),
        Some(endpoint.as_str())
    );
    assert!(fixture
        .gateway
        .route_matches_config(&live, &rendered)
        .unwrap());
    let tampered = "model = 'gpt-5'\nopenai_base_url = 'https://other.example/v1'";
    assert!(!fixture
        .gateway
        .route_matches_config(&live, tampered)
        .unwrap());
}

#[test]
fn official_takeover_requires_takeover_policy_and_a_bound_account() {
    // The client-path guard is a process-global lock; the first fixture must
    // be dropped before the second one can take it.
    let error = {
        let direct = Fixture::new(false);
        direct
            .gateway
            .project_codex_official_takeover(direct.official_plan(), &direct.policy())
            .err()
            .expect("takeover off must refuse the official projection")
    };
    assert!(error.contains("接管"), "{error}");

    let takeover = Fixture::new(true);
    let unbound = SwitchPlan::direct(official_profile(), default_client_settings(AppKind::Codex));
    let error = takeover
        .gateway
        .project_codex_official_takeover(unbound, &takeover.policy())
        .err()
        .expect("unbound official plan must refuse takeover");
    assert!(error.contains("托管账号"), "{error}");
}

#[test]
fn official_route_resolution_refreshes_credentials_and_verifies_native_identity() {
    let fixture = Fixture::new(true);
    let projection = fixture
        .gateway
        .project_codex_official_takeover(fixture.official_plan(), &fixture.policy())
        .unwrap();
    let saved = routed_route(&projection);
    let header =
        |value: &str| tiny_http::Header::from_bytes("Authorization", value).expect("header");

    // The route's own capability token is always accepted.
    let capability = format!("Bearer {}", saved.client_token);
    let resolved = super::super::codex_account::resolve(
        &saved,
        &fixture.gateway.inner,
        Some(&[header(&capability)][..]),
    )
    .unwrap();
    assert_eq!(
        resolved.api_key, fixture.access_token,
        "fresh managed token"
    );
    assert_eq!(
        resolved
            .codex_account
            .as_ref()
            .map(|auth| auth.account_id.as_str()),
        Some("work")
    );
    assert_eq!(
        saved.api_key, "",
        "resolution never mutates the saved route"
    );

    // A native bearer of the bound account is accepted.
    let native = format!("Bearer {}", fixture.access_token);
    super::super::codex_account::resolve(
        &saved,
        &fixture.gateway.inner,
        Some(&[header(&native)][..]),
    )
    .unwrap();

    // No authorization at all is admitted; the gateway injects credentials.
    super::super::codex_account::resolve(&saved, &fixture.gateway.inner, None).unwrap();

    // A CLI logged into a different account is rejected, never silently
    // allowed to spend the bound account's quota.
    let foreign = format!(
        "Bearer {}",
        jwt(json!({
            "exp": 4102444800_i64,
            "https://api.openai.com/auth": {"chatgpt_account_id": "other-work"}
        }))
    );
    let (status, message) = super::super::codex_account::resolve(
        &saved,
        &fixture.gateway.inner,
        Some(&[header(&foreign)][..]),
    )
    .err()
    .expect("a foreign CLI login must be rejected");
    assert_eq!(status, 403);
    assert!(message.contains("身份不一致"), "{message}");

    // Unparseable non-capability bearers fail closed.
    let (status, _) = super::super::codex_account::resolve(
        &saved,
        &fixture.gateway.inner,
        Some(&[header("Bearer not-a-jwt")][..]),
    )
    .err()
    .expect("unparseable bearers must be rejected");
    assert_eq!(status, 403);
}

#[test]
fn official_upstream_exit_sends_bearer_and_bound_account_headers() {
    let fixture = Fixture::new(true);
    let projection = fixture
        .gateway
        .project_codex_official_takeover(fixture.official_plan(), &fixture.policy())
        .unwrap();
    let saved = routed_route(&projection);
    let resolved =
        super::super::codex_account::resolve(&saved, &fixture.gateway.inner, None).unwrap();
    let mut headers = reqwest::header::HeaderMap::new();
    crate::gateway::codex::request::headers(&resolved, &mut headers);
    assert_eq!(
        headers
            .get("chatgpt-account-id")
            .and_then(|value| value.to_str().ok()),
        Some("work")
    );

    // The shared upstream exit composes base headers with the Codex request
    // headers (transport.rs): the resolved token becomes the official bearer
    // credential, and native passthrough never forwards the client's own.
    let incoming =
        [tiny_http::Header::from_bytes("Authorization", "Bearer client-own-token").unwrap()];
    let mut upstream =
        super::super::route::upstream_headers(&resolved, None, None, Some(&incoming));
    crate::gateway::codex::request::headers(&resolved, &mut upstream);
    let expected = format!("Bearer {}", fixture.access_token);
    assert_eq!(
        upstream
            .get("authorization")
            .and_then(|value| value.to_str().ok()),
        Some(expected.as_str())
    );
    assert_eq!(
        upstream
            .get("chatgpt-account-id")
            .and_then(|v| v.to_str().ok()),
        Some("work")
    );
}

#[test]
fn official_models_are_served_from_the_backend_document_not_a_catalog() {
    let fixture = Fixture::new(true);
    let projection = fixture
        .gateway
        .project_codex_official_takeover(fixture.official_plan(), &fixture.policy())
        .unwrap();
    let route = routed_route(&projection);
    assert!(
        route.codex.as_ref().unwrap().catalog.is_empty(),
        "官方模型不属于任何本地目录"
    );
    // Admission keeps the client's model identity verbatim: an official
    // model name passes where a catalog route would reject it.
    let body = json!({"model": "gpt-5.3-codex", "input": "hi"})
        .to_string()
        .into_bytes();
    let validated = super::super::codex::resolve_model_and_validate(
        &route,
        super::super::CodexOperation::Responses,
        body,
    )
    .expect("official models bypass the catalog");
    let value: serde_json::Value = serde_json::from_slice(&validated).unwrap();
    assert_eq!(value["model"], "gpt-5.3-codex");
    let missing = json!({"input": "hi"}).to_string().into_bytes();
    assert!(super::super::codex::resolve_model_and_validate(
        &route,
        super::super::CodexOperation::Responses,
        missing
    )
    .unwrap_err()
    .contains("model"));
}

#[test]
fn takeover_fingerprint_follows_the_bound_account_and_keeps_the_capability_stable() {
    let fixture = Fixture::new(true);
    // One stored official profile (stable id, as in the store) projected
    // twice must yield the same route identity.
    let plan = fixture.official_plan();
    let first = routed_route(
        &fixture
            .gateway
            .project_codex_official_takeover(plan.clone(), &fixture.policy())
            .unwrap(),
    );
    let second = routed_route(
        &fixture
            .gateway
            .project_codex_official_takeover(plan, &fixture.policy())
            .unwrap(),
    );
    assert_eq!(first.fingerprint, second.fingerprint);
    assert_eq!(first.client_token, second.client_token);
    assert_eq!(first.continuation_key, second.continuation_key);
    assert_eq!(first.upstream_base_url, second.upstream_base_url);
}
