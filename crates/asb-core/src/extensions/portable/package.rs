use serde::{Deserialize, Serialize};

use crate::extensions::contracts::{CodexServerOptions, SkillManifest, SourceRef};

/// Version of the portable package format. Strict parsing: a mismatched
/// version is rejected, never guessed.
pub const PORTABLE_PACKAGE_VERSION: u16 = 1;

/// One content file of an exported skill. Text files embed their text;
/// non-UTF-8 payloads travel base64-encoded and are marked as such.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PortableFile {
    /// `/`-separated path relative to the skill root; validated against
    /// traversal on import.
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base64: Option<String>,
}

impl PortableFile {
    /// The decoded bytes of the file, or an error naming the problem.
    pub fn decode(&self) -> Result<Vec<u8>, String> {
        match (&self.text, &self.base64) {
            (Some(text), None) => Ok(text.clone().into_bytes()),
            (None, Some(encoded)) => {
                base64_decode(encoded).ok_or_else(|| format!("{} 的 base64 内容无效", self.path))
            }
            _ => Err(format!("{} 必须且只能包含 text 或 base64 之一", self.path)),
        }
    }
}

/// A minimal, dependency-free base64 decoder so the contract stays pure.
pub(super) fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut accumulator: u32 = 0;
    let mut bits: u32 = 0;
    let mut out = Vec::new();
    for byte in input.bytes() {
        if byte == b'=' || byte == b'\n' || byte == b'\r' {
            continue;
        }
        let value = TABLE.iter().position(|candidate| *candidate == byte)? as u32;
        accumulator = (accumulator << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((accumulator >> bits) & 0xff) as u8);
        }
    }
    Some(out)
}

pub(super) fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in input.chunks(3) {
        let byte_0 = chunk[0] as u32;
        let byte_1 = chunk.get(1).map_or(0, |byte| *byte as u32);
        let byte_2 = chunk.get(2).map_or(0, |byte| *byte as u32);
        let triple = (byte_0 << 16) | (byte_1 << 8) | byte_2;
        out.push(TABLE[(triple >> 18) as usize & 0x3f] as char);
        out.push(TABLE[(triple >> 12) as usize & 0x3f] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(triple >> 6) as usize & 0x3f] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[triple as usize & 0x3f] as char
        } else {
            '='
        });
    }
    out
}

/// The discriminated package payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum PortablePayload {
    Skill {
        name: String,
        manifest: SkillManifest,
        #[serde(default)]
        files: Vec<PortableFile>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<SourceRef>,
        /// Non-sensitive host fact: the content only satisfies one client's
        /// native loading rules.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        host_scoped: Option<crate::contracts::AppKind>,
        /// Declared dependency names only; library links are re-created on
        /// the importing machine.
        #[serde(default)]
        dependencies: Vec<String>,
    },
    /// A stdio MCP skeleton: the launch structure travels, credential slots
    /// are named but empty.
    McpStdio {
        name: String,
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env_slot_names: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        codex_options: Option<CodexServerOptions>,
    },
}

/// The on-disk portable package. Everything in it is safe to share.
/// Strictness lives at the payload variants (`deny_unknown_fields` cannot
/// combine with the flattened tagged union); an unknown `kind` still fails
/// the union.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PortablePackage {
    pub schema_version: u16,
    #[serde(flatten)]
    pub payload: PortablePayload,
}

/// Why a package cannot be exported or imported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableError {
    pub message: String,
}

pub(super) fn reject(message: impl Into<String>) -> PortableError {
    PortableError {
        message: message.into(),
    }
}
