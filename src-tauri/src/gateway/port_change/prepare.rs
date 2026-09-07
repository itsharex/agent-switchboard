//! Read-only port-change preparation and stale-preview snapshots.

use super::*;
use tiny_http::Server;

pub(crate) fn prepare(
    controller: &GatewayController,
    local: &LocalState,
    preparations: &PortChangePreparations,
    new_port: u16,
) -> Result<GatewayPortChangePlan, String> {
    validate_custom_port(new_port)?;
    if !controller.state_available() {
        return Err("本机协议网关状态不可用，请先恢复状态文件后再修改端口".to_string());
    }
    let from_port = controller.configured_port();
    if new_port == from_port {
        return Err("新端口与当前监听端口相同".to_string());
    }
    if controller.blocked_recovery().is_some() || journal_path(local).exists() {
        return Err("存在未完成的端口修改恢复，请先处理后再发起新的修改".to_string());
    }
    let server = Server::http(("127.0.0.1", new_port))
        .map_err(|error| bind_failure_report(new_port, error).message)?;
    let observed = observe_clients(controller, local, from_port, new_port)?;
    let clients = observed
        .iter()
        .filter_map(|entry| entry.client.clone())
        .collect();
    let snapshots = observed.into_iter().map(|entry| entry.snapshot).collect();
    let plan = GatewayPortChangePlan {
        preparation_id: String::new(),
        from_port,
        to_port: new_port,
        clients,
    };
    preparations.issue(PreparedPortChange {
        created_at: Instant::now(),
        plan,
        listener: BoundListener::from_server(server),
        snapshots,
    })
}

pub(super) fn observe_clients(
    controller: &GatewayController,
    local: &LocalState,
    from_port: u16,
    to_port: u16,
) -> Result<Vec<ObservedClient>, String> {
    [AppKind::Codex, AppKind::Claude]
        .into_iter()
        .map(|app| observe_client(controller, local, app, from_port, to_port))
        .collect()
}

fn observe_client(
    controller: &GatewayController,
    local: &LocalState,
    app: AppKind,
    from_port: u16,
    to_port: u16,
) -> Result<ObservedClient, String> {
    let target = local
        .target(app)
        .map_err(|error| format!("无法解析客户端配置路径：{error}"))?;
    let (configuration, config) = read_optional_text(&target, "客户端配置")?;
    if let Some(text) = &configuration {
        adapter::validate_syntax(app, text)
            .map_err(|_| "客户端配置格式无效，无法安全修改端口；请先修复该配置".to_string())?;
    }
    let (auth_text, codex_auth) = if app == AppKind::Codex {
        let auth_path = LocalState::codex_auth_path()
            .map_err(|error| format!("无法解析 Codex 登录缓存路径：{error}"))?;
        let (text, snapshot) = read_optional_text(&auth_path, "Codex 登录缓存")?;
        (text, Some(snapshot))
    } else {
        (None, None)
    };
    let candidates = route_candidates(controller, local, app)?;
    let mut matching = Vec::new();
    if let Some(text) = configuration.as_deref() {
        for candidate in candidates {
            if controller.route_matches_config(&candidate.route, text, auth_text.as_deref())? {
                matching.push(candidate);
            }
        }
    }
    let (route, client) = match matching.as_slice() {
        [candidate] => {
            let current_base_url = endpoint_for(app, from_port);
            let new_base_url = endpoint_for(app, to_port);
            let client = GatewayPortChangeClient {
                app,
                profile_id: candidate.route.profile_id.clone(),
                profile_name: candidate.profile_name.clone(),
                current_base_url,
                new_base_url,
            };
            (
                Some(OwnedRouteSnapshot {
                    profile_id: candidate.route.profile_id.clone(),
                    fingerprint: candidate.route.fingerprint.clone(),
                }),
                Some(client),
            )
        }
        [] => {
            if configuration.as_deref().is_some_and(|text| {
                routing::config_points_at_gateway(app, text, auth_text.as_deref())
            }) {
                return Err(
                    "客户端配置指向本机协议网关，但无法确认所属供应商；请先重新应用该供应商，再修改端口"
                        .to_string(),
                );
            }
            (None, None)
        }
        _ => return Err("客户端配置匹配多个本机协议网关供应商，无法安全修改端口".to_string()),
    };
    Ok(ObservedClient {
        snapshot: ClientSnapshot {
            app,
            config,
            codex_auth,
            route,
        },
        configuration,
        client,
    })
}

struct RouteCandidate {
    route: ActiveRoute,
    profile_name: String,
}

fn route_candidates(
    controller: &GatewayController,
    local: &LocalState,
    app: AppKind,
) -> Result<Vec<RouteCandidate>, String> {
    local
        .configuration()
        .list_providers()
        .map_err(|_| "无法读取供应商配置，已取消本次修改".to_string())?
        .into_iter()
        .filter_map(|record| {
            let profile = record.profile;
            (profile.app == app && profile.route_mode == RouteMode::Custom && !is_direct(&profile))
                .then_some(profile)
        })
        .map(|profile| {
            let profile_name = profile.name.clone();
            controller
                .route_for_profile(&profile)
                .map(|route| RouteCandidate {
                    route,
                    profile_name,
                })
        })
        .collect()
}

fn read_optional_text(
    path: &std::path::Path,
    label: &str,
) -> Result<(Option<String>, FileSnapshot), String> {
    match fs::read_to_string(path) {
        Ok(text) => Ok((
            Some(text.clone()),
            FileSnapshot {
                existed: true,
                hash: hash_bytes(text.as_bytes()),
            },
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok((
            None,
            FileSnapshot {
                existed: false,
                hash: hash_bytes(b""),
            },
        )),
        Err(_) => Err(format!("无法读取{label}，已取消本次修改")),
    }
}

fn endpoint_for(app: AppKind, port: u16) -> String {
    let base = format!("http://127.0.0.1:{port}");
    match app {
        AppKind::Codex => format!("{base}/v1"),
        AppKind::Claude => base,
    }
}
