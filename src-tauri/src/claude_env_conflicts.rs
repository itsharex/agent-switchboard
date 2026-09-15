//! Claude 环境变量冲突（Claude 专属）：系统环境（Windows 注册表 HKCU/HKLM 的
//! `Environment` 键）或 shell 启动文件里的 `ANTHROPIC*` 变量会盖过 settings.json 中的
//! 供应商路由。扫描只读；删除前把完整值备份到应用状态目录，恢复只接受该目录下的
//! 备份。返回给界面的值经统一脱敏。

use asb_core::redact;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const PREFIX: &str = "ANTHROPIC";
const BACKUP_DIR: &str = "backups/claude-env";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeEnvConflict {
    pub var_name: String,
    /// Secret-shaped values are redacted; only the backup keeps the real value.
    pub value_preview: String,
    pub source: ClaudeEnvSource,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub(crate) enum ClaudeEnvSource {
    /// A Windows registry environment hive; elsewhere the current process
    /// environment, which can only be reported.
    System { location: String },
    /// One `export` line of a shell start-up file (1-based line).
    File { path: String, line: usize },
}

/// Full-value entry used by backups and deletion; never returned to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Entry {
    var_name: String,
    value: String,
    source: ClaudeEnvSource,
}

impl Entry {
    fn conflict(&self) -> ClaudeEnvConflict {
        ClaudeEnvConflict {
            var_name: self.var_name.clone(),
            value_preview: redact::redact(&self.var_name, &self.value),
            source: self.source.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeEnvScan {
    pub conflicts: Vec<ClaudeEnvConflict>,
    /// Digest of the full scan; a removal must echo it back.
    pub revision: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeEnvSelection {
    pub var_name: String,
    pub source: ClaudeEnvSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BackupFile {
    version: u32,
    created_at: String,
    entries: Vec<Entry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeEnvBackup {
    pub file_name: String,
    pub created_at: String,
    pub entries: Vec<ClaudeEnvConflict>,
}

fn matches(name: &str) -> bool {
    name.to_ascii_uppercase().starts_with(PREFIX)
}

fn revision(entries: &[Entry]) -> String {
    asb_switch::sha256_hex(&serde_json::to_string(entries).unwrap_or_default())
}

pub(crate) fn scan() -> Result<ClaudeEnvScan, String> {
    let entries = collect()?;
    Ok(ClaudeEnvScan {
        conflicts: entries.iter().map(Entry::conflict).collect(),
        revision: revision(&entries),
    })
}

fn collect() -> Result<Vec<Entry>, String> {
    let mut entries = platform::system_entries()?;
    entries.extend(shell_entries(&platform::shell_files()));
    Ok(entries)
}

/// Parses one shell line of the form `export NAME=value` / `NAME=value`.
fn parse_export(line: &str) -> Option<(String, String)> {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return None;
    }
    let assignment = trimmed.strip_prefix("export ").unwrap_or(trimmed);
    let (name, value) = assignment.split_once('=')?;
    let name = name.trim();
    let valid = !name.is_empty()
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || character == '_');
    if !valid {
        return None;
    }
    let value = value.trim();
    let value = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(value);
    Some((name.to_string(), value.to_string()))
}

fn shell_entries(paths: &[PathBuf]) -> Vec<Entry> {
    let mut entries = Vec::new();
    for path in paths {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        for (index, line) in text.lines().enumerate() {
            if let Some((name, value)) = parse_export(line).filter(|(name, _)| matches(name)) {
                entries.push(Entry {
                    var_name: name,
                    value,
                    source: ClaudeEnvSource::File {
                        path: path.to_string_lossy().into(),
                        line: index + 1,
                    },
                });
            }
        }
    }
    entries
}

/// Backs up then removes the selected variables. The scan revision must
/// still match so a stale selection never deletes a different value.
pub(crate) fn remove(
    root: &Path,
    selections: &[ClaudeEnvSelection],
    expected_revision: &str,
) -> Result<ClaudeEnvBackup, String> {
    remove_selected(root, collect()?, selections, expected_revision)
}

fn remove_selected(
    root: &Path,
    entries: Vec<Entry>,
    selections: &[ClaudeEnvSelection],
    expected_revision: &str,
) -> Result<ClaudeEnvBackup, String> {
    if revision(&entries) != expected_revision {
        return Err("环境变量已变化，请重新扫描后再删除".into());
    }
    if selections.is_empty() {
        return Err("没有选择要删除的环境变量".into());
    }
    let mut chosen = Vec::new();
    for selection in selections {
        let entry = entries
            .iter()
            .find(|entry| entry.var_name == selection.var_name && entry.source == selection.source)
            .ok_or_else(|| format!("扫描结果中没有 {}，请重新扫描", selection.var_name))?;
        if !platform::can_remove(&entry.source) {
            return Err(format!(
                "{} 来自当前进程环境，无法在此删除；请在启动它的 shell 中移除",
                entry.var_name
            ));
        }
        chosen.push(entry.clone());
    }
    let backup = write_backup(root, &chosen)?;
    // Higher lines first so earlier line numbers stay valid within one file.
    chosen.sort_by(|left, right| match (&left.source, &right.source) {
        (
            ClaudeEnvSource::File { path: a, line: x },
            ClaudeEnvSource::File { path: b, line: y },
        ) => a.cmp(b).then(y.cmp(x)),
        (ClaudeEnvSource::File { .. }, _) => std::cmp::Ordering::Greater,
        (_, ClaudeEnvSource::File { .. }) => std::cmp::Ordering::Less,
        _ => std::cmp::Ordering::Equal,
    });
    for entry in &chosen {
        delete_entry(entry).map_err(|error| {
            format!(
                "删除 {} 失败：{error}；备份已保存为 {}",
                entry.var_name, backup.file_name
            )
        })?;
    }
    Ok(backup)
}

fn write_backup(root: &Path, entries: &[Entry]) -> Result<ClaudeEnvBackup, String> {
    let created_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let file_name = format!(
        "env-{}.json",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%.3fZ")
    );
    let file = BackupFile {
        version: 1,
        created_at: created_at.clone(),
        entries: entries.to_vec(),
    };
    let text = serde_json::to_string_pretty(&file).map_err(|_| "环境变量备份无法编码")?;
    crate::config_store::write_json_atomic(&root.join(BACKUP_DIR).join(&file_name), &text)
        .map_err(|error| format!("环境变量备份写入失败：{error}"))?;
    Ok(ClaudeEnvBackup {
        file_name,
        created_at,
        entries: entries.iter().map(Entry::conflict).collect(),
    })
}

fn delete_entry(entry: &Entry) -> Result<(), String> {
    match &entry.source {
        ClaudeEnvSource::File { path, line } => {
            remove_export_line(Path::new(path), *line, &entry.var_name)
        }
        ClaudeEnvSource::System { location } => platform::delete_system(location, &entry.var_name),
    }
}

fn remove_export_line(path: &Path, line: usize, name: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|error| format!("无法读取 {}：{error}", path.display()))?;
    let mut lines: Vec<&str> = text.lines().collect();
    let index = line.checked_sub(1).filter(|index| *index < lines.len());
    let still_there = index
        .is_some_and(|index| parse_export(lines[index]).is_some_and(|(found, _)| found == name));
    if !still_there {
        return Err(format!(
            "{} 第 {line} 行已不再定义 {name}，未改写该文件",
            path.display()
        ));
    }
    lines.remove(index.expect("validated index"));
    let mut rendered = lines.join("\n");
    if text.ends_with('\n') {
        rendered.push('\n');
    }
    std::fs::write(path, rendered).map_err(|error| format!("无法写入 {}：{error}", path.display()))
}

fn append_export(path: &Path, name: &str, value: &str) -> Result<(), String> {
    let mut text = std::fs::read_to_string(path).unwrap_or_default();
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    text.push_str(&format!(
        "export {name}='{}'\n",
        value.replace('\'', "'\\''")
    ));
    std::fs::write(path, text).map_err(|error| format!("无法写入 {}：{error}", path.display()))
}

fn backup_path(root: &Path, file_name: &str) -> Result<PathBuf, String> {
    let valid = file_name.starts_with("env-")
        && file_name.ends_with(".json")
        && !file_name.contains(['/', '\\']);
    if !valid {
        return Err("环境变量备份名称无效".into());
    }
    Ok(root.join(BACKUP_DIR).join(file_name))
}

pub(crate) fn list_backups(root: &Path) -> Result<Vec<ClaudeEnvBackup>, String> {
    let directory = root.join(BACKUP_DIR);
    let entries = match std::fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("环境变量备份目录不可读".into()),
    };
    let mut backups = Vec::new();
    for entry in entries.flatten() {
        let file_name = entry.file_name().to_string_lossy().to_string();
        if backup_path(root, &file_name).is_err() {
            continue;
        }
        let Ok(file) = read_backup(&entry.path()) else {
            continue;
        };
        backups.push(ClaudeEnvBackup {
            file_name,
            created_at: file.created_at,
            entries: file.entries.iter().map(Entry::conflict).collect(),
        });
    }
    backups.sort_by(|left, right| right.file_name.cmp(&left.file_name));
    Ok(backups)
}

fn read_backup(path: &Path) -> Result<BackupFile, String> {
    let text = std::fs::read_to_string(path).map_err(|_| "环境变量备份不可读".to_string())?;
    let file: BackupFile =
        serde_json::from_str(&text).map_err(|_| "环境变量备份格式无效".to_string())?;
    if file.version != 1 {
        return Err("环境变量备份版本不受支持".into());
    }
    Ok(file)
}

/// Re-creates every variable recorded in one backup. Returns the count.
pub(crate) fn restore(root: &Path, file_name: &str) -> Result<usize, String> {
    let file = read_backup(&backup_path(root, file_name)?)?;
    for entry in &file.entries {
        match &entry.source {
            ClaudeEnvSource::File { path, .. } => {
                append_export(Path::new(path), &entry.var_name, &entry.value)?
            }
            ClaudeEnvSource::System { location } => {
                platform::set_system(location, &entry.var_name, &entry.value)?
            }
        }
    }
    Ok(file.entries.len())
}

#[cfg(windows)]
mod platform {
    use super::{matches, ClaudeEnvSource, Entry};
    use std::path::PathBuf;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE};
    use winreg::{RegKey, HKEY};

    const HIVES: [(&str, HKEY, &str); 2] = [
        (
            "HKEY_CURRENT_USER\\Environment",
            HKEY_CURRENT_USER,
            "Environment",
        ),
        (
            "HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
            HKEY_LOCAL_MACHINE,
            "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
        ),
    ];

    fn hive(location: &str, access: u32) -> Result<RegKey, String> {
        let (_, root, path) = HIVES
            .iter()
            .find(|(name, _, _)| *name == location)
            .ok_or("未知的注册表环境位置")?;
        RegKey::predef(*root)
            .open_subkey_with_flags(path, access)
            .map_err(|error| format!("无法打开 {location}（系统级需要管理员权限）：{error}"))
    }

    pub(super) fn system_entries() -> Result<Vec<Entry>, String> {
        let mut entries = Vec::new();
        for (location, _, _) in HIVES {
            let Ok(key) = hive(location, KEY_READ) else {
                continue;
            };
            for (name, value) in key.enum_values().flatten() {
                if matches(&name) {
                    entries.push(Entry {
                        var_name: name,
                        value: value.to_string(),
                        source: ClaudeEnvSource::System {
                            location: location.to_string(),
                        },
                    });
                }
            }
        }
        Ok(entries)
    }

    pub(super) fn shell_files() -> Vec<PathBuf> {
        Vec::new()
    }

    pub(super) fn can_remove(_source: &ClaudeEnvSource) -> bool {
        true
    }

    pub(super) fn delete_system(location: &str, name: &str) -> Result<(), String> {
        hive(location, KEY_SET_VALUE)?
            .delete_value(name)
            .map_err(|error| format!("删除注册表值失败：{error}"))
    }

    pub(super) fn set_system(location: &str, name: &str, value: &str) -> Result<(), String> {
        hive(location, KEY_SET_VALUE)?
            .set_value(name, &value.to_string())
            .map_err(|error| format!("恢复注册表值失败：{error}"))
    }
}

#[cfg(not(windows))]
mod platform {
    use super::{matches, ClaudeEnvSource, Entry};
    use std::path::PathBuf;

    const LOCATION: &str = "当前进程环境";

    pub(super) fn system_entries() -> Result<Vec<Entry>, String> {
        Ok(std::env::vars()
            .filter(|(name, _)| matches(name))
            .map(|(name, value)| Entry {
                var_name: name,
                value,
                source: ClaudeEnvSource::System {
                    location: LOCATION.into(),
                },
            })
            .collect())
    }

    pub(super) fn shell_files() -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(home) = crate::local_state::user_home_dir() {
            for name in [
                ".bashrc",
                ".bash_profile",
                ".zshrc",
                ".zprofile",
                ".profile",
            ] {
                files.push(home.join(name));
            }
        }
        files.push(PathBuf::from("/etc/profile"));
        files.push(PathBuf::from("/etc/bashrc"));
        files
    }

    pub(super) fn can_remove(source: &ClaudeEnvSource) -> bool {
        matches!(source, ClaudeEnvSource::File { .. })
    }

    pub(super) fn delete_system(_location: &str, name: &str) -> Result<(), String> {
        Err(format!("{name} 来自当前进程环境，无法在此删除"))
    }

    pub(super) fn set_system(_location: &str, name: &str, _value: &str) -> Result<(), String> {
        Err(format!("{name} 来自当前进程环境，无法在此恢复"))
    }
}

#[cfg(test)]
mod tests;
