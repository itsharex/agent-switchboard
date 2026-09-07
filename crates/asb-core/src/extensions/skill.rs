//! Skill content handling: manifest extraction, content digests, and
//! compatibility diagnostics.
//!
//! Everything here is pure. The caller walks directories and passes file
//! bytes in; this module never touches the filesystem and never executes
//! anything found inside a skill (no scripts, no dynamic commands, no
//! referenced instructions, and no remote images).

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::contracts::AppKind;
use crate::extensions::contracts::{CompatibilityNote, SkillManifest};
use crate::extensions::validate::{self, ContentEntryKind, ExtensionValidationError};

/// One entry of managed content as observed by the caller's walk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentEntry {
    /// `/`-separated path relative to the content root.
    pub relative_path: String,
    pub kind: ContentEntryKind,
    /// File bytes; directories carry none.
    pub bytes: Vec<u8>,
    /// Unix mode bits; Windows captures the plain-file default.
    pub mode: u32,
}

impl ContentEntry {
    pub fn size(&self) -> u64 {
        self.bytes.len() as u64
    }
}

/// Stable digest of one content version. The canon covers each file's
/// relative path, entry kind, byte content, and business-meaningful mode
/// bits; modification times never participate.
pub fn content_digest(entries: &[ContentEntry]) -> String {
    let mut hasher = Sha256::new();
    let mut sorted: Vec<&ContentEntry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    for entry in sorted {
        let path = entry.relative_path.as_bytes();
        hasher.update((path.len() as u64).to_le_bytes());
        hasher.update(path);
        let kind_byte: u8 = match entry.kind {
            ContentEntryKind::File => b'F',
            ContentEntryKind::Dir => b'D',
            ContentEntryKind::Link => b'L',
        };
        hasher.update([kind_byte]);
        match entry.kind {
            ContentEntryKind::File => {
                let file_hash = Sha256::digest(&entry.bytes);
                hasher.update(file_hash);
            }
            _ => hasher.update([0u8; 32]),
        }
        hasher.update(entry.mode.to_le_bytes());
    }
    hex_lower(&hasher.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

/// The manifest of a skill that could not be fully parsed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestParseFailure {
    pub message: String,
}

/// Result of extracting a `SKILL.md` manifest.
#[derive(Debug, Clone, PartialEq)]
pub enum ManifestExtraction {
    Parsed(SkillManifest),
    /// No `SKILL.md` at the content root; the directory is not a skill.
    MissingManifest,
    /// A manifest exists but its frontmatter is unreadable. The raw content
    /// is still importable as bytes; the definition records the failure.
    Unreadable(ManifestParseFailure),
}

/// Extracts the manifest from one `SKILL.md` document. The parser accepts
/// the flat frontmatter subset skills actually use (scalar values and simple
/// lists); nested structures are recorded as unparsed keys and preserved
/// verbatim in the library copy.
pub fn extract_manifest(skill_md: &str) -> ManifestExtraction {
    let rest = match skill_md
        .strip_prefix("---\n")
        .or_else(|| skill_md.strip_prefix("---\r\n"))
    {
        Some(rest) => rest,
        None => return ManifestExtraction::MissingManifest,
    };
    let end = match rest.find("\n---").or_else(|| rest.find("\r\n---")) {
        Some(index) => index,
        None => {
            return ManifestExtraction::Unreadable(ManifestParseFailure {
                message: "frontmatter 未闭合".to_string(),
            })
        }
    };
    let frontmatter = &rest[..end];
    let mut name = None;
    let mut description = None;
    let mut license = None;
    let mut allowed_tools: Option<Vec<String>> = None;
    let mut unparsed_keys: Vec<String> = Vec::new();
    let mut current_list: Option<&mut Vec<String>> = None;
    for raw_line in frontmatter.lines() {
        let line = raw_line.trim_end_matches('\r');
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if line.starts_with(' ') || line.starts_with('\t') {
            // A continuation of the previous key. Only simple list items of
            // `allowed-tools` are interpreted; other nesting stays opaque.
            let item = line.trim();
            if let (Some(list), Some(item)) = (current_list.as_deref_mut(), item.strip_prefix("- "))
            {
                list.push(unquote(item.trim()));
            }
            continue;
        }
        current_list = None;
        let Some((key, value)) = line.split_once(':') else {
            unparsed_keys.push(line.trim().to_string());
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match (key, value.is_empty()) {
            ("name", false) => name = Some(unquote(value)),
            ("description", false) => description = Some(unquote(value)),
            ("license", false) => license = Some(unquote(value)),
            ("allowed-tools", false) => {
                allowed_tools = Some(
                    value
                        .split(',')
                        .map(|item| item.trim().to_string())
                        .collect(),
                );
            }
            ("allowed-tools", true) => {
                let list = allowed_tools.get_or_insert_with(Vec::new);
                current_list = Some(list);
            }
            // Multi-line scalars and nested maps stay opaque by design.
            _ => unparsed_keys.push(key.to_string()),
        }
    }
    let Some(name) = name else {
        return ManifestExtraction::Unreadable(ManifestParseFailure {
            message: "frontmatter 缺少 name 字段".to_string(),
        });
    };
    ManifestExtraction::Parsed(SkillManifest {
        name,
        description,
        license,
        allowed_tools,
        unparsed_keys,
    })
}

fn unquote(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

/// The name a deployed skill directory uses. The portable specification
/// binds directory layout to the manifest name, so display names never
/// choose deployment paths.
pub fn deploy_name_for(manifest: &SkillManifest) -> Result<String, ExtensionValidationError> {
    validate::validate_skill_name(&manifest.name)?;
    Ok(manifest.name.clone())
}

/// Builds the Claude-side frontmatter diagnostics for one manifest.
pub fn claude_compatibility_notes(manifest: &SkillManifest) -> Vec<CompatibilityNote> {
    let mut notes = Vec::new();
    if manifest.allowed_tools.is_some() {
        notes.push(CompatibilityNote {
            code: "claude-host-field".to_string(),
            message: "allowed-tools 是 Claude 专属字段；Codex 不读取该声明".to_string(),
        });
    }
    if manifest.description.is_none() {
        notes.push(CompatibilityNote {
            code: "host-scoped-frontmatter".to_string(),
            message: "缺少 description：内容按 Claude 规则有效，但不能作为通用 Skill 部署到 Codex"
                .to_string(),
        });
    }
    if !manifest.unparsed_keys.is_empty() {
        notes.push(CompatibilityNote {
            code: "unparsed-frontmatter".to_string(),
            message: format!(
                "以下 frontmatter 字段未解释并按原文保留：{}",
                manifest.unparsed_keys.join("、")
            ),
        });
    }
    notes
}

/// Whether the content may be deployed to the client as-is. Host-scoped
/// content requires a portable copy before a cross-client deployment.
pub fn deployable_to(
    manifest: &SkillManifest,
    host_scoped: Option<AppKind>,
    client: AppKind,
) -> Result<(), String> {
    if let Some(host) = host_scoped {
        if host != client {
            return Err(format!(
                "该内容仅满足 {host:?} 的加载规则；跨端部署前请创建符合通用规范的本地副本"
            ));
        }
        // The host's own loading rules govern content imported from it;
        // missing portable fields are expected and valid there.
        return Ok(());
    }
    if manifest.description.is_none() {
        return Err("内容缺少 description，只能部署到导入它的客户端".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORTABLE_SKILL_MD: &str =
        "---\nname: api-spec\ndescription: 撰写接口规范\n---\n\n# API 规范\n";

    #[test]
    fn digests_are_stable_and_content_sensitive() {
        let entry = |bytes: &[u8]| ContentEntry {
            relative_path: "SKILL.md".to_string(),
            kind: ContentEntryKind::File,
            bytes: bytes.to_vec(),
            mode: 0o644,
        };
        let a = content_digest(&[entry(b"one")]);
        let again = content_digest(&[entry(b"one")]);
        let different = content_digest(&[entry(b"two")]);
        assert_eq!(a, again);
        assert_ne!(a, different);
        // Order of the caller's walk never matters.
        let entries = vec![
            ContentEntry {
                relative_path: "a.md".to_string(),
                kind: ContentEntryKind::File,
                bytes: b"a".to_vec(),
                mode: 0o644,
            },
            ContentEntry {
                relative_path: "b.md".to_string(),
                kind: ContentEntryKind::File,
                bytes: b"b".to_vec(),
                mode: 0o644,
            },
        ];
        let reordered: Vec<ContentEntry> = entries.iter().rev().cloned().collect();
        assert_eq!(content_digest(&entries), content_digest(&reordered));
        // Mode bits participate in the canon.
        let mut exec = entries.clone();
        exec[0].mode = 0o755;
        assert_ne!(content_digest(&entries), content_digest(&exec));
    }

    #[test]
    fn manifests_extract_the_portable_fields() {
        let manifest = match extract_manifest(PORTABLE_SKILL_MD) {
            ManifestExtraction::Parsed(manifest) => manifest,
            other => panic!("expected parsed manifest, got {other:?}"),
        };
        assert_eq!(manifest.name, "api-spec");
        assert_eq!(manifest.description.as_deref(), Some("撰写接口规范"));
        assert_eq!(manifest.unparsed_keys, Vec::<String>::new());
    }

    #[test]
    fn claude_host_fields_stay_readable_but_scoped() {
        let document = "---\nname: helper\ndescription: h\nallowed-tools:\n  - Read\n  - Grep\nmetadata:\n  owner: team\n---\nbody";
        let manifest = match extract_manifest(document) {
            ManifestExtraction::Parsed(manifest) => manifest,
            other => panic!("expected parsed manifest, got {other:?}"),
        };
        assert_eq!(
            manifest.allowed_tools,
            Some(vec!["Read".to_string(), "Grep".to_string()])
        );
        assert!(manifest.unparsed_keys.contains(&"metadata".to_string()));
        let notes = claude_compatibility_notes(&manifest);
        assert!(notes.iter().any(|note| note.code == "claude-host-field"));
        assert!(notes.iter().any(|note| note.code == "unparsed-frontmatter"));
    }

    #[test]
    fn missing_and_broken_frontmatter_are_distinguishable() {
        assert_eq!(
            extract_manifest("# Just a document"),
            ManifestExtraction::MissingManifest
        );
        match extract_manifest("---\ndescription: no name\n---\n") {
            ManifestExtraction::Unreadable(failure) => {
                assert!(failure.message.contains("name"));
            }
            other => panic!("expected unreadable, got {other:?}"),
        }
        assert_eq!(
            extract_manifest("---\nname: x\n"),
            ManifestExtraction::Unreadable(ManifestParseFailure {
                message: "frontmatter 未闭合".to_string()
            })
        );
    }

    #[test]
    fn host_scoped_content_cannot_cross_clients_without_a_copy() {
        let manifest = SkillManifest {
            name: "helper".to_string(),
            description: None,
            license: None,
            allowed_tools: Some(vec!["Read".to_string()]),
            unparsed_keys: Vec::new(),
        };
        assert!(deployable_to(&manifest, Some(AppKind::Claude), AppKind::Claude).is_ok());
        let error = deployable_to(&manifest, Some(AppKind::Claude), AppKind::Codex).unwrap_err();
        assert!(error.contains("本地副本"));
    }

    #[test]
    fn deploy_names_come_from_the_manifest_name() {
        let manifest = SkillManifest {
            name: "api-spec".to_string(),
            description: Some("d".to_string()),
            license: None,
            allowed_tools: None,
            unparsed_keys: Vec::new(),
        };
        assert_eq!(deploy_name_for(&manifest).unwrap(), "api-spec");
        let mut invalid = manifest.clone();
        invalid.name = "Bad Name".to_string();
        assert!(deploy_name_for(&invalid).is_err());
    }
}
