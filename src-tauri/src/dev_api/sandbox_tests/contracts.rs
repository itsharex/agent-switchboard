use super::*;

fn prepare_gateway_port_change(api: &Api, excluded_port: u16) -> Value {
    for _ in 0..20 {
        let probe = Server::http(("127.0.0.1", 0)).unwrap();
        let port = probe.server_addr().to_ip().unwrap().port();
        drop(probe);
        if port == excluded_port {
            continue;
        }
        let response = api.request("gateway_prepare_port_change", json!({ "newPort": port }));
        if response["kind"].as_str() == Some("success") {
            return response["result"].clone();
        }
        assert_eq!(response["error"]["code"], "gateway-port-change-invalid");
    }
    panic!("could not reserve a gateway test port")
}

pub(super) fn gateway_commands(sandbox: &Sandbox) {
    let api = &sandbox.api;
    // The browser-development dispatcher exposes the same one-shot
    // prepare/confirm/cancel contract as the desktop invoke boundary.
    let initial_gateway = api.ok("gateway_status", json!({}));
    let prepared_port = prepare_gateway_port_change(
        api,
        initial_gateway["configuredPort"].as_u64().unwrap() as u16,
    );
    api.rejected(
        "gateway_commit_port_change",
        json!({ "preparationId": prepared_port["preparationId"], "confirmWrite": false }),
        "write-not-confirmed",
    );
    let committed_port = api.ok(
        "gateway_commit_port_change",
        json!({ "preparationId": prepared_port["preparationId"], "confirmWrite": true }),
    );
    assert_eq!(committed_port["toPort"], prepared_port["toPort"]);
    assert_eq!(committed_port["warnings"], json!([]));

    let cancelled_port =
        prepare_gateway_port_change(api, committed_port["toPort"].as_u64().unwrap() as u16);
    api.ok(
        "gateway_cancel_port_change",
        json!({ "preparationId": cancelled_port["preparationId"] }),
    );
    let released = Server::http((
        "127.0.0.1",
        cancelled_port["toPort"].as_u64().unwrap() as u16,
    ));
    assert!(
        released.is_ok(),
        "cancelled preview must release its socket"
    );
    drop(released);
}

pub(super) fn reject_obsolete_credentials(sandbox: &Sandbox) {
    let secret = sandbox.secret.as_str();
    let api = &sandbox.api;
    // Authentication is derived by `upstreamProtocol`; obsolete manual
    // authentication input must fail at the public command boundary before
    // either command can contact a provider.
    api.rejected(
        "fetch_provider_models",
        json!({
            "request": {
                "url": "https://provider.example/v1",
                "apiKey": secret,
                "upstreamProtocol": "responses",
                "authScheme": "bearer"
            }
        }),
        "web-argument-invalid",
    );
    api.rejected(
        "test_usage_query",
        json!({
            "request": {
                "query": {
                    "kind": "declarative",
                    "url": "https://provider.example/usage",
                    "remainingPath": "balance",
                    "refreshIntervalMinutes": 0
                },
                "apiKey": secret,
                "baseUrl": "https://provider.example/v1",
                "upstreamProtocol": "responses",
                "authScheme": "bearer"
            }
        }),
        "web-argument-invalid",
    );
}
