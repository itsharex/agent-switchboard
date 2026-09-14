use std::collections::{BTreeMap, BTreeSet};

use crate::extensions::contracts::{McpDefinition, SecretValue};
use crate::redact::is_secret_value;

use crate::extensions::validate::error::ExtensionValidationError;
use crate::extensions::validate::error::{invalid, reject};

/// Skill names follow the portable specification: lowercase letters, digits,
/// and hyphens, at most 64 characters, without leading or trailing hyphens.
pub fn validate_skill_name(name: &str) -> Result<(), ExtensionValidationError> {
    if name.is_empty() || name.len() > 64 {
        return reject("name", "Skill 名称长度必须在 1 到 64 个字符之间");
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return reject("name", "Skill 名称只能包含小写字母、数字和连字符");
    }
    if name.starts_with('-') || name.ends_with('-') || name.contains("--") {
        return reject("name", "Skill 名称不能以连字符开头/结尾或包含连续连字符");
    }
    Ok(())
}

/// Native MCP server keys: `[A-Za-z0-9_-]+`（最长 64）。
pub fn validate_server_key(key: &str) -> Result<(), ExtensionValidationError> {
    if key.is_empty() || key.len() > 64 {
        return reject("nativeKey", "MCP 服务键长度必须在 1 到 64 个字符之间");
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return reject("nativeKey", "MCP 服务键只能包含字母、数字、下划线和连字符");
    }
    Ok(())
}

pub(crate) fn validate_mcp_definition(mcp: &McpDefinition) -> Result<(), ExtensionValidationError> {
    match mcp {
        McpDefinition::Stdio {
            command,
            args,
            env,
            codex_options,
        } => {
            if command.trim().is_empty() {
                return reject("command", "stdio 命令不能为空");
            }
            if command.contains('\0') || args.iter().any(|arg| arg.contains('\0')) {
                return reject("command", "命令与参数不能包含空字符");
            }
            for (name, value) in env {
                validate_env_name(name)?;
                validate_secret_value(value, "env")?;
            }
            if let Some(options) = codex_options {
                if let Some(cwd) = &options.cwd {
                    if cwd.trim().is_empty() || cwd.contains('\0') {
                        return reject("codexOptions", "Codex 工作目录不能为空或包含空字符");
                    }
                }
            }
            Ok(())
        }
        McpDefinition::Http {
            url,
            headers,
            bearer,
        } => {
            validate_remote_url(url, &["http", "https"])?;
            validate_headers(headers)?;
            if let Some(bearer) = bearer {
                if headers
                    .keys()
                    .any(|name| name.eq_ignore_ascii_case("authorization"))
                {
                    return reject("bearer", "Bearer 与 Authorization 请求头不能同时设置");
                }
                validate_secret_value(bearer, "bearer")?;
                validate_header_value(bearer, "bearer")?;
            }
            Ok(())
        }
        McpDefinition::ClaudeSse { url, headers } => {
            validate_remote_url(url, &["http", "https"])?;
            validate_headers(headers)
        }
        McpDefinition::ClaudeWs { url, headers } => {
            // Native Claude ws entries may carry websocket or plain http
            // scheme URLs; both survive import, so both must validate.
            validate_remote_url(url, &["ws", "wss", "http", "https"])?;
            validate_headers(headers)
        }
    }
}

fn validate_remote_url(url: &str, schemes: &[&str]) -> Result<(), ExtensionValidationError> {
    let parsed = url::Url::parse(url).map_err(|_| invalid("url", "MCP 服务地址无法解析"))?;
    if !schemes.contains(&parsed.scheme()) {
        return reject(
            "url",
            format!("MCP 服务地址必须使用 {} 之一", schemes.join(" / ")),
        );
    }
    Ok(())
}

/// Environment variable names: `[A-Za-z_][A-Za-z0-9_]*`。
pub fn validate_env_name(name: &str) -> Result<(), ExtensionValidationError> {
    let mut chars = name.chars();
    let valid = match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {
            chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        }
        _ => false,
    };
    if !valid {
        return reject("env", format!("环境变量名 {name} 不是合法名称"));
    }
    Ok(())
}

fn validate_header_name(name: &str) -> Result<(), ExtensionValidationError> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
    {
        return reject("headers", format!("请求头名称 {name} 不合法"));
    }
    Ok(())
}

fn validate_headers(
    headers: &BTreeMap<String, SecretValue>,
) -> Result<(), ExtensionValidationError> {
    let mut seen = BTreeSet::new();
    for (name, value) in headers {
        validate_header_name(name)?;
        if !seen.insert(name.to_ascii_lowercase()) {
            return reject("headers", format!("请求头 {name} 重复（名称不区分大小写）"));
        }
        validate_secret_value(value, "headers")?;
        validate_header_value(value, "headers")?;
    }
    Ok(())
}

fn validate_header_value(
    value: &SecretValue,
    field: &'static str,
) -> Result<(), ExtensionValidationError> {
    if matches!(value, SecretValue::Plain { value } if value.contains(['\r', '\n'])) {
        return reject(field, "请求头值不能包含换行");
    }
    Ok(())
}

fn validate_secret_value(
    value: &SecretValue,
    field: &'static str,
) -> Result<(), ExtensionValidationError> {
    match value {
        SecretValue::EnvRef { name } => validate_env_name(name).map_err(|mut error| {
            error.field = field;
            error
        }),
        SecretValue::SecretRef { reference } => {
            if reference.trim().is_empty() {
                return reject(field, "凭据引用不能为空");
            }
            Ok(())
        }
        SecretValue::Plain { value } => {
            if value.contains('\0') {
                return reject(field, "明文值不能包含空字符");
            }
            // Sensitive-shaped literals never enter the library through the
            // ordinary definition path; they must become secret references.
            if is_secret_value(value) {
                return reject(
                    field,
                    "该值疑似凭据；请通过专用凭据接口保存，不要写入扩展定义",
                );
            }
            Ok(())
        }
    }
}
