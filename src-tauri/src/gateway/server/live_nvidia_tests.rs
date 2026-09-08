use crate::gateway::GatewayController;
use crate::local_state::LocalState;
use asb_core::contracts::{AppKind, ProviderDraft, RouteMode, SwitchPlan, UpstreamProtocol};
use asb_core::ownership::default_client_settings;
use asb_switch::io::FsIo;
use std::fs::{self, File};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const NVIDIA_BASE_URL: &str = "https://integrate.api.nvidia.com/v1";
const NVIDIA_MODEL: &str = "moonshotai/kimi-k3";
const TEST_TIMEOUT: Duration = Duration::from_secs(600);

struct CapturedOutput {
    status: ExitStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[test]
#[ignore = "requires ASB_NVIDIA_TEST_API_KEY plus installed Codex and Claude Code CLIs"]
fn nvidia_kimi_k3_completes_through_both_isolated_clients() {
    let api_key = std::env::var("ASB_NVIDIA_TEST_API_KEY")
        .expect("set ASB_NVIDIA_TEST_API_KEY only for this isolated live test");
    run_codex(&api_key);
    run_claude(&api_key);
}

fn nvidia_draft(app: AppKind, api_key: &str) -> ProviderDraft {
    ProviderDraft {
        parameters: asb_core::ownership::default_provider_parameters(app),
        app,
        route_mode: RouteMode::Custom,
        name: "NVIDIA Kimi K3 live sandbox".to_string(),
        base_url: Some(NVIDIA_BASE_URL.to_string()),
        api_key: api_key.to_string(),
        upstream_protocol: Some(UpstreamProtocol::ChatCompletions),
        responses_options: None,
        max_output_tokens: None.into(),
        model: Some(NVIDIA_MODEL.to_string()),
        model_options: None,
        notes: None,
        website_url: None,
        usage_query: None,
        official_quota_refresh_interval_minutes: None,
    }
}

fn wait_for_child(
    mut child: Child,
    client: &str,
    stdout_path: &Path,
    stderr_path: &Path,
) -> Result<CapturedOutput, CapturedOutput> {
    let deadline = Instant::now() + TEST_TIMEOUT;
    let mut timed_out = false;
    loop {
        if child
            .try_wait()
            .unwrap_or_else(|_| panic!("无法轮询 {client} 实际进程"))
            .is_some()
        {
            break;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            timed_out = true;
            break;
        }
        thread::sleep(Duration::from_millis(100));
    }
    let output = CapturedOutput {
        status: child
            .wait()
            .unwrap_or_else(|_| panic!("无法等待 {client} 实际进程结束")),
        stdout: fs::read(stdout_path).unwrap_or_default(),
        stderr: fs::read(stderr_path).unwrap_or_default(),
    };
    if timed_out {
        Err(output)
    } else {
        Ok(output)
    }
}

fn safe_output(output: &CapturedOutput, api_key: &str, loopback_token: &str) -> (String, String) {
    let redact = |value: &[u8]| {
        let value = String::from_utf8_lossy(value)
            .replace(api_key, "<redacted-nvidia-key>")
            .replace(loopback_token, "<redacted-loopback-token>");
        let maximum = 8_192;
        if value.chars().count() <= maximum {
            value
        } else {
            format!(
                "{}…<truncated>",
                value.chars().take(maximum).collect::<String>()
            )
        }
    };
    (redact(&output.stdout), redact(&output.stderr))
}

fn run_codex(api_key: &str) {
    Command::new("codex")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("NVIDIA live test requires the Codex CLI");

    let directory = tempfile::tempdir().expect("temporary Codex NVIDIA sandbox");
    let state = LocalState::from_root(directory.path().join("state"));
    let record = state
        .configuration()
        .create_provider(nvidia_draft(AppKind::Codex, api_key))
        .expect("create temporary Codex NVIDIA provider");
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project(&SwitchPlan::direct(
            record.profile,
            default_client_settings(AppKind::Codex),
        ))
        .expect("project temporary Codex NVIDIA route");

    let codex_home = directory.path().join("codex-home");
    let workdir = directory.path().join("work");
    fs::create_dir_all(&codex_home).expect("create temporary Codex home");
    fs::create_dir_all(&workdir).expect("create temporary Codex workdir");
    let config = codex_home.join("config.toml");
    let auth = codex_home.join("auth.json");
    fs::write(&auth, r#"{"auth_mode":"chatgpt","tokens":{"access_token":"sandbox-access","refresh_token":"sandbox-refresh","id_token":"sandbox-id"}}"#).expect("isolated login fixture");
    apply_fixture(&directory, &config, &projection, &gateway);
    let rendered = fs::read_to_string(&config).expect("read temporary Codex settings");
    assert!(!rendered.contains(api_key));

    let stdout_path = directory.path().join("codex.stdout.log");
    let stderr_path = directory.path().join("codex.stderr.log");
    let child = Command::new("codex")
        .args([
            "exec",
            "--ephemeral",
            "--disable",
            "plugins",
            "--skip-git-repo-check",
            "Reply with exactly ASB_NVIDIA_CODEX_OK and no other text.",
        ])
        .current_dir(workdir)
        .env("CODEX_HOME", &codex_home)
        .env_remove("OPENAI_API_KEY")
        .env_remove("ASB_NVIDIA_TEST_API_KEY")
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost")
        .stdout(Stdio::from(
            File::create(&stdout_path).expect("create Codex stdout capture"),
        ))
        .stderr(Stdio::from(
            File::create(&stderr_path).expect("create Codex stderr capture"),
        ))
        .spawn()
        .expect("start isolated Codex NVIDIA request");
    let output = wait_for_child(child, "Codex", &stdout_path, &stderr_path).unwrap_or_else(
        |output| {
            let (stdout, stderr) = safe_output(&output, api_key, &projection.plan.profile.api_key);
            panic!(
                "Codex 通过 NVIDIA Kimi K3 的隔离请求在 10 分钟内没有完成；stdout={stdout:?}; stderr={stderr:?}"
            );
        },
    );
    let (stdout, stderr) = safe_output(&output, api_key, &projection.plan.profile.api_key);
    assert!(
        output.status.success(),
        "Codex NVIDIA request failed; stdout={stdout:?}; stderr={stderr:?}"
    );
    assert!(stdout.contains("ASB_NVIDIA_CODEX_OK"));
    assert!(!stdout.contains(api_key));
    assert!(!stderr.contains(api_key));
    gateway.shutdown();
}

fn run_claude(api_key: &str) {
    Command::new("claude.cmd")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .expect("NVIDIA live test requires the Claude Code CLI");

    let directory = tempfile::tempdir().expect("temporary Claude NVIDIA sandbox");
    let state = LocalState::from_root(directory.path().join("state"));
    let record = state
        .configuration()
        .create_provider(nvidia_draft(AppKind::Claude, api_key))
        .expect("create temporary Claude NVIDIA provider");
    let gateway = GatewayController::start(&state);
    let projection = gateway
        .project(&SwitchPlan::direct(
            record.profile,
            default_client_settings(AppKind::Claude),
        ))
        .expect("project temporary Claude NVIDIA route");

    let claude_config = directory.path().join("claude-config");
    let workdir = directory.path().join("work");
    let home = directory.path().join("home");
    fs::create_dir_all(&claude_config).expect("create temporary Claude config");
    fs::create_dir_all(&workdir).expect("create temporary Claude workdir");
    fs::create_dir_all(&home).expect("create temporary Claude home");
    let settings = claude_config.join("settings.json");
    apply_fixture(&directory, &settings, &projection, &gateway);
    let rendered = fs::read_to_string(&settings).expect("read temporary Claude settings");
    assert!(!rendered.contains(api_key));
    assert!(rendered.contains("CLAUDE_CODE_DISABLE_EXPERIMENTAL_BETAS"));

    let stdout_path = directory.path().join("claude.stdout.log");
    let stderr_path = directory.path().join("claude.stderr.log");
    let child = Command::new("claude.cmd")
        .args([
            "-p",
            "--no-session-persistence",
            "--effort",
            "low",
            "--setting-sources",
            "user",
            "Reply with exactly ASB_NVIDIA_CLAUDE_OK and no other text.",
        ])
        .current_dir(workdir)
        .env("CLAUDE_CONFIG_DIR", &claude_config)
        .env("HOME", &home)
        .env("USERPROFILE", &home)
        .env("APPDATA", home.join("AppData"))
        .env("LOCALAPPDATA", home.join("LocalAppData"))
        .env_remove("ANTHROPIC_API_KEY")
        .env_remove("ANTHROPIC_AUTH_TOKEN")
        .env_remove("ANTHROPIC_BASE_URL")
        .env_remove("ASB_NVIDIA_TEST_API_KEY")
        .env("NO_PROXY", "127.0.0.1,localhost")
        .env("no_proxy", "127.0.0.1,localhost")
        .stdout(Stdio::from(
            File::create(&stdout_path).expect("create Claude stdout capture"),
        ))
        .stderr(Stdio::from(
            File::create(&stderr_path).expect("create Claude stderr capture"),
        ))
        .spawn()
        .expect("start isolated Claude NVIDIA request");
    let output = wait_for_child(child, "Claude Code", &stdout_path, &stderr_path).unwrap_or_else(
        |output| {
            let (stdout, stderr) = safe_output(&output, api_key, &projection.plan.profile.api_key);
            panic!(
                "Claude Code 通过 NVIDIA Kimi K3 的隔离请求在 10 分钟内没有完成；stdout={stdout:?}; stderr={stderr:?}"
            );
        },
    );
    let (stdout, stderr) = safe_output(&output, api_key, &projection.plan.profile.api_key);
    assert!(
        output.status.success(),
        "Claude Code NVIDIA request failed; stdout={stdout:?}; stderr={stderr:?}"
    );
    assert!(stdout.contains("ASB_NVIDIA_CLAUDE_OK"));
    assert!(!stdout.contains(api_key));
    assert!(!stderr.contains(api_key));
    gateway.shutdown();
}

fn apply_fixture(
    directory: &tempfile::TempDir,
    target: &std::path::Path,
    projection: &crate::gateway::GatewayProjection,
    gateway: &GatewayController,
) {
    let backup_dir = directory.path().join("backups");
    let io = FsIo;
    let preview =
        asb_switch::read_preview(&io, target, &projection.plan, &backup_dir.to_string_lossy())
            .expect("preview isolated gateway switch");
    let committed_projection = projection.clone();
    let committed_gateway = gateway.clone();
    asb_switch::execute(
        &io,
        &asb_switch::SwitchRequest {
            target: target,
            plan: &projection.plan,
            backup_dir: &backup_dir,
            expected_hash: &preview.content_hash,
            expected_rendered_hash: &preview.rendered_hash,
        },
        move |_| committed_gateway.commit(&committed_projection, || Ok(())),
    )
    .expect("execute isolated gateway switch");
}
