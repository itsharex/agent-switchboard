//! Freshness and content verification: document hashes, rendered syntax,
//! managed-manifest comparison, and the plan validity window.

use std::path::Path;

use asb_core::extensions::contracts::{DocumentSyntax, ManagedFileEntry};
use asb_core::extensions::plan::ExtensionPlan;
use asb_core::extensions::skill::{content_digest, ContentEntry};
use asb_core::extensions::validate::ContentEntryKind;

use super::{directory_digest_marker, sha256_bytes_hex, ExtensionError};
use crate::io::{PathKind, SwitchIo};

pub(super) fn document_hash(
    io: &dyn SwitchIo,
    path: &Path,
) -> Result<Option<String>, ExtensionError> {
    match io.path_kind(path).map_err(|error| {
        ExtensionError::preparation(format!("无法读取 {}：{error}", path.display()))
    })? {
        PathKind::Absent => Ok(None),
        PathKind::File { .. } => {
            let bytes = io.read_bytes(path).map_err(|error| {
                ExtensionError::preparation(format!("无法读取 {}：{error}", path.display()))
            })?;
            Ok(Some(sha256_bytes_hex(&bytes)))
        }
        PathKind::Directory | PathKind::Other => Err(ExtensionError::preparation(format!(
            "{} 不是普通文件，不能作为配置文档写入",
            path.display()
        ))),
    }
}

pub(super) fn validate_rendered_syntax(
    syntax: DocumentSyntax,
    rendered: &str,
) -> Result<(), String> {
    match syntax {
        DocumentSyntax::Toml => rendered
            .parse::<toml_edit::DocumentMut>()
            .map(|_| ())
            .map_err(|error| format!("渲染结果不是有效 TOML：{error}")),
        DocumentSyntax::Json => serde_json::from_str::<serde_json::Value>(rendered)
            .map(|_| ())
            .map_err(|error| format!("渲染结果不是有效 JSON：{error}")),
    }
}

pub(super) fn verify_managed_files(
    entries: &[ContentEntry],
    expected: &[ManagedFileEntry],
    what: &str,
) -> Result<String, ExtensionError> {
    let mut actual_files: Vec<ManagedFileEntry> = entries
        .iter()
        .map(|entry| ManagedFileEntry {
            relative_path: entry.relative_path.clone(),
            digest: if entry.kind == ContentEntryKind::Dir {
                directory_digest_marker()
            } else {
                sha256_bytes_hex(&entry.bytes)
            },
            mode: entry.mode,
            size: entry.bytes.len() as u64,
        })
        .collect();
    actual_files.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    let mut expected_sorted: Vec<&ManagedFileEntry> = expected.iter().collect();
    expected_sorted.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    let matches = actual_files.len() == expected_sorted.len()
        && actual_files
            .iter()
            .zip(expected_sorted.iter())
            .all(|(actual, expected)| actual == *expected);
    if !matches {
        return Err(ExtensionError::PlanStale {
            message: format!("{what} 的内容与准备计划时不一致；请重新预览"),
        });
    }
    Ok(content_digest(entries))
}

/// The plan's validity window, checked under lock before any write. A plan
/// past its expiry is stale: the user previews again.
pub(super) fn verify_plan_freshness<Io: SwitchIo>(
    io: &Io,
    plan: &ExtensionPlan,
) -> Result<(), String> {
    let now = io.now_rfc3339();
    let expired = chrono::DateTime::parse_from_rfc3339(&plan.preconditions.expires_at)
        .map(|expires| {
            chrono::DateTime::parse_from_rfc3339(&now)
                .map(|now| now > expires)
                .unwrap_or(false)
        })
        .unwrap_or(true);
    if expired {
        return Err(format!(
            "计划已于 {} 过期；请重新预览后应用",
            plan.preconditions.expires_at
        ));
    }
    Ok(())
}
