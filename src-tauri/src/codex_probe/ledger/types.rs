/// Terminal-vs-live batch state. `Interrupted` is only assigned by the
/// startup recovery pass; `ConfigChanged` only by the executor's
/// configuration guard.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProbeBatchStatus {
    Running,
    Completed,
    Cancelled,
    Failed,
    Interrupted,
    ConfigChanged,
}

impl ProbeBatchStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Cancelled => "cancelled",
            Self::Failed => "failed",
            Self::Interrupted => "interrupted",
            Self::ConfigChanged => "config-changed",
        }
    }
    pub(crate) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "running" => Self::Running,
            "completed" => Self::Completed,
            "cancelled" => Self::Cancelled,
            "failed" => Self::Failed,
            "interrupted" => Self::Interrupted,
            "config-changed" => Self::ConfigChanged,
            _ => return None,
        })
    }
    pub(crate) fn is_terminal(self) -> bool {
        self != Self::Running
    }
}

/// One finished run's grading outcome. Execution failures, cancellation, and
/// interruption never grade — they stay `Undetermined`, separate from a wrong
/// answer (`Failed`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProbeRunStatus {
    #[default]
    Running,
    Passed,
    Failed,
    Undetermined,
}

impl ProbeRunStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Undetermined => "undetermined",
        }
    }
    pub(super) fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "running" => Self::Running,
            "passed" => Self::Passed,
            "failed" => Self::Failed,
            "undetermined" => Self::Undetermined,
            _ => return None,
        })
    }
}

/// The question exactly as the batch ran it — a snapshot, not a reference
/// into the live catalog.
#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeQuestionSnapshot {
    pub id: String,
    pub label: String,
    pub text: String,
    pub expected_answer: String,
}

/// The Codex configuration the batch was started against. `profile_*` are
/// snapshots: later renames or deletions never rewrite history, and an
/// unidentified configuration stays `None` (shown as 未关联档案).
#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeConfigRecord {
    pub profile_id: Option<String>,
    pub profile_name: Option<String>,
    pub profile_model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub connection_identity: Option<String>,
    pub fingerprint: String,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeBatchRecord {
    #[serde(rename = "batchId")]
    pub id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: ProbeBatchStatus,
    pub planned_runs: u32,
    pub question: ProbeQuestionSnapshot,
    pub grading_version: String,
    pub cli_version: Option<String>,
    pub config: ProbeConfigRecord,
    pub status_error: Option<String>,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeRunRecord {
    pub seq: u32,
    pub status: ProbeRunStatus,
    /// Saved the moment the CLI reports it, before the run finishes.
    pub session_id: Option<String>,
    pub final_answer: Option<String>,
    pub reported_model: Option<String>,
    pub duration_ms: Option<u64>,
    /// Unknown usage stays `None`; a recorded real zero stays zero.
    pub reasoning_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub execution_error: Option<String>,
    pub usage_error: Option<String>,
}

/// Everything the executor needs to create a batch row before any call.
#[derive(Clone, Debug)]
pub(crate) struct NewProbeBatch {
    pub id: String,
    pub started_at: String,
    pub planned_runs: u32,
    pub question: ProbeQuestionSnapshot,
    pub grading_version: String,
    pub cli_version: Option<String>,
    pub config: ProbeConfigRecord,
}

/// History-list narrowing. `days: None` means all time.
#[derive(Clone, Debug)]
pub(crate) struct ProbeHistoryQuery {
    pub offset: u32,
    pub limit: u32,
    pub days: Option<u32>,
    pub profile: ProbeProfileFilter,
    pub status: Option<ProbeBatchStatus>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ProbeProfileFilter {
    All,
    Unlinked,
    Id(String),
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeHistoryItem {
    pub batch_id: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub status: ProbeBatchStatus,
    pub question_label: String,
    pub profile_id: Option<String>,
    pub profile_name: Option<String>,
    pub run_count: u32,
    pub passed_count: u32,
    pub judged_count: u32,
    pub recorded_runs: u32,
    pub total_tokens: Option<u64>,
}

#[derive(Clone, Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeHistoryPage {
    pub items: Vec<ProbeHistoryItem>,
    pub total: u32,
    pub offset: u32,
    pub limit: u32,
}

#[derive(Clone, Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProbeHistoryProfile {
    pub profile_id: String,
    pub profile_name: String,
}
