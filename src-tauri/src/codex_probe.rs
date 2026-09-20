//! The Codex degradation probe: batch-tests the active client configuration
//! with one fixed question and reports per-run pass results plus reasoning
//! tokens. Only real Codex usage can answer this, so every run is opt-in,
//! lands in the normal usage report, and stays cancellable.

use crate::codex_metering::{summarize_token_usage, SessionTokenSummary};
use crate::process_control::{suppress_window, terminate_process_tree};
use serde::Serialize;
use std::collections::BTreeSet;
use std::io::BufReader;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Wall-clock budget for one probe run; a run that exceeds it is killed and
/// recorded as a failed run, not a probe failure.
const PER_RUN_TIMEOUT: Duration = Duration::from_secs(240);
/// Cadence when polling the Codex child process.
const WAIT_POLL: Duration = Duration::from_millis(50);
/// The rollout file is written by the CLI as the session ends; a short retry
/// covers filesystem timing after process exit.
const SESSION_FILE_RETRIES: usize = 3;
const SESSION_FILE_RETRY_DELAY: Duration = Duration::from_millis(400);

/// Upper bound accepted from the renderer for one probe's run count.
pub(crate) const MAX_RUN_COUNT: u32 = 10;
/// `question_id` that selects the user-authored question.
pub(crate) const CUSTOM_QUESTION_ID: &str = "custom";
const MAX_CUSTOM_PROMPT_CHARS: usize = 4000;

// ----------------------------------------------------------------- questions

/// One built-in probe question. Answers were verified by hand; the probe only
/// looks for the answer as a standalone number in the CLI output.
pub(crate) struct ProbeQuestion {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    prompt: &'static str,
    expected_answer: &'static str,
}

pub(crate) const QUESTIONS: [ProbeQuestion; 3] = [
    ProbeQuestion {
        id: "divisible-or-contains-3",
        label: "计数题",
        prompt: "从 1 到 100（含两端）的整数中，有多少个数能被 3 整除，或者十进制写法中至少含有一个数字 3？只输出最终数字。",
        expected_answer: "45",
    },
    ProbeQuestion {
        id: "mountain-round-trip",
        label: "行程题",
        prompt: "某人沿同一条山路上山后原路下山：上山速度 5 km/h，下山速度 7 km/h，往返一共用了 4 小时 48 分钟。山路单程长多少公里？只输出最终数字。",
        expected_answer: "14",
    },
    ProbeQuestion {
        id: "round-table-neighbours",
        label: "圆桌题",
        prompt: "5 个人围一张圆桌就坐（旋转后相同的坐法视为同一种），其中甲、乙两人必须相邻。共有多少种不同的坐法？只输出最终数字。",
        expected_answer: "12",
    },
];

/// The question one probe runs, after request validation.
pub(crate) struct ResolvedQuestion {
    pub(crate) label: String,
    prompt: String,
    expected_answer: String,
}

pub(crate) fn resolve_question(
    id: &str,
    custom_question: Option<String>,
    custom_answer: Option<String>,
) -> Result<ResolvedQuestion, String> {
    if id == CUSTOM_QUESTION_ID {
        let prompt = custom_question.unwrap_or_default().trim().to_string();
        if prompt.is_empty() {
            return Err("自定义题目不能为空".to_string());
        }
        if prompt.chars().count() > MAX_CUSTOM_PROMPT_CHARS {
            return Err(format!("自定义题目超过 {MAX_CUSTOM_PROMPT_CHARS} 字上限"));
        }
        let answer = custom_answer.unwrap_or_default().trim().to_string();
        if answer.is_empty() {
            return Err("自定义题目需要填写期望的整数答案".to_string());
        }
        if !answer.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("期望答案必须是非负整数".to_string());
        }
        return Ok(ResolvedQuestion {
            label: "自定义题".to_string(),
            prompt,
            expected_answer: answer,
        });
    }
    let question = QUESTIONS
        .iter()
        .find(|question| question.id == id)
        .ok_or_else(|| "未知的探针题目".to_string())?;
    Ok(ResolvedQuestion {
        label: question.label.to_string(),
        prompt: question.prompt.to_string(),
        expected_answer: question.expected_answer.to_string(),
    })
}

/// True when the expected integer appears as a standalone number: no ASCII
/// digit on either side, the same judgement the community candy eval uses.
pub(crate) fn answer_present(output: &str, expected: &str) -> bool {
    let expected = expected.trim();
    if expected.is_empty() || !expected.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    let bytes = output.as_bytes();
    let mut from = 0;
    while let Some(found) = output[from..].find(expected) {
        let start = from + found;
        let end = start + expected.len();
        let left_standalone = start == 0 || !bytes[start - 1].is_ascii_digit();
        let right_standalone = end == bytes.len() || !bytes[end].is_ascii_digit();
        if left_standalone && right_standalone {
            return true;
        }
        from = start + 1;
    }
    false
}

// -------------------------------------------------------------------- status

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum ProbePhase {
    Running,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRunResult {
    pub passed: bool,
    pub reasoning_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub model: Option<String>,
    pub duration_ms: u64,
    /// Machine-level failure for this run (timeout, exit code). Provider
    /// output never reaches this string.
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeStatus {
    pub phase: ProbePhase,
    pub question_label: String,
    pub run_count: u32,
    pub completed_runs: u32,
    pub started_at: String,
    pub runs: Vec<ProbeRunResult>,
    pub error: Option<String>,
}

// ------------------------------------------------------------------ registry

/// One probe slot: a second start is rejected while a probe runs, and the
/// finished status stays readable until the next start replaces it.
#[derive(Clone, Default)]
pub(crate) struct ProbeRegistry {
    slot: Arc<Mutex<Option<ProbeState>>>,
    next_id: Arc<AtomicU64>,
}

struct ProbeState {
    id: String,
    /// Present while the worker may still be running.
    cancel: Option<Arc<AtomicBool>>,
    status: ProbeStatus,
}

impl ProbeRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn begin(
        &self,
        question: &ResolvedQuestion,
        run_count: u32,
    ) -> Result<(String, Arc<AtomicBool>), String> {
        let mut slot = self.slot.lock().expect("probe registry");
        if let Some(state) = slot.as_ref() {
            if state.cancel.is_some() {
                return Err("已有一次降智检测在运行，请先取消或等待完成".to_string());
            }
        }
        let id = format!("probe-{}", self.next_id.fetch_add(1, Ordering::SeqCst) + 1);
        let cancel = Arc::new(AtomicBool::new(false));
        let status = ProbeStatus {
            phase: ProbePhase::Running,
            question_label: question.label.clone(),
            run_count,
            completed_runs: 0,
            started_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            runs: Vec::new(),
            error: None,
        };
        *slot = Some(ProbeState {
            id: id.clone(),
            cancel: Some(cancel.clone()),
            status,
        });
        Ok((id, cancel))
    }

    pub(crate) fn status(&self, id: &str) -> Option<ProbeStatus> {
        self.slot
            .lock()
            .expect("probe registry")
            .as_ref()
            .filter(|state| state.id == id)
            .map(|state| state.status.clone())
    }

    pub(crate) fn cancel(&self, id: &str) -> bool {
        let mut slot = self.slot.lock().expect("probe registry");
        match slot.as_mut() {
            Some(state) if state.id == id => {
                let Some(flag) = state.cancel.as_ref() else {
                    return false;
                };
                flag.store(true, Ordering::SeqCst);
                true
            }
            _ => false,
        }
    }

    fn push_run(&self, id: &str, result: ProbeRunResult) {
        let mut slot = self.slot.lock().expect("probe registry");
        if let Some(state) = slot.as_mut() {
            if state.id == id {
                state.status.runs.push(result);
                state.status.completed_runs = state.status.runs.len() as u32;
            }
        }
    }

    fn finish(&self, id: &str, phase: ProbePhase, error: Option<String>) {
        let mut slot = self.slot.lock().expect("probe registry");
        if let Some(state) = slot.as_mut() {
            if state.id == id {
                state.status.phase = phase;
                state.status.error = error;
                state.cancel = None;
            }
        }
    }
}

/// Starts the worker thread for one probe batch and returns its id.
pub(crate) fn spawn_probe(
    registry: &ProbeRegistry,
    question: ResolvedQuestion,
    run_count: u32,
) -> Result<String, String> {
    let (id, cancel) = registry.begin(&question, run_count)?;
    let registry = registry.clone();
    let worker_id = id.clone();
    std::thread::spawn(move || {
        for _ in 0..run_count {
            if cancel.load(Ordering::SeqCst) {
                registry.finish(&worker_id, ProbePhase::Cancelled, None);
                return;
            }
            match run_once(&question, &cancel) {
                Ok(result) => registry.push_run(&worker_id, result),
                Err(ProbeAbort::Cancelled) => {
                    registry.finish(&worker_id, ProbePhase::Cancelled, None);
                    return;
                }
                Err(ProbeAbort::Spawn(message)) => {
                    registry.finish(&worker_id, ProbePhase::Failed, Some(message));
                    return;
                }
            }
        }
        registry.finish(&worker_id, ProbePhase::Completed, None);
    });
    Ok(id)
}

// --------------------------------------------------------------------- runner

/// Fatal conditions that end the whole probe instead of one run.
#[derive(Debug)]
enum ProbeAbort {
    Cancelled,
    Spawn(String),
}

fn run_once(question: &ResolvedQuestion, cancel: &AtomicBool) -> Result<ProbeRunResult, ProbeAbort> {
    let started = Instant::now();
    let before = snapshot_session_files();
    let workdir = fresh_workdir().map_err(|_| ProbeAbort::Spawn("无法创建临时工作目录".to_string()))?;
    let mut command = Command::new(codex_executable());
    // The probe inherits the process environment on purpose: it must test
    // exactly the configuration the user's terminal Codex would use.
    command
        .args(["exec", "--skip-git-repo-check", &question.prompt])
        .current_dir(&workdir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    suppress_window(&mut command);
    let mut child = command
        .spawn()
        .map_err(|_| ProbeAbort::Spawn("无法启动 Codex CLI，请确认已安装并在 PATH 中".to_string()))?;
    // Drain both pipes on reader threads: a child whose output exceeds the
    // OS pipe buffer would otherwise block on write and never exit.
    let stdout_reader = child.stdout.take().map(drain_pipe);
    let stderr_reader = child.stderr.take().map(drain_pipe);
    let deadline = started + PER_RUN_TIMEOUT;
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) => {}
            Err(_) => {
                terminate_process_tree(&mut child);
                return Err(ProbeAbort::Spawn("无法监测 Codex 进程".to_string()));
            }
        }
        if cancel.load(Ordering::SeqCst) {
            terminate_process_tree(&mut child);
            return Err(ProbeAbort::Cancelled);
        }
        if Instant::now() >= deadline {
            terminate_process_tree(&mut child);
            timed_out = true;
            break;
        }
        std::thread::sleep(WAIT_POLL);
    }
    let exit = child
        .wait()
        .map_err(|_| ProbeAbort::Spawn("无法收集 Codex 退出状态".to_string()))?;
    let _ = std::fs::remove_dir_all(&workdir);
    let stdout = stdout_reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();
    let stderr = stderr_reader
        .and_then(|reader| reader.join().ok())
        .unwrap_or_default();
    let combined = format!("{stdout}\n{stderr}");
    let passed = answer_present(&combined, &question.expected_answer);
    let error = if timed_out {
        Some("单次运行超时".to_string())
    } else if !exit.success() {
        Some(format!("Codex 进程退出码 {}", exit.code().unwrap_or(-1)))
    } else {
        None
    };
    let summary = wait_for_new_session_summary(&before);
    Ok(ProbeRunResult {
        passed,
        reasoning_tokens: summary.as_ref().and_then(|summary| summary.reasoning_output),
        total_tokens: summary.as_ref().and_then(|summary| summary.total),
        model: summary.and_then(|summary| summary.model),
        duration_ms: started.elapsed().as_millis() as u64,
        error,
    })
}

/// Resolves the Codex launcher without relying on an interactive shell. The
/// npm wrapper is a batch file, which `Command` cannot find by bare name.
pub(crate) fn codex_executable() -> PathBuf {
    if let Some(explicit) = std::env::var_os("ASB_TEST_CODEX_BIN") {
        return PathBuf::from(explicit);
    }
    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            for name in ["codex.exe", "codex.cmd", "codex.bat"] {
                let candidate = directory.join(name);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    "codex".into()
}

/// Reads one output pipe to EOF on its own thread, lossily decoded.
fn drain_pipe<R: std::io::Read + Send + 'static>(mut pipe: R) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = std::io::Read::read_to_end(&mut pipe, &mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

/// A unique empty directory so the probe run can never land in a repository.
fn fresh_workdir() -> std::io::Result<PathBuf> {
    let directory =
        std::env::temp_dir().join(format!("asb-codex-probe-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory)?;
    Ok(directory)
}

fn snapshot_session_files() -> BTreeSet<PathBuf> {
    let mut files = BTreeSet::new();
    let Ok(root) = crate::session_manager::codex_root() else {
        return files;
    };
    for session_root in crate::local_state::codex_paths::session_roots(&root) {
        if let Ok(paths) = crate::session_manager::collect_session_jsonl_files(&session_root) {
            files.extend(paths);
        }
    }
    files
}

fn wait_for_new_session_summary(before: &BTreeSet<PathBuf>) -> Option<SessionTokenSummary> {
    for attempt in 0..SESSION_FILE_RETRIES {
        if let Some(path) = newest_new_session_file(before) {
            let file = std::fs::File::open(&path).ok()?;
            return summarize_token_usage(BufReader::new(file)).ok();
        }
        if attempt + 1 < SESSION_FILE_RETRIES {
            std::thread::sleep(SESSION_FILE_RETRY_DELAY);
        }
    }
    None
}

fn newest_new_session_file(before: &BTreeSet<PathBuf>) -> Option<PathBuf> {
    let root = crate::session_manager::codex_root().ok()?;
    let mut newest: Option<(std::time::SystemTime, PathBuf)> = None;
    for session_root in crate::local_state::codex_paths::session_roots(&root) {
        let Ok(paths) = crate::session_manager::collect_session_jsonl_files(&session_root) else {
            continue;
        };
        for path in paths {
            if before.contains(&path) {
                continue;
            }
            let Ok(metadata) = std::fs::metadata(&path) else {
                continue;
            };
            let Ok(modified) = metadata.modified() else {
                continue;
            };
            let better = match &newest {
                Some((best_modified, _)) => modified > *best_modified,
                None => true,
            };
            if better {
                newest = Some((modified, path));
            }
        }
    }
    newest.map(|(_, path)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standalone_answers_match_and_embedded_digits_do_not() {
        assert!(answer_present("答案是 45。", "45"));
        assert!(answer_present("The answer is 45.", "45"));
        assert!(answer_present("结果为45人", "45"));
        assert!(answer_present("12 34 45", "45"));
        // The number must stand alone on both sides.
        assert!(!answer_present("145", "45"));
        assert!(!answer_present("451", "45"));
        assert!(!answer_present("1,450", "45"));
        assert!(!answer_present("4.5", "45"));
        assert!(!answer_present("没有数字", "45"));
        // Only well-formed expectations are ever matched.
        assert!(!answer_present("45", ""));
        assert!(!answer_present("45", "4 5"));
        assert!(!answer_present("45", "-45"));
    }

    #[test]
    fn builtin_questions_have_unique_ids_and_pure_digit_answers() {
        let mut ids = Vec::new();
        for question in QUESTIONS.iter() {
            assert!(!question.id.is_empty());
            assert!(!question.label.is_empty());
            assert!(!question.prompt.is_empty());
            assert!(question
                .expected_answer
                .bytes()
                .all(|byte| byte.is_ascii_digit()));
            ids.push(question.id);
        }
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), QUESTIONS.len());
    }

    #[test]
    fn resolve_question_validates_builtin_and_custom_selections() {
        let builtin = resolve_question(QUESTIONS[0].id, None, None).expect("builtin question");
        assert_eq!(builtin.label, QUESTIONS[0].label);
        assert_eq!(builtin.expected_answer, QUESTIONS[0].expected_answer);

        let custom = resolve_question(
            CUSTOM_QUESTION_ID,
            Some("  题目文本 ".to_string()),
            Some(" 21 ".to_string()),
        )
        .expect("custom question");
        assert_eq!(custom.prompt, "题目文本");
        assert_eq!(custom.expected_answer, "21");

        assert!(resolve_question("unknown", None, None).is_err());
        assert!(resolve_question(CUSTOM_QUESTION_ID, None, None).is_err());
        assert!(resolve_question(CUSTOM_QUESTION_ID, Some("题目".into()), None).is_err());
        assert!(resolve_question(CUSTOM_QUESTION_ID, Some("题目".into()), Some("x".into())).is_err());
    }

    #[test]
    fn registry_allows_one_running_probe_and_keeps_the_finished_status() {
        let registry = ProbeRegistry::new();
        let question = resolve_question(QUESTIONS[0].id, None, None).expect("builtin question");
        let (id, _cancel) = registry.begin(&question, 2).expect("first begin");
        assert!(registry.begin(&question, 1).is_err());

        registry.push_run(
            &id,
            ProbeRunResult {
                passed: true,
                reasoning_tokens: Some(7),
                total_tokens: Some(21),
                model: Some("gpt-test".to_string()),
                duration_ms: 5,
                error: None,
            },
        );
        let status = registry.status(&id).expect("running status");
        assert_eq!(status.phase, ProbePhase::Running);
        assert_eq!(status.completed_runs, 1);

        registry.finish(&id, ProbePhase::Completed, None);
        // A finished probe no longer accepts cancels and a new one may start.
        assert!(!registry.cancel(&id));
        let status = registry.status(&id).expect("finished status");
        assert_eq!(status.phase, ProbePhase::Completed);
        assert_eq!(status.run_count, 2);
        assert_eq!(status.completed_runs, 1);
        assert!(registry.begin(&question, 1).is_ok());
        assert!(registry.status(&id).is_none());
    }

    /// Writes a stub Codex launcher that succeeds with the expected answer and
    /// leaves one rollout file under the redirected `CODEX_HOME`.
    fn write_answering_stub(directory: &std::path::Path) -> std::path::PathBuf {
        let rollout_lines = [
            r#"{"type":"session_meta","timestamp":"2026-01-01T00:00:00.000Z","payload":{"id":"11111111-1111-1111-1111-111111111111"}}"#,
            r#"{"type":"event_msg","timestamp":"2026-01-01T00:00:02.000Z","payload":{"type":"token_count","info":{"model":"gpt-test","total_token_usage":{"input_tokens":120,"cached_input_tokens":30,"output_tokens":40,"reasoning_output_tokens":77,"total_tokens":190},"last_token_usage":{"input_tokens":120,"cached_input_tokens":30,"output_tokens":40}}}}"#,
        ];
        #[cfg(windows)]
        let (path, script) = {
            let path = directory.join("codex-stub.cmd");
            let mut script = String::from("@echo off\r\n");
            script.push_str("if not exist \"%CODEX_HOME%\\sessions\\t\" mkdir \"%CODEX_HOME%\\sessions\\t\"\r\n");
            script.push_str(">%CODEX_HOME%\\sessions\\t\\rollout-11111111-1111-1111-1111-111111111111.jsonl (\r\n");
            for line in rollout_lines {
                script.push_str(&format!("echo {line}\r\n"));
            }
            script.push_str(")\r\n");
            script.push_str("echo The answer is 45.\r\n");
            script.push_str("exit /b 0\r\n");
            (path, script)
        };
        #[cfg(not(windows))]
        let (path, script) = {
            let path = directory.join("codex-stub.sh");
            let mut script = String::from("#!/bin/sh\n");
            script.push_str("mkdir -p \"$CODEX_HOME/sessions/t\"\n");
            script.push_str("cat > \"$CODEX_HOME/sessions/t/rollout-11111111-1111-1111-1111-111111111111.jsonl\" <<'ROLLOUT'\n");
            for line in rollout_lines {
                script.push_str(&format!("{line}\n"));
            }
            script.push_str("ROLLOUT\n");
            script.push_str("echo The answer is 45.\n");
            (path, script)
        };
        std::fs::write(&path, script).expect("write stub launcher");
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make stub executable");
        }
        path
    }

    #[test]
    fn one_run_passes_when_the_answer_and_rollout_appear() {
        let _guard = crate::test_client_paths::redirect_client_paths();
        let home = std::path::PathBuf::from(
            std::env::var_os("CODEX_HOME").expect("redirected CODEX_HOME"),
        );
        let stub = write_answering_stub(&home);
        let previous = std::env::var_os("ASB_TEST_CODEX_BIN");
        std::env::set_var("ASB_TEST_CODEX_BIN", &stub);
        let question = resolve_question(QUESTIONS[0].id, None, None).expect("builtin question");
        let cancel = AtomicBool::new(false);
        let result = run_once(&question, &cancel).expect("stubbed run");
        match previous {
            Some(value) => std::env::set_var("ASB_TEST_CODEX_BIN", value),
            None => std::env::remove_var("ASB_TEST_CODEX_BIN"),
        }
        assert!(result.passed);
        assert_eq!(result.reasoning_tokens, Some(77));
        assert_eq!(result.total_tokens, Some(190));
        assert_eq!(result.model.as_deref(), Some("gpt-test"));
        assert!(result.error.is_none());
    }

    #[test]
    fn one_run_returns_control_immediately_when_cancelled() {
        let _guard = crate::test_client_paths::redirect_client_paths();
        let home = std::path::PathBuf::from(
            std::env::var_os("CODEX_HOME").expect("redirected CODEX_HOME"),
        );
        #[cfg(windows)]
        let (path, script) = (
            home.join("codex-sleep.cmd"),
            "@echo off\r\nping -n 3 127.0.0.1 > nul\r\nexit /b 0\r\n".to_string(),
        );
        #[cfg(not(windows))]
        let (path, script) = (
            home.join("codex-sleep.sh"),
            "#!/bin/sh\nsleep 2\n".to_string(),
        );
        std::fs::write(&path, script).expect("write sleep stub");
        #[cfg(not(windows))]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("make sleep stub executable");
        }
        let previous = std::env::var_os("ASB_TEST_CODEX_BIN");
        std::env::set_var("ASB_TEST_CODEX_BIN", &path);
        let question = resolve_question(QUESTIONS[0].id, None, None).expect("builtin question");
        let cancel = AtomicBool::new(true);
        let aborted = run_once(&question, &cancel);
        match previous {
            Some(value) => std::env::set_var("ASB_TEST_CODEX_BIN", value),
            None => std::env::remove_var("ASB_TEST_CODEX_BIN"),
        }
        assert!(matches!(aborted, Err(ProbeAbort::Cancelled)));
    }
}
