const MAX_CUSTOM_PROMPT_CHARS: usize = 4000;
pub(crate) const CUSTOM_QUESTION_ID: &str = "custom";
/// Version of the grading rule ("final answer must be the exact non-negative
/// integer"). Stored with every batch so history stays interpretable after
/// the rule changes.
pub(crate) const GRADING_VERSION: &str = "1";

/// One built-in probe question, graded against the final answer only.
pub(crate) struct ProbeQuestion {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(super) prompt: &'static str,
    pub(super) expected_answer: &'static str,
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

/// The question one probe runs, after request validation. The label, full
/// text, and expected answer are snapshotted into the batch record so later
/// catalog edits never rewrite history.
pub(crate) struct ResolvedQuestion {
    pub(crate) id: String,
    pub(crate) label: String,
    pub(super) prompt: String,
    pub(super) expected_answer: String,
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
            id: CUSTOM_QUESTION_ID.to_string(),
            label: "自定义题".to_string(),
            prompt,
            expected_answer: normalize_integer(&answer).to_string(),
        });
    }
    let question = QUESTIONS
        .iter()
        .find(|question| question.id == id)
        .ok_or_else(|| "未知的探针题目".to_string())?;
    Ok(ResolvedQuestion {
        id: question.id.to_string(),
        label: question.label.to_string(),
        prompt: question.prompt.to_string(),
        expected_answer: question.expected_answer.to_string(),
    })
}

pub(super) fn normalize_integer(value: &str) -> &str {
    let normalized = value.trim_start_matches('0');
    if normalized.is_empty() { "0" } else { normalized }
}

pub(super) fn answer_matches(answer: &str, expected: &str) -> bool {
    let answer = answer.trim();
    !answer.is_empty()
        && answer.bytes().all(|byte| byte.is_ascii_digit())
        && normalize_integer(answer) == expected
}
