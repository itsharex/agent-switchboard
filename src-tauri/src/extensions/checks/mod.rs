//! MCP connection checks: registry, static diagnostics, and the probe
//! entry points. The stdio and HTTP transports live in their own modules.

use asb_core::extensions::contracts::{
    ExtensionTarget, McpCheckOutcome, McpCheckResult, McpDefinition, SecretValue,
};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Protocol versions the check speaks. The legacy probe requests the
/// original version and accepts any supported answer below the modern one;
/// the modern probe speaks the cursor-paginating protocol.
pub(super) const SUPPORTED_PROTOCOL_VERSIONS: [&str; 3] =
    ["2025-06-18", "2025-03-26", "2024-11-05"];
pub(super) const MODERN_PROTOCOL_VERSION: &str = "2025-06-18";
pub(super) const LEGACY_REQUESTED_PROTOCOL_VERSION: &str = "2024-11-05";

/// The discovery (initialize) phase gets its own slice of the check
/// deadline so a slow handshake cannot starve the listing phase.
pub(super) const DISCOVERY_DEADLINE: Duration = Duration::from_secs(10);

/// Overall wall-clock budget for one connection check.
pub(super) const TOTAL_DEADLINE: Duration = Duration::from_secs(30);

/// Per-HTTP-request budget inside one check.
pub(super) const HTTP_REQUEST_DEADLINE: Duration = Duration::from_secs(10);

/// Pagination cap for listing servers, tools, and prompts.
pub(super) const MAX_PAGES: u64 = 20;

/// Hard cap on bytes read from one server response or stdout chunk.
pub(super) const MAX_OUTPUT_BYTES: u64 = 1_048_576;

/// Per-line read cap for stdio JSON-RPC traffic.
pub(super) const MAX_LINE_BYTES: usize = 1_048_576;

/// Poll interval when waiting on the reader thread.
pub(super) const RECEIVE_POLL: Duration = Duration::from_millis(5);

#[derive(Default)]
pub struct CheckRegistry {
    next_id: AtomicU64,
    cancelled: Mutex<BTreeMap<String, Arc<AtomicBool>>>,
}

impl CheckRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&self) -> (String, Arc<AtomicBool>) {
        let id = format!("check-{}", self.next_id.fetch_add(1, Ordering::SeqCst) + 1);
        let flag = Arc::new(AtomicBool::new(false));
        self.cancelled
            .lock()
            .expect("registry")
            .insert(id.clone(), flag.clone());
        (id, flag)
    }

    pub fn cancel(&self, id: &str) -> bool {
        if let Some(flag) = self.cancelled.lock().expect("registry").get(id) {
            flag.store(true, Ordering::SeqCst);
            return true;
        }
        false
    }

    fn finish(&self, id: &str) {
        self.cancelled.lock().expect("registry").remove(id);
    }
}

/// Static validation diagnostics. No command runs and no endpoint is reached.
pub fn static_diagnostics(definition: &McpDefinition) -> Vec<String> {
    match definition {
        McpDefinition::Stdio { command, .. } if !command_resolves(command) => {
            vec![format!(
                "命令 {command} 未在 PATH 中找到；运行时可能无法启动"
            )]
        }
        _ => Vec::new(),
    }
}

fn command_resolves(command: &str) -> bool {
    if command.contains('/') || command.contains('\\') {
        return std::path::Path::new(command).is_file();
    }
    let path_env = std::env::var_os("PATH").unwrap_or_default();
    #[cfg(windows)]
    let extensions: Vec<String> = std::env::var("PATHEXT")
        .unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string())
        .split(';')
        .map(str::to_string)
        .collect();
    for directory in std::env::split_paths(&path_env) {
        let candidate = directory.join(command);
        #[cfg(windows)]
        {
            if candidate.is_file() {
                return true;
            }
            for extension in &extensions {
                if candidate
                    .with_file_name(format!("{command}{extension}"))
                    .is_file()
                {
                    return true;
                }
            }
        }
        #[cfg(not(windows))]
        if candidate.is_file() {
            return true;
        }
    }
    false
}

/// Materialized values for one check. They are not persisted or logged.
pub struct CheckMaterial {
    pub env: BTreeMap<String, String>,
    pub headers: BTreeMap<String, String>,
    pub bearer: Option<String>,
}

/// Resolves all values needed by this probe. An environment reference means
/// the process environment at check time. Substituting a literal reference
/// string into a child process would test the wrong configuration.
pub fn material_for(
    definition: &McpDefinition,
    resolve: &dyn Fn(&str) -> Option<String>,
) -> Result<CheckMaterial, String> {
    let resolve_value = |value: &SecretValue| -> Result<String, String> {
        match value {
            SecretValue::EnvRef { name } => {
                std::env::var(name).map_err(|_| format!("环境变量 {name} 无法解析"))
            }
            SecretValue::Plain { value } => Ok(value.clone()),
            SecretValue::SecretRef { reference } => {
                resolve(reference).ok_or_else(|| format!("凭据引用 {reference} 无法解析"))
            }
        }
    };
    let mut material = CheckMaterial {
        env: BTreeMap::new(),
        headers: BTreeMap::new(),
        bearer: None,
    };
    match definition {
        McpDefinition::Stdio { env, .. } => {
            for (name, value) in env {
                material.env.insert(name.clone(), resolve_value(value)?);
            }
        }
        McpDefinition::Http {
            headers, bearer, ..
        } => {
            for (name, value) in headers {
                material.headers.insert(name.clone(), resolve_value(value)?);
            }
            material.bearer = bearer.as_ref().map(resolve_value).transpose()?;
        }
        McpDefinition::ClaudeSse { headers, .. } | McpDefinition::ClaudeWs { headers, .. } => {
            for (name, value) in headers {
                material.headers.insert(name.clone(), resolve_value(value)?);
            }
        }
    }
    Ok(material)
}

struct ProbeOutcome {
    outcome: McpCheckOutcome,
    protocol_version: Option<String>,
    truncated: bool,
}

impl ProbeOutcome {
    fn failed(classification: &str, error: impl Into<String>) -> Self {
        Self {
            outcome: McpCheckOutcome::Failed {
                classification: classification.to_string(),
                error: error.into(),
            },
            protocol_version: None,
            truncated: false,
        }
    }

    fn cancelled(protocol_version: Option<String>) -> Self {
        Self {
            outcome: McpCheckOutcome::Cancelled,
            protocol_version,
            truncated: false,
        }
    }

    fn partial(
        protocol_version: Option<String>,
        error: impl Into<String>,
        truncated: bool,
    ) -> Self {
        Self {
            outcome: McpCheckOutcome::Partial {
                error: format!("握手成功但目录读取未完成：{}", error.into()),
            },
            protocol_version,
            truncated,
        }
    }
}

/// A child deadline always consumes the one check-wide budget.
#[derive(Clone)]
struct CheckDeadline {
    ends_at: Instant,
    cancel_flag: Arc<AtomicBool>,
}

impl CheckDeadline {
    fn new(cancel_flag: Arc<AtomicBool>) -> Self {
        Self {
            ends_at: Instant::now() + TOTAL_DEADLINE,
            cancel_flag,
        }
    }

    fn limited(&self, limit: Duration) -> Self {
        Self {
            ends_at: self.ends_at.min(Instant::now() + limit),
            cancel_flag: self.cancel_flag.clone(),
        }
    }

    fn cancelled(&self) -> bool {
        self.cancel_flag.load(Ordering::SeqCst)
    }

    fn remaining(&self) -> Result<Duration, CheckError> {
        if self.cancelled() {
            return Err(CheckError::cancelled());
        }
        self.ends_at
            .checked_duration_since(Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(CheckError::timed_out)
    }

    fn request_timeout(&self) -> Result<Duration, CheckError> {
        Ok(self.remaining()?.min(HTTP_REQUEST_DEADLINE))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CheckErrorKind {
    Cancelled,
    TimedOut,
    OutputLimit,
    Closed,
    Protocol,
    Transport,
}

#[derive(Debug)]
struct CheckError {
    kind: CheckErrorKind,
    message: String,
}

impl CheckError {
    fn cancelled() -> Self {
        Self {
            kind: CheckErrorKind::Cancelled,
            message: "检测已取消".to_string(),
        }
    }

    fn timed_out() -> Self {
        Self {
            kind: CheckErrorKind::TimedOut,
            message: "检测超时".to_string(),
        }
    }

    fn output_limit() -> Self {
        Self {
            kind: CheckErrorKind::OutputLimit,
            message: "输出超过大小上限".to_string(),
        }
    }

    fn closed() -> Self {
        Self {
            kind: CheckErrorKind::Closed,
            message: "连接在等待响应时关闭".to_string(),
        }
    }

    fn protocol(message: impl Into<String>) -> Self {
        Self {
            kind: CheckErrorKind::Protocol,
            message: message.into(),
        }
    }

    fn transport(message: impl Into<String>) -> Self {
        Self {
            kind: CheckErrorKind::Transport,
            message: message.into(),
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_check(
    check_id: &str,
    registry: &CheckRegistry,
    definition: &McpDefinition,
    target: &ExtensionTarget,
    definition_revision: u64,
    resolve_secret: &dyn Fn(&str) -> Option<String>,
) -> McpCheckResult {
    let started = Instant::now();
    let deadline = CheckDeadline::new(registry_flag(registry, check_id));
    let probe = match material_for(definition, resolve_secret) {
        Err(message) => ProbeOutcome::failed("material", message),
        Ok(material) => match definition {
            McpDefinition::Stdio { command, args, .. } => {
                // This process spawns the server the same way Claude Code
                // does, so Windows shell shims need the same `cmd /c` wrap.
                let wrapped = match asb_core::extensions::mcp::ClaudeHost::current() {
                    asb_core::extensions::mcp::ClaudeHost::Windows => {
                        asb_core::extensions::mcp::wrap_windows_launcher(command, args)
                    }
                    asb_core::extensions::mcp::ClaudeHost::Unix => None,
                };
                let (command, args) = wrapped
                    .as_ref()
                    .map(|(command, args)| (command.as_str(), args.as_slice()))
                    .unwrap_or((command.as_str(), args.as_slice()));
                check_stdio(command, args, &material.env, &deadline)
            }
            McpDefinition::Http { url, .. } => check_http(
                url,
                &material.headers,
                material.bearer.as_deref(),
                &deadline,
            ),
            McpDefinition::ClaudeSse { .. } | McpDefinition::ClaudeWs { .. } => ProbeOutcome {
                outcome: McpCheckOutcome::NeedsNativeConfirmation,
                protocol_version: None,
                truncated: false,
            },
        },
    };
    registry.finish(check_id);
    McpCheckResult {
        definition_id: String::new(),
        definition_revision,
        target: target.clone(),
        checked_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        protocol_version: probe.protocol_version,
        outcome: probe.outcome,
        duration_ms: started.elapsed().as_millis() as u64,
        truncated: probe.truncated,
    }
}

#[allow(clippy::too_many_arguments)]
pub fn run_check_for(
    definition_id: &str,
    definition_revision: u64,
    target: &ExtensionTarget,
    check_id: &str,
    registry: &CheckRegistry,
    definition: &McpDefinition,
    resolve_secret: &dyn Fn(&str) -> Option<String>,
) -> McpCheckResult {
    let mut result = run_check(
        check_id,
        registry,
        definition,
        target,
        definition_revision,
        resolve_secret,
    );
    result.definition_id = definition_id.to_string();
    result
}

fn registry_flag(registry: &CheckRegistry, check_id: &str) -> Arc<AtomicBool> {
    registry
        .cancelled
        .lock()
        .expect("registry")
        .get(check_id)
        .cloned()
        .unwrap_or_else(|| Arc::new(AtomicBool::new(false)))
}

use http::check_http;
use stdio::check_stdio;

mod http;
mod stdio;
#[cfg(test)]
mod tests;
