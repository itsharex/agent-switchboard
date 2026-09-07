use crate::extensions::contracts::{DesiredState, ExtensionBinding};

/// Windows reserved device names that must never appear as a path segment.
const WINDOWS_RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Managed-content entry kinds as observed by the caller's walk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentEntryKind {
    File,
    Dir,
    /// Any symlink, junction, or reparse point. Only fully materializable
    /// regular directories are accepted; links are rejected, never followed.
    Link,
}

/// Limits applied when importing or staging managed content. They bound
/// extraction work before any target write happens.
pub const MAX_CONTENT_FILES: usize = 2000;
pub const MAX_CONTENT_TOTAL_BYTES: u64 = 64 * 1024 * 1024;
pub const MAX_CONTENT_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_CONTENT_DEPTH: usize = 16;

/// Validates one managed-content entry path against traversal, absolute
/// forms, reserved names, collisions, and the extraction limits. `path`
/// uses `/` separators and is relative to the content root.
pub fn validate_content_paths(
    entries: &[(String, ContentEntryKind, u64)],
) -> Result<(), Vec<String>> {
    let mut errors = Vec::new();
    if entries.len() > MAX_CONTENT_FILES {
        errors.push(format!(
            "内容包含 {} 个条目，超过上限 {MAX_CONTENT_FILES}",
            entries.len()
        ));
    }
    let mut total_bytes = 0u64;
    for (path, kind, size) in entries {
        if errors.len() >= 16 {
            break;
        }
        if let Some(error) = check_single_path(path) {
            errors.push(error);
            continue;
        }
        if *kind == ContentEntryKind::Link {
            errors.push(format!(
                "{path} 是链接或重解析点；只接受可完整物化的普通目录"
            ));
            continue;
        }
        if *kind == ContentEntryKind::File {
            if *size > MAX_CONTENT_FILE_BYTES {
                errors.push(format!("{path} 超过单文件大小上限"));
            }
            total_bytes += size;
        }
        if path.matches('/').count() + 1 > MAX_CONTENT_DEPTH {
            errors.push(format!("{path} 超过目录深度上限 {MAX_CONTENT_DEPTH}"));
        }
    }
    if total_bytes > MAX_CONTENT_TOTAL_BYTES {
        errors.push(format!("内容总大小超过上限 {MAX_CONTENT_TOTAL_BYTES} 字节"));
    }
    // Directory-entry collision pass: two entries whose full paths fold to
    // the same lowercase form collide on Windows.
    let mut folded_paths: Vec<String> = entries
        .iter()
        .map(|(path, _, _)| path.to_lowercase())
        .collect();
    folded_paths.sort();
    for pair in folded_paths.windows(2) {
        if pair[0] == pair[1] {
            errors.push(format!("路径 {} 在大小写不敏感文件系统上冲突", pair[0]));
            break;
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn check_single_path(path: &str) -> Option<String> {
    if path.is_empty() {
        return Some("内容包含空路径".to_string());
    }
    if path.starts_with('/') || path.starts_with('\\') {
        return Some(format!("{path} 是绝对路径，已拒绝"));
    }
    if path.contains(':') {
        return Some(format!("{path} 含盘符前缀，已拒绝"));
    }
    for segment in path.split('/') {
        if segment.is_empty() || segment == "." {
            return Some(format!("{path} 含空路径段"));
        }
        if segment == ".." {
            return Some(format!("{path} 试图越过内容根目录，已拒绝"));
        }
        if segment.contains('\\') || segment.contains('\0') {
            return Some(format!("{path} 含非法字符"));
        }
        let stem = segment.split('.').next().unwrap_or(segment);
        if WINDOWS_RESERVED_NAMES
            .iter()
            .any(|reserved| stem.eq_ignore_ascii_case(reserved))
        {
            return Some(format!("{path} 使用了 Windows 保留名 {stem}"));
        }
        if segment.ends_with(' ') || segment.ends_with('.') {
            return Some(format!("{path} 以空格或点结尾，Windows 上无法表示"));
        }
    }
    None
}

/// Whether a desired-state change is meaningful for a binding that never
/// deployed. Enabling an undeployed binding is a normal install; disabling
/// one is recorded without any client write.
pub fn disable_requires_client_write(binding: &ExtensionBinding) -> bool {
    binding.desired == DesiredState::Disabled && binding.last_applied_revision.is_some()
}
