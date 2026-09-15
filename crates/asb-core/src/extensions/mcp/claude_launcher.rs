//! Windows launcher wrapping for Claude Code stdio servers.
//!
//! Claude Code on native Windows spawns `command` directly, so Node package
//! launchers that are really `.cmd` shims (`npx`, `npm`, `node`, …) fail to
//! start unless the entry is written as `cmd /c <launcher> …`. The library
//! keeps the portable form; this module is the one place that wraps it for a
//! Windows Claude render and unwraps it again on import, so the same
//! definition projects correctly on every host and to Codex.

/// Launchers that only exist as shell shims on Windows.
pub const WINDOWS_SHELL_LAUNCHERS: [&str; 7] = ["npx", "npm", "yarn", "pnpm", "node", "bun", "deno"];

/// The host a Claude render targets. `current()` follows the compiled
/// platform; tests pick a host explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaudeHost {
    Windows,
    Unix,
}

impl ClaudeHost {
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Unix
        }
    }
}

/// The launcher a command refers to: `npx`, `NPX.CMD`, or `C:\tools\npx.cmd`
/// all name `npx`. Anything else is not a shell shim.
fn launcher_of(command: &str) -> Option<&'static str> {
    let file = command
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(command)
        .to_ascii_lowercase();
    let stem = file.strip_suffix(".cmd").unwrap_or(&file);
    WINDOWS_SHELL_LAUNCHERS
        .iter()
        .copied()
        .find(|launcher| *launcher == stem)
}

fn is_cmd(command: &str) -> bool {
    matches!(command.to_ascii_lowercase().as_str(), "cmd" | "cmd.exe")
}

/// Rewrites `<launcher> args…` into `cmd /c <launcher> args…`. Returns
/// `None` when the command is not a shim or is already wrapped.
pub fn wrap_windows_launcher(command: &str, args: &[String]) -> Option<(String, Vec<String>)> {
    if is_cmd(command) || launcher_of(command).is_none() {
        return None;
    }
    let mut wrapped = Vec::with_capacity(args.len() + 2);
    wrapped.push("/c".to_string());
    wrapped.push(command.to_string());
    wrapped.extend(args.iter().cloned());
    Some(("cmd".to_string(), wrapped))
}

/// Inverse of [`wrap_windows_launcher`]: `cmd /c <launcher> args…` becomes
/// `<launcher> args…`. Any other `cmd` invocation stays as written.
pub fn unwrap_windows_launcher(command: &str, args: &[String]) -> Option<(String, Vec<String>)> {
    if !is_cmd(command) {
        return None;
    }
    let [flag, launcher, rest @ ..] = args else {
        return None;
    };
    if !flag.eq_ignore_ascii_case("/c") || launcher_of(launcher).is_none() {
        return None;
    }
    Some((launcher.clone(), rest.to_vec()))
}
