use std::collections::BTreeMap;

use crate::extensions::contracts::{
    McpDefinition, SecretValue, SkillDependency, SkillManifest, SourceRef,
};
use crate::extensions::skill::ContentEntry;
use crate::extensions::validate::ContentEntryKind;

use crate::extensions::portable::package::{
    reject, PortableError, PortablePackage, PortablePayload, PORTABLE_PACKAGE_VERSION,
};

/// The result of validating one package for import: the material the
/// library needs, plus what the user must re-bind on this machine.
#[derive(Debug, Clone, PartialEq)]
pub enum ImportMaterial {
    Skill {
        manifest: SkillManifest,
        source: Option<SourceRef>,
        host_scoped: Option<crate::contracts::AppKind>,
        dependencies: Vec<String>,
        entries: Vec<ContentEntry>,
    },
    McpStdio {
        name: String,
        definition: McpDefinition,
        /// Env slot names whose values stayed behind on the exporting
        /// machine; the user must fill them before deploying.
        missing_env_slots: Vec<String>,
    },
}

/// Validates one parsed package and derives the import material. Paths are
/// checked against traversal; the version must match exactly.
pub fn prepare_import(package: &PortablePackage) -> Result<ImportMaterial, PortableError> {
    if package.schema_version != PORTABLE_PACKAGE_VERSION {
        return Err(reject(format!(
            "便携包版本 {} 不受支持（当前支持 {}）",
            package.schema_version, PORTABLE_PACKAGE_VERSION
        )));
    }
    match &package.payload {
        PortablePayload::Skill {
            name,
            manifest,
            files,
            source,
            host_scoped,
            dependencies,
        } => {
            if name.trim().is_empty() {
                return Err(reject("Skill 名称不能为空"));
            }
            let mut seen = std::collections::BTreeSet::new();
            let mut entries = Vec::new();
            for file in files {
                validate_portable_path(&file.path)?;
                if !seen.insert(file.path.clone()) {
                    return Err(reject(format!("{} 在包中重复", file.path)));
                }
                let bytes = file.decode().map_err(reject)?;
                if bytes.is_empty() {
                    return Err(reject(format!("{} 内容为空", file.path)));
                }
                entries.push(ContentEntry {
                    relative_path: file.path.clone(),
                    kind: ContentEntryKind::File,
                    bytes,
                    mode: 0o644,
                });
            }
            let skill_md = entries
                .iter()
                .find(|entry| entry.relative_path == "SKILL.md")
                .ok_or_else(|| reject("便携包缺少 SKILL.md"))?;
            if std::str::from_utf8(&skill_md.bytes).is_err() {
                return Err(reject("SKILL.md 必须是文本文件"));
            }
            Ok(ImportMaterial::Skill {
                manifest: manifest.clone(),
                source: source.clone(),
                host_scoped: *host_scoped,
                dependencies: dependencies.clone(),
                entries,
            })
        }
        PortablePayload::McpStdio {
            name,
            command,
            args,
            env_slot_names,
            codex_options,
        } => {
            crate::extensions::validate::validate_server_key(name)
                .map_err(|error| reject(error.message))?;
            if command.trim().is_empty() {
                return Err(reject("启动命令不能为空"));
            }
            let mut slots = std::collections::BTreeSet::new();
            for slot in env_slot_names {
                if slot.trim().is_empty() {
                    return Err(reject("凭据槽名不能为空"));
                }
                if !slots.insert(slot.clone()) {
                    return Err(reject(format!("凭据槽 {slot} 重复")));
                }
            }
            let definition = McpDefinition::Stdio {
                command: command.clone(),
                args: args.clone(),
                env: BTreeMap::new(),
                codex_options: codex_options.clone(),
            };
            crate::extensions::validate::validate_mcp_definition(&definition)
                .map_err(|error| reject(error.message))?;
            Ok(ImportMaterial::McpStdio {
                name: name.clone(),
                definition,
                missing_env_slots: env_slot_names.clone(),
            })
        }
    }
}

fn validate_portable_path(path: &str) -> Result<(), PortableError> {
    if path.is_empty() {
        return Err(reject("文件路径不能为空"));
    }
    if path.starts_with('/') || path.contains('\\') {
        return Err(reject(format!("{path} 不是相对路径")));
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." {
            return Err(reject(format!("{path} 含有非法路径段")));
        }
    }
    // Windows drive letters and reserved device names never appear in a
    // portable relative path.
    if path.len() >= 2 && path.as_bytes()[1] == b':' && path.as_bytes()[0].is_ascii_alphabetic() {
        return Err(reject(format!("{path} 含有盘符，不是相对路径")));
    }
    Ok(())
}

/// Rebuilds skill dependency declarations from portable names: every
/// dependency arrives unlinked (`resource_id: None`) and must be bound on
/// this machine.
pub fn portable_dependencies(names: &[String]) -> Vec<SkillDependency> {
    names
        .iter()
        .map(|name| SkillDependency {
            name: name.clone(),
            resource_id: None,
        })
        .collect()
}

/// Confirms that no exported env slot keeps a value: the exporter reduces
/// every stored position to a name. Used by tests as a contract lock.
pub fn exported_slots_carry_no_values(mcp: &McpDefinition) -> bool {
    match mcp {
        McpDefinition::Stdio { env, .. } => env.values().all(|value| match value {
            SecretValue::Plain { value } if value.trim().is_empty() => true,
            SecretValue::Plain { .. } => false,
            _ => true,
        }),
        _ => true,
    }
}
