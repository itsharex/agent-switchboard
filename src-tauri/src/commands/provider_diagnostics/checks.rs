use super::{check, message, ProviderDiagnosticCheck};
use crate::commands::{error::CommandError, ConfigFileStatus};
use crate::gateway::{GatewayObservation, GatewayStatusKind};
use asb_core::contracts::{AppKind, ProviderProfile, RouteMode};

pub(super) fn safe_endpoint(profile: &ProviderProfile) -> Option<String> {
    let value = profile.base_url.as_deref()?;
    if profile.connection.claude_native.is_some() { return None; }
    let parsed = reqwest::Url::parse(value).ok()?;
    (matches!(parsed.scheme(), "http" | "https") && parsed.username().is_empty()
        && parsed.password().is_none() && parsed.query().is_none() && parsed.fragment().is_none())
        .then(|| parsed.to_string())
}

pub(super) fn profile_checks(profile: &ProviderProfile) -> Vec<ProviderDiagnosticCheck> {
    if profile.route_mode == RouteMode::Official {
        return ["address", "protocol", "authentication", "model"].into_iter().map(|category| {
            check(category, "unknown", "official", "由客户端官方登录与服务决定；本地检查不代表登录或模型可用。",
                Some(("testOfficial", "请在客户端确认登录状态并发起一次请求。")))
        }).collect();
    }
    vec![address(profile), protocol(profile), authentication(profile), model(profile)]
}

fn address(profile: &ProviderProfile) -> ProviderDiagnosticCheck {
    if let Some(native) = &profile.connection.claude_native {
        let valid = native.validate(profile.base_url.as_deref()).is_ok();
        return check("address", if valid { "pass" } else { "error" }, "nativeAddress",
            "原生云 SDK 构造请求地址；普通 HTTP 地址测试不适用。",
            Some(("editNative", "检查原生云服务的区域、资源与模型权限。")));
    }
    let valid = profile.base_url.as_deref().zip(profile.upstream_protocol)
        .is_some_and(|(base, protocol)| {
            if protocol == asb_core::UpstreamProtocol::GeminiGenerateContent {
                asb_core::claude_gemini::request_preview(base, profile.connection.is_full_url, None).is_ok()
            } else {
                asb_core::endpoint::upstream_endpoint_with_options(base, protocol, profile.connection.is_full_url).is_ok()
            }
        });
    if valid {
        check("address", "pass", "addressValid", "请求地址通过本地解析；尚未验证网络连通性。", None)
    } else {
        check("address", "error", "addressInvalid", "请求地址与所选协议或完整地址模式不匹配。",
            Some(("editAddress", "编辑已保存供应商，核对 API 根地址、协议和完整地址模式。")))
    }
}

fn protocol(profile: &ProviderProfile) -> ProviderDiagnosticCheck {
    let Some(protocol) = profile.upstream_protocol else {
        return check("protocol", "error", "protocolMissing", "自定义供应商没有上游协议。",
            Some(("editProtocol", "按服务商文档选择请求协议后保存。")));
    };
    let mut item = check("protocol", "pass", "protocolConfigured", "已配置请求协议；协议兼容性需要实际请求验证。",
        Some(("runRequest", "运行联网请求测试，确认认证、模型和响应协议。")));
    item.message = message("protocolValue", serde_json::json!({ "protocol": protocol.label() }),
        "请求协议已配置；协议兼容性需要实际请求验证。");
    item
}

fn authentication(profile: &ProviderProfile) -> ProviderDiagnosticCheck {
    if profile.connection.claude_native.is_some() || profile.connection.auth_binding.is_some() {
        return check("authentication", "unknown", "managedAuth", "认证由原生云 SDK 或绑定账号提供；未向服务端验证。",
            Some(("runRequest", "运行联网请求测试，确认认证、模型和响应协议。")));
    }
    if profile.api_key.trim().is_empty() {
        return check("authentication", "error", "authMissing", "已保存供应商缺少 API 凭据。",
            Some(("editAuth", "在供应商编辑页设置凭据并保存；不要将凭据写入诊断记录。")));
    }
    check("authentication", "unknown", "authPresent", "已保存凭据；尚未确认服务端是否接受。",
        Some(("runRequest", "运行联网请求测试，确认认证、模型和响应协议。")))
}

fn model(profile: &ProviderProfile) -> ProviderDiagnosticCheck {
    if profile.model.as_deref().is_some_and(|model| !model.trim().is_empty()) {
        check("model", "unknown", "modelConfigured", "已配置默认模型；账号权限、上游模型 ID 与可用性需要联网验证。",
            Some(("runRequest", "运行联网请求测试，确认认证、模型和响应协议。")))
    } else {
        check("model", "warning", "modelInherited", "未设置默认模型，将使用客户端选择。",
            Some(("editModel", "确认客户端选择的模型在该供应商可用，或保存明确的默认模型。")))
    }
}

pub(super) fn environment(app: AppKind) -> ProviderDiagnosticCheck {
    let names: &[&str] = match app {
        AppKind::Codex => &["CODEX_HOME", "OPENAI_API_KEY", "OPENAI_BASE_URL", "OPENAI_ORG_ID", "OPENAI_PROJECT_ID", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"],
        AppKind::Claude => &["CLAUDE_CONFIG_DIR", "ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_BASE_URL", "ANTHROPIC_MODEL", "CLAUDE_CODE_USE_BEDROCK", "CLAUDE_CODE_USE_VERTEX", "CLAUDE_CODE_USE_FOUNDRY", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "NO_PROXY"],
    };
    environment_for(names.iter().filter(|name| std::env::var_os(name).is_some()).copied().collect())
}

fn environment_for(names: Vec<&str>) -> ProviderDiagnosticCheck {
    let mut item = check("environment", if names.is_empty() { "unknown" } else { "warning" },
        "environmentNone", "本应用进程未检测到已知覆盖变量；其他终端、项目或启动器的环境不可见。",
        Some(("checkEnvironment", "在启动客户端的终端或启动器中核对变量；修改后重启客户端。此处只展示变量名。")));
    if !names.is_empty() {
        item.message = message("environmentFound", serde_json::json!({ "names": names.join(", ") }),
            "本应用进程检测到可能影响客户端的环境变量。");
    }
    item
}

pub(super) fn configuration(status: &ConfigFileStatus, active: bool, preview: Result<&asb_switch::FilePreview, &CommandError>) -> ProviderDiagnosticCheck {
    if status.read_error.is_some() || !status.syntax_ok || status.recovery_issue.is_some() || status.client_settings_error.is_some() {
        let mut item = check("configuration", "error", "configurationInvalid", "当前客户端公共配置不可读、无效或有待处理的恢复事务。",
            Some(("openConfiguration", "在客户端配置页查看具体错误并处理；诊断不会覆盖损坏配置。")));
        let details = [&status.read_error, &status.recovery_issue, &status.client_settings_error]
            .into_iter().filter_map(|value| value.as_deref()).collect::<Vec<_>>().join("; ");
        if !details.is_empty() { item.message = message("configurationDetail",
            serde_json::json!({ "detail": asb_core::adapter::scrub_message(details) }), "当前客户端公共配置存在错误。"); }
        return item;
    }
    if !active {
        return check("configuration", "unknown", "inactiveConfiguration", "此供应商尚未激活；当前客户端配置不作为它的故障依据。", None);
    }
    match preview {
        Ok(file) if super::repairs::differs(file) =>
            check("configuration", "warning", "configurationDrift", "当前供应商配置与已保存档案的投影存在差异。",
                Some(("previewRepair", "预览修复差异，确认后重新应用当前供应商。"))),
        Ok(_) => check("configuration", "pass", "configurationMatches", "当前配置与已保存供应商投影一致。", None),
        Err(error) => error_check("configuration", error),
    }
}

pub(super) fn route(profile: &ProviderProfile, active: bool, observation: GatewayObservation, projection: Result<&crate::gateway::GatewayProjection, &CommandError>) -> ProviderDiagnosticCheck {
    let projection = match projection { Ok(projection) => projection, Err(error) => return error_check("route", error) };
    if !active { return check("route", "unknown", "inactiveRoute", "此供应商未激活；没有活动路由是正常状态。", None); }
    if !projection.plan.is_gateway() {
        return check("route", "pass", "directRoute", "此供应商使用客户端直连，无需本地协议网关。", None);
    }
    let served = observation.routes.iter().any(|route| route.app == profile.app && route.profile_id == profile.id);
    if matches!(observation.status, GatewayStatusKind::Running) && served && observation.listening_port == Some(observation.configured_port) {
        check("route", "pass", "gatewayReady", "本地网关正在监听，且当前供应商路由已加载。", None)
    } else {
        let mut item = check("route", "error", "gatewayUnavailable", "当前供应商需要本地网关，但监听器或路由尚未就绪。",
            Some(("openGateway", "打开网关页查看端口占用、恢复状态与路由诊断。")));
        let detail = observation.failure.as_ref().map(|failure| failure.message.clone())
            .or_else(|| observation.blocked_recovery.as_ref().map(|blocked| blocked.reason.clone()))
            .or(observation.repair_reason);
        item.message = message("gatewayFacts", serde_json::json!({
            "configuredPort": observation.configured_port,
            "listeningPort": observation.listening_port.map(|port| port.to_string()).unwrap_or_else(|| "—".into()),
            "routeKey": if served { "providerDiagnostics.backend.routePresent" } else { "providerDiagnostics.backend.routeMissing" },
            "stateKey": gateway_state_key(observation.status),
            "detail": asb_core::adapter::scrub_message(detail.unwrap_or_default()),
        }), "本地网关监听或路由尚未就绪。");
        item
    }
}

fn gateway_state_key(status: GatewayStatusKind) -> &'static str {
    match status {
        GatewayStatusKind::Standby => "providerDiagnostics.backend.gatewayStandby",
        GatewayStatusKind::Running => "providerDiagnostics.backend.gatewayRunning",
        GatewayStatusKind::PortConflict => "providerDiagnostics.backend.gatewayPortConflict",
        GatewayStatusKind::BindRejected => "providerDiagnostics.backend.gatewayBindRejected",
        GatewayStatusKind::NeedsRepair => "providerDiagnostics.backend.gatewayNeedsRepair",
        GatewayStatusKind::RecoveryBlocked => "providerDiagnostics.backend.gatewayRecoveryBlocked",
    }
}

fn error_check(category: &'static str, error: &CommandError) -> ProviderDiagnosticCheck {
    let mut item = check(category, "error", "projectionUnavailable", "无法为该供应商生成有效配置预览。",
        Some(("editProfile", "检查已保存供应商、通用配置与依赖的模型路由。")));
    item.message = asb_core::contracts::LocalizedMessage::new(
        error.message_key.unwrap_or("providerDiagnostics.backend.projectionDetail"),
        error.params.clone().unwrap_or_else(|| serde_json::json!({ "detail": error.message })), error.message.clone());
    item
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_reports_names_and_process_scope_only() {
        let result = environment_for(vec!["OPENAI_API_KEY", "HTTPS_PROXY"]);
        assert_eq!(result.status, "warning");
        assert_eq!(result.message.params.unwrap()["names"], "OPENAI_API_KEY, HTTPS_PROXY");
        assert_eq!(environment_for(vec![]).status, "unknown");
    }
}
