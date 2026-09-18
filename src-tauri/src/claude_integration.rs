//! Claude 客户端集成标记（Claude 专属）：Claude Code 插件 / VS Code 扩展读取的
//! `config.json` `primaryApiKey`，以及 `~/.claude.json` 的 `hasCompletedOnboarding`。
//! 每次写入只改这一个键，经写入锁、外改校验、备份与原子替换；备份放在独立目录，
//! 不进入配置切换的备份列表，也不参与切换事务日志。

use crate::local_state::LocalState;
use asb_core::contracts::{AppKind, ChangeKind, KeyChange, RouteMode};
use asb_switch::{execute_rendered, sha256_hex, FsIo, RenderedWriteRequest};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

const POLICY_FILE: &str = "claude-integration.json";
const BACKUP_DIR: &str = "backups/claude-integration";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ClaudeIntegrationFlag {
    /// `primaryApiKey: "any"` tells the Claude Code plugin an API key is
    /// configured, so a third-party route does not trigger its login prompt.
    Plugin,
    /// `hasCompletedOnboarding: true` skips the CLI's first-run onboarding.
    Onboarding,
}

impl ClaudeIntegrationFlag {
    fn target(self) -> Result<PathBuf, String> {
        match self {
            Self::Plugin => LocalState::claude_plugin_config_path(),
            Self::Onboarding => LocalState::claude_user_document_path(),
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Plugin => "primaryApiKey",
            Self::Onboarding => "hasCompletedOnboarding",
        }
    }

    fn value(self) -> Value {
        match self {
            Self::Plugin => Value::String("any".into()),
            Self::Onboarding => Value::Bool(true),
        }
    }

    fn reason(self) -> &'static str {
        match self {
            Self::Plugin => "claude-integration-plugin",
            Self::Onboarding => "claude-integration-onboarding",
        }
    }
}

/// Persisted preference: whether a Claude switch keeps the plugin marker in
/// step with the new route (custom → set, official → clear).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeIntegrationPolicy {
    #[serde(default)]
    pub plugin_integration: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeIntegrationFlagState {
    pub flag: ClaudeIntegrationFlag,
    pub target: String,
    pub exists: bool,
    pub applied: bool,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ClaudeIntegrationView {
    pub policy: ClaudeIntegrationPolicy,
    pub flags: Vec<ClaudeIntegrationFlagState>,
}

/// A side-effect-free plan for one marker change; `apply` recomputes it and
/// refuses when the document or the intent changed in between.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ClaudeIntegrationPreview {
    pub flag: ClaudeIntegrationFlag,
    pub enable: bool,
    pub target: String,
    pub content_hash: String,
    pub target_existed: bool,
    pub rendered_hash: String,
    pub changes: Vec<KeyChange>,
}

struct Document {
    text: String,
    exists: bool,
    fields: Map<String, Value>,
}

fn read_document(target: &Path) -> Result<Document, String> {
    let (text, exists) = match std::fs::read_to_string(target) {
        Ok(text) => (text, true),
        // The executor treats a missing Claude document as `{}`; the hash
        // the preview reports must match that reading.
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => ("{}".to_string(), false),
        Err(_) => return Err(format!("无法读取 {}", target.display())),
    };
    let fields = if text.trim().is_empty() {
        Map::new()
    } else {
        match serde_json::from_str::<Value>(&text) {
            Ok(Value::Object(fields)) => fields,
            _ => return Err(format!("{} 不是 JSON 对象，未改写该文件", target.display())),
        }
    };
    Ok(Document {
        text,
        exists,
        fields,
    })
}

fn flag_state(flag: ClaudeIntegrationFlag) -> Result<ClaudeIntegrationFlagState, String> {
    let target = flag.target()?;
    let document = read_document(&target)?;
    Ok(ClaudeIntegrationFlagState {
        flag,
        target: target.to_string_lossy().into(),
        exists: document.exists,
        applied: document.fields.get(flag.key()) == Some(&flag.value()),
        content_hash: sha256_hex(&document.text),
    })
}

pub(crate) fn view(root: &Path) -> Result<ClaudeIntegrationView, String> {
    Ok(ClaudeIntegrationView {
        policy: load_policy(root)?,
        flags: vec![
            flag_state(ClaudeIntegrationFlag::Plugin)?,
            flag_state(ClaudeIntegrationFlag::Onboarding)?,
        ],
    })
}

pub(crate) fn load_policy(root: &Path) -> Result<ClaudeIntegrationPolicy, String> {
    match crate::config_store::read_optional(&root.join(POLICY_FILE)) {
        Ok(Some(text)) => crate::config_store::parse_strict(&text)
            .map_err(|_| "Claude 客户端集成策略文件无效，请修复或删除后重试".to_string()),
        Ok(None) => Ok(ClaudeIntegrationPolicy::default()),
        Err(_) => Err("Claude 客户端集成策略文件不可读".into()),
    }
}

pub(crate) fn save_policy(
    root: &Path,
    policy: &ClaudeIntegrationPolicy,
) -> Result<ClaudeIntegrationView, String> {
    let text = serde_json::to_string_pretty(policy).map_err(|_| "策略无法编码")?;
    crate::config_store::write_json_atomic(&root.join(POLICY_FILE), &text)?;
    view(root)
}

pub(crate) fn preview(
    flag: ClaudeIntegrationFlag,
    enable: bool,
) -> Result<(ClaudeIntegrationPreview, String), String> {
    let target = flag.target()?;
    let document = read_document(&target)?;
    let before = document.fields.get(flag.key());
    let after = enable.then(|| flag.value());
    let changes = if before == after.as_ref() {
        Vec::new()
    } else {
        vec![KeyChange {
            key: flag.key().to_string(),
            kind: if enable {
                ChangeKind::Set
            } else {
                ChangeKind::Remove
            },
            before: before.map(Value::to_string),
            after: after.as_ref().map(Value::to_string),
        }]
    };
    let rendered = if changes.is_empty() {
        document.text.clone()
    } else {
        let mut fields = document.fields;
        match &after {
            Some(value) => {
                fields.insert(flag.key().to_string(), value.clone());
            }
            None => {
                fields.remove(flag.key());
            }
        }
        let mut text =
            serde_json::to_string_pretty(&Value::Object(fields)).map_err(|_| "文档无法编码")?;
        text.push('\n');
        text
    };
    Ok((
        ClaudeIntegrationPreview {
            flag,
            enable,
            target: target.to_string_lossy().into(),
            content_hash: sha256_hex(&document.text),
            target_existed: document.exists,
            rendered_hash: sha256_hex(&rendered),
            changes,
        },
        rendered,
    ))
}

/// Applies one previewed marker change. A preview with no changes is a
/// no-op that never touches the file.
pub(crate) fn apply(
    root: &Path,
    preview_plan: &ClaudeIntegrationPreview,
) -> Result<ClaudeIntegrationView, String> {
    let (current, rendered) = preview(preview_plan.flag, preview_plan.enable)?;
    if current != *preview_plan {
        return Err("Claude 客户端文件在预览后已变化，请重新预览".into());
    }
    if current.changes.is_empty() {
        return view(root);
    }
    let target = PathBuf::from(&current.target);
    execute_rendered(
        &FsIo,
        &RenderedWriteRequest {
            target: &target,
            app: AppKind::Claude,
            backup_dir: &root.join(BACKUP_DIR),
            expected_hash: &current.content_hash,
            expected_target_existed: current.target_existed,
            rendered: &rendered,
            reason: current.flag.reason(),
        },
        |_| Ok(()),
    )
    .map_err(|error| error.to_string())?;
    view(root)
}

/// After a Claude switch, keeps the plugin marker in step with the new
/// route when the policy asks for it. Returns whether the marker changed.
pub(crate) fn reconcile_after_switch(root: &Path, route_mode: RouteMode) -> Result<bool, String> {
    if !load_policy(root)?.plugin_integration {
        return Ok(false);
    }
    let (plan, _) = preview(
        ClaudeIntegrationFlag::Plugin,
        route_mode == RouteMode::Custom,
    )?;
    if plan.changes.is_empty() {
        return Ok(false);
    }
    apply(root, &plan).map(|_| true)
}

