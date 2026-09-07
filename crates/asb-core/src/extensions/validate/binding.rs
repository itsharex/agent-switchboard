use crate::contracts::AppKind;
use crate::extensions::contracts::{
    ExtensionBinding, ExtensionDefinition, ExtensionPayload, ExtensionTarget,
    EXTENSIONS_SCHEMA_VERSION,
};

use crate::extensions::validate::error::ExtensionValidationError;
use crate::extensions::validate::error::{invalid, reject};
use crate::extensions::validate::naming::{validate_server_key, validate_skill_name};

/// Validates a binding's target and per-kind fields against the definition
/// payload it points at. Scope support and host-scoped content limits are
/// enforced here at the backend boundary, not only in the UI.
pub fn validate_binding(
    definition: &ExtensionDefinition,
    binding: &ExtensionBinding,
) -> Result<(), ExtensionValidationError> {
    if binding.schema_version != EXTENSIONS_SCHEMA_VERSION {
        return reject("schemaVersion", "绑定文件版本不受支持");
    }
    validate_id(&binding.id, "id")?;
    if binding.resource_id != definition.id {
        return reject("resourceId", "绑定指向的扩展定义不匹配");
    }
    validate_target(&binding.target)?;
    if !target_scope_supported(&binding.target) {
        return reject("target", "Codex 没有项目私有配置文件，不能建立项目私有绑定");
    }
    match &definition.payload {
        ExtensionPayload::Skill(skill) => {
            if let Some(host) = skill.host_scoped {
                if host != binding.target.client() {
                    return reject(
                        "target",
                        "宿主专用内容只能绑定导入它的客户端；跨端部署前请创建通用规范的本地副本",
                    );
                }
            }
            let name = binding
                .deploy_name
                .as_deref()
                .ok_or_else(|| invalid("deployName", "Skill 绑定必须提供部署目录名"))?;
            validate_skill_name(name).map_err(|mut error| {
                error.field = "deployName";
                error
            })?;
            if binding.native_key.is_some() {
                return reject("nativeKey", "Skill 绑定不能携带 MCP 服务键");
            }
            if let Some(locked) = binding.locked_digest.as_deref() {
                if locked.trim().is_empty() {
                    return reject("lockedDigest", "固定的内容摘要不能为空");
                }
            }
        }
        ExtensionPayload::Mcp(mcp) => {
            let key = binding
                .native_key
                .as_deref()
                .ok_or_else(|| invalid("nativeKey", "MCP 绑定必须提供原生服务键"))?;
            validate_server_key(key).map_err(|mut error| {
                error.field = "nativeKey";
                error
            })?;
            if !mcp.supports_client(binding.target.client()) {
                return reject(
                    "target",
                    format!("{} 不能部署到该客户端", mcp.transport_label()),
                );
            }
            if binding.deploy_name.is_some() {
                return reject("deployName", "MCP 绑定不能携带部署目录名");
            }
            if binding.locked_digest.is_some() {
                return reject("lockedDigest", "MCP 绑定没有内容版本，不能固定摘要");
            }
        }
    }
    Ok(())
}

/// Validates scope composition. Project-private Codex targets are rejected
/// because Codex defines no private project document; the application never
/// invents one.
pub fn validate_target(target: &ExtensionTarget) -> Result<(), ExtensionValidationError> {
    match target {
        ExtensionTarget::App { .. } => Ok(()),
        ExtensionTarget::ProjectShared { project_id, .. }
        | ExtensionTarget::ProjectPrivate { project_id, .. } => validate_id(project_id, "target"),
    }
}

/// Claude allows a private per-project MCP document; Codex does not.
pub fn target_scope_supported(target: &ExtensionTarget) -> bool {
    !matches!(
        target,
        ExtensionTarget::ProjectPrivate {
            client: AppKind::Codex,
            ..
        }
    )
}

pub(super) fn validate_id(id: &str, field: &'static str) -> Result<(), ExtensionValidationError> {
    if id.trim().is_empty() {
        return reject(field, "标识符不能为空");
    }
    if !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return reject(field, "标识符只能包含 ASCII 字母、数字和连字符");
    }
    Ok(())
}
