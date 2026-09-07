use super::http::{argument, is_health_request, origin_is_allowed, InvokeRequest};
use asb_core::contracts::AppKind;
use tiny_http::Method;

#[test]
fn requests_require_a_command_and_object_arguments() {
    assert!(serde_json::from_str::<InvokeRequest>(r#"{"command":"config_status"}"#).is_ok());
    assert!(serde_json::from_str::<InvokeRequest>(r#"{"command":7}"#).is_err());
    assert!(
        serde_json::from_str::<InvokeRequest>(r#"{"command":"config_status","extra":true}"#)
            .is_err()
    );
}

#[test]
fn argument_errors_are_typed_and_do_not_accept_wrong_shapes() {
    let args = serde_json::json!({ "target": "codex" });
    assert_eq!(
        argument::<AppKind>(&args, "target").unwrap(),
        AppKind::Codex
    );
    assert_eq!(
        argument::<String>(&args, "missing").unwrap_err().code,
        "web-argument-missing"
    );
    assert_eq!(
        argument::<bool>(&args, "target").unwrap_err().code,
        "web-argument-invalid"
    );
}

#[test]
fn health_route_is_get_only() {
    assert!(is_health_request(&Method::Get, "/health"));
    assert!(!is_health_request(&Method::Post, "/health"));
    assert!(!is_health_request(&Method::Get, "/invoke"));
}

#[test]
fn development_origin_requires_an_exact_match() {
    let configured = "http://127.0.0.1:1420";
    assert!(origin_is_allowed(configured, configured));
    assert!(!origin_is_allowed("http://127.0.0.1:1421", configured));
    assert!(!origin_is_allowed("http://127.0.0.2:1420", configured));
}
