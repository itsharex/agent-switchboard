use std::collections::BTreeSet;

use crate::extensions::contracts::McpMetadata;

use super::error::{invalid, reject, ExtensionValidationError};

pub fn validate_mcp_metadata(metadata: &McpMetadata) -> Result<(), ExtensionValidationError> {
    for (value, label, limit) in [
        (&metadata.display_name, "显示名称", 128),
        (&metadata.description, "描述", 2000),
    ] {
        if let Some(value) = value {
            if value.trim().is_empty() || value.chars().count() > limit || value.contains('\0') {
                return reject(
                    "mcpMetadata",
                    format!("{label}须为 1 到 {limit} 个字符且不含空字符"),
                );
            }
        }
    }
    let mut seen = BTreeSet::new();
    if metadata.tags.len() > 32 {
        return reject("mcpMetadata", "标签最多 32 项");
    }
    for tag in &metadata.tags {
        if tag.trim().is_empty() || tag.chars().count() > 64 || tag.chars().any(char::is_control) {
            return reject("mcpMetadata", "标签须为 1 到 64 个字符且不含控制字符");
        }
        if !seen.insert(tag.trim()) {
            return reject("mcpMetadata", "标签不能重复");
        }
    }
    for (value, label) in [(&metadata.homepage, "主页"), (&metadata.docs, "文档链接")] {
        if let Some(value) = value {
            let error = || invalid("mcpMetadata", format!("{label}须为有效的 HTTP / HTTPS URL"));
            let url = url::Url::parse(value).map_err(|_| error())?;
            if value.len() > 2048
                || value.chars().any(char::is_control)
                || !matches!(url.scheme(), "http" | "https")
                || url.host_str().is_none()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(error());
            }
        }
    }
    Ok(())
}
