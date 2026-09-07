use crate::extensions::contracts::{McpDefinition, SkillDefinition};
use crate::extensions::skill::ContentEntry;
use crate::extensions::validate::ContentEntryKind;

use crate::extensions::portable::package::{
    base64_encode, reject, PortableError, PortableFile, PortablePackage, PortablePayload,
    PORTABLE_PACKAGE_VERSION,
};

/// Builds the portable package of one skill definition and its immutable
/// content. Nothing sensitive exists in skill content by construction; the
/// manifest and source identity are included verbatim.
pub fn export_skill(
    definition: &SkillDefinition,
    entries: &[ContentEntry],
) -> Result<PortablePackage, PortableError> {
    let mut files = Vec::new();
    for entry in entries {
        if entry.kind == ContentEntryKind::Dir {
            continue;
        }
        if entry.kind == ContentEntryKind::Link {
            return Err(reject(format!(
                "{} 是链接；便携包只包含静态文件",
                entry.relative_path
            )));
        }
        let portable = match std::str::from_utf8(&entry.bytes) {
            Ok(text) => PortableFile {
                path: entry.relative_path.clone(),
                text: Some(text.to_string()),
                base64: None,
            },
            Err(_) => PortableFile {
                path: entry.relative_path.clone(),
                text: None,
                base64: Some(base64_encode(&entry.bytes)),
            },
        };
        files.push(portable);
    }
    if files.is_empty() {
        return Err(reject("Skill 内容为空，不能导出"));
    }
    if !files.iter().any(|file| file.path == "SKILL.md") {
        return Err(reject("Skill 内容缺少 SKILL.md，不能导出"));
    }
    Ok(PortablePackage {
        schema_version: PORTABLE_PACKAGE_VERSION,
        payload: PortablePayload::Skill {
            name: definition.manifest.name.clone(),
            manifest: definition.manifest.clone(),
            files,
            source: definition.source.clone(),
            host_scoped: definition.host_scoped,
            dependencies: definition
                .dependencies
                .iter()
                .map(|dependency| dependency.name.clone())
                .collect(),
        },
    })
}

/// Builds the portable package of one MCP definition. Remote transports
/// carry a service URL and are refused; stdio exports its launch structure
/// with credential slots reduced to names.
pub fn export_mcp(name: &str, mcp: &McpDefinition) -> Result<PortablePackage, PortableError> {
    let McpDefinition::Stdio {
        command,
        args,
        env,
        codex_options,
    } = mcp
    else {
        return Err(reject(
            "远程 MCP（HTTP/SSE/WebSocket）包含服务地址，不属于可移植内容；请在目标设备重新创建",
        ));
    };
    let mut env_slot_names: Vec<String> = env.keys().cloned().collect();
    env_slot_names.sort();
    Ok(PortablePackage {
        schema_version: PORTABLE_PACKAGE_VERSION,
        payload: PortablePayload::McpStdio {
            name: name.to_string(),
            command: command.clone(),
            args: args.clone(),
            env_slot_names,
            codex_options: codex_options.clone(),
        },
    })
}
