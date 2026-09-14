//! Native Claude cloud SDK configuration, never an Anthropic HTTP gateway disguise.
mod source;
use serde::{Deserialize, Serialize};
pub use source::from_config;
use std::collections::BTreeMap;

pub const MANIFEST: &str = "ASB_CLAUDE_NATIVE_KEYS";
pub const KINDS: [ClaudeNativeKind; 3] = [
    ClaudeNativeKind::Bedrock,
    ClaudeNativeKind::Vertex,
    ClaudeNativeKind::Foundry,
];
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ClaudeNativeKind {
    Bedrock,
    Vertex,
    Foundry,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeNative {
    pub kind: ClaudeNativeKind,
    #[serde(default)]
    pub environment: BTreeMap<String, String>,
}
impl std::fmt::Debug for ClaudeNative {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClaudeNative")
            .field("kind", &self.kind)
            .field(
                "environment_keys",
                &self.environment.keys().collect::<Vec<_>>(),
            )
            .finish()
    }
}
impl ClaudeNativeKind {
    pub fn flag(self) -> &'static str {
        match self {
            Self::Bedrock => "CLAUDE_CODE_USE_BEDROCK",
            Self::Vertex => "CLAUDE_CODE_USE_VERTEX",
            Self::Foundry => "CLAUDE_CODE_USE_FOUNDRY",
        }
    }
    pub fn base_key(self) -> &'static str {
        match self {
            Self::Bedrock => "ANTHROPIC_BEDROCK_BASE_URL",
            Self::Vertex => "ANTHROPIC_VERTEX_BASE_URL",
            Self::Foundry => "ANTHROPIC_FOUNDRY_BASE_URL",
        }
    }
    pub fn accepts(self, key: &str) -> bool {
        match self {
            Self::Bedrock => matches!(
                key,
                "AWS_REGION"
                    | "AWS_DEFAULT_REGION"
                    | "AWS_ACCESS_KEY_ID"
                    | "AWS_SECRET_ACCESS_KEY"
                    | "AWS_SESSION_TOKEN"
                    | "AWS_PROFILE"
                    | "AWS_SHARED_CREDENTIALS_FILE"
                    | "AWS_CONFIG_FILE"
                    | "AWS_BEARER_TOKEN_BEDROCK"
                    | "CLAUDE_CODE_SKIP_AWS_CRED_CACHE"
                    | "CLAUDE_CODE_AWS_CHAIN_RESOLVE_TIMEOUT_MS"
                    | "ANTHROPIC_SMALL_FAST_MODEL_AWS_REGION"
                    | "ANTHROPIC_BEDROCK_REGION_PREFIX"
            ),
            Self::Vertex => {
                matches!(
                    key,
                    "CLOUD_ML_REGION"
                        | "ANTHROPIC_VERTEX_PROJECT_ID"
                        | "GOOGLE_APPLICATION_CREDENTIALS"
                        | "CLAUDE_CODE_SKIP_VERTEX_AUTH"
                ) || key
                    .strip_prefix("VERTEX_REGION_CLAUDE_")
                    .is_some_and(|suffix| {
                        !suffix.is_empty()
                            && suffix
                                .bytes()
                                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
                    })
            }
            Self::Foundry => matches!(
                key,
                "ANTHROPIC_FOUNDRY_RESOURCE"
                    | "ANTHROPIC_FOUNDRY_API_KEY"
                    | "ANTHROPIC_FOUNDRY_AUTH_TOKEN"
                    | "AZURE_CLIENT_ID"
                    | "AZURE_TENANT_ID"
                    | "AZURE_CLIENT_SECRET"
                    | "AZURE_CLIENT_CERTIFICATE_PATH"
                    | "AZURE_CLIENT_CERTIFICATE_PASSWORD"
                    | "AZURE_FEDERATED_TOKEN_FILE"
                    | "AZURE_AUTHORITY_HOST"
            ),
        }
    }
}
impl ClaudeNative {
    pub fn validate(&self, base: Option<&str>) -> Result<(), String> {
        if let Some(base) = base {
            crate::endpoint::validate_base_url(base, crate::UpstreamProtocol::AnthropicMessages)?;
        }
        for (key, value) in &self.environment {
            if !self.kind.accepts(key) {
                return Err(format!("Claude 原生云配置不接受环境键 {key}"));
            }
            if value.is_empty() || value.len() > 8192 || value.chars().any(char::is_control) {
                return Err(format!("Claude 原生云环境键 {key} 的值无效"));
            }
        }
        if self.kind == ClaudeNativeKind::Bedrock {
            let id = self.environment.contains_key("AWS_ACCESS_KEY_ID");
            let secret = self.environment.contains_key("AWS_SECRET_ACCESS_KEY");
            if id != secret {
                return Err("Bedrock Access Key ID 与 Secret Access Key 必须同时提供".into());
            }
        }
        if self.kind == ClaudeNativeKind::Foundry
            && base.is_none()
            && !self.environment.contains_key("ANTHROPIC_FOUNDRY_RESOURCE")
        {
            return Err("Foundry 需要资源名称或原生服务根地址".into());
        }
        Ok(())
    }
    pub fn projected(&self, base: Option<&str>) -> BTreeMap<String, String> {
        let mut env = self.environment.clone();
        env.insert(self.kind.flag().into(), "1".into());
        if let Some(base) = base {
            env.insert(self.kind.base_key().into(), base.into());
        }
        env
    }
}
pub fn is_native_env(key: &str) -> bool {
    KINDS
        .iter()
        .any(|kind| kind.accepts(key) || key == kind.flag() || key == kind.base_key())
}
pub fn declared_keys(root: &serde_json::Value) -> Result<Vec<String>, String> {
    let Some(value) = root.pointer(&format!("/env/{MANIFEST}")) else {
        return Ok(Vec::new());
    };
    let text = value
        .as_str()
        .ok_or("Claude 原生配置所有权标记无效，原文件已保留")?;
    let keys: Vec<String> = serde_json::from_str(text)
        .map_err(|_| "Claude 原生配置所有权标记无法解析，原文件已保留")?;
    if keys.len() > 128 || keys.iter().any(|key| !is_native_env(key)) {
        return Err("Claude 原生配置所有权标记包含未知键，原文件已保留".into());
    }
    Ok(keys)
}

#[cfg(test)]
mod tests;
