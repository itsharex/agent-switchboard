use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub(super) fn binary() -> PathBuf {
    if let Some(path) = std::env::var_os("ASB_TEST_CLAUDE_BIN") {
        return path.into();
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    root.join("node_modules/@anthropic-ai/claude-code/bin/claude.exe")
}

pub(super) fn run(
    config: &Path,
    work: &Path,
    home: &Path,
    prompt: &str,
    tools: &str,
    proxy: &str,
) -> Output {
    run_with_model(config, work, home, prompt, tools, proxy, None)
}

pub(super) fn run_with_model(
    config: &Path,
    work: &Path,
    home: &Path,
    prompt: &str,
    tools: &str,
    proxy: &str,
    model: Option<&str>,
) -> Output {
    run_with_deadline(config, work, home, prompt, tools, proxy, model, 75, true)
}

/// Like `run_with_model`, but a run that outlives its deadline is killed and
/// reported as a non-success output instead of panicking. Used where the
/// client is *expected* to be unable to complete (a dead gateway).
pub(super) fn run_with_deadline(
    config: &Path,
    work: &Path,
    home: &Path,
    prompt: &str,
    tools: &str,
    proxy: &str,
    model: Option<&str>,
    deadline_secs: u64,
    panic_on_timeout: bool,
) -> Output {
    let mut command = Command::new(binary());
    if let Some(model) = model {
        command.args(["--model", model]);
    }
    command
        .args([
            "-p",
            "--no-session-persistence",
            "--setting-sources",
            "user",
            "--effort",
            "max",
        ])
        .args([
            "--tools",
            "default",
            "--allowedTools",
            tools,
            "--strict-mcp-config",
            "--mcp-config",
            r#"{"mcpServers":{}}"#,
        ])
        .arg("--")
        .arg(prompt)
        .current_dir(work)
        .env_clear();
    inherit_runtime(&mut command);
    isolated_environment(&mut command, config, home, proxy);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("installed pinned Claude CLI");
    let stdout = capture(child.stdout.take().unwrap());
    let stderr = capture(child.stderr.take().unwrap());
    let deadline = Instant::now() + Duration::from_secs(deadline_secs);
    let status = loop {
        if let Some(status) = child.try_wait().expect("poll isolated Claude") {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let out = stdout.join().unwrap();
            let err = stderr.join().unwrap();
            if panic_on_timeout {
                let _ = child.wait();
                panic!(
                    "isolated Claude CLI timed out; stdout={}; stderr={}",
                    String::from_utf8_lossy(&out),
                    String::from_utf8_lossy(&err)
                );
            }
            let status = child.wait().expect("reap killed isolated Claude");
            return Output {
                status,
                stdout: out,
                stderr: err,
            };
        }
        thread::sleep(Duration::from_millis(25));
    };
    Output {
        status,
        stdout: stdout.join().unwrap(),
        stderr: stderr.join().unwrap(),
    }
}

fn capture(reader: impl Read + Send + 'static) -> thread::JoinHandle<Vec<u8>> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        reader.take(256 * 1024).read_to_end(&mut bytes).unwrap();
        bytes
    })
}

fn inherit_runtime(command: &mut Command) {
    for name in [
        "PATH",
        "SystemRoot",
        "WINDIR",
        "SystemDrive",
        "COMSPEC",
        "PATHEXT",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "CLAUDE_CODE_GIT_BASH_PATH",
    ] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
}

fn isolated_environment(command: &mut Command, config: &Path, home: &Path, proxy: &str) {
    for (name, path) in [
        ("HOME", home.to_path_buf()),
        ("USERPROFILE", home.to_path_buf()),
        ("APPDATA", home.join("AppData/Roaming")),
        ("LOCALAPPDATA", home.join("AppData/Local")),
        ("XDG_CONFIG_HOME", home.join("config")),
        ("XDG_DATA_HOME", home.join("data")),
        ("TEMP", home.join("tmp")),
        ("TMP", home.join("tmp")),
    ] {
        fs::create_dir_all(&path).unwrap();
        command.env(name, path);
    }
    command
        .env("CLAUDE_CONFIG_DIR", config)
        .env("CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC", "1")
        .env("DISABLE_TELEMETRY", "1")
        .env("DISABLE_ERROR_REPORTING", "1")
        .env("DISABLE_AUTOUPDATER", "1")
        .env("CLAUDE_CODE_DISABLE_AUTO_MEMORY", "1")
        .env("HTTP_PROXY", proxy)
        .env("HTTPS_PROXY", proxy)
        .env("ALL_PROXY", proxy)
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost");
}
