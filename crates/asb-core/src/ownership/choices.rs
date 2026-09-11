use crate::ownership::spec::{ChoiceControl, ChoiceOption, ChoiceSpec, SettingOwner};
use crate::ownership::CODEX_WEB_SEARCH_KEY;

pub const CODEX_SUBAGENT_REASONING_EFFORT_KEY: &str = "agents.default_subagent_reasoning_effort";

/// The one real Codex reasoning-effort value domain. The same domain backs
/// every reasoning-effort setting and the catalog reasoning levels; values are
/// model-dependent upstream and are never inferred, only user-declared.
const CODEX_REASONING_EFFORT_OPTIONS: &[ChoiceOption] = &[
    ChoiceOption {
        value: "none",
        label: "无",
    },
    ChoiceOption {
        value: "minimal",
        label: "极低",
    },
    ChoiceOption {
        value: "low",
        label: "低",
    },
    ChoiceOption {
        value: "medium",
        label: "中",
    },
    ChoiceOption {
        value: "high",
        label: "高",
    },
    ChoiceOption {
        value: "xhigh",
        label: "极高",
    },
    ChoiceOption {
        value: "max",
        label: "最高",
    },
    ChoiceOption {
        value: "ultra",
        label: "超极高",
    },
];

pub const CODEX_CHOICES: &[ChoiceSpec] = &[
    ChoiceSpec {
        key: "model_reasoning_effort",
        owner: SettingOwner::Provider,
        label: "推理强度",
        group: "模型行为",
        control: ChoiceControl::Slider,
        options: CODEX_REASONING_EFFORT_OPTIONS,
    },
    ChoiceSpec {
        key: CODEX_SUBAGENT_REASONING_EFFORT_KEY,
        owner: SettingOwner::Provider,
        label: "默认推理强度",
        group: "子 agent",
        control: ChoiceControl::Slider,
        options: CODEX_REASONING_EFFORT_OPTIONS,
    },
    ChoiceSpec {
        key: "plan_mode_reasoning_effort",
        owner: SettingOwner::Provider,
        label: "计划模式推理强度",
        group: "模型行为",
        control: ChoiceControl::Slider,
        options: CODEX_REASONING_EFFORT_OPTIONS,
    },
    ChoiceSpec {
        key: "model_reasoning_summary",
        owner: SettingOwner::Provider,
        label: "推理摘要",
        group: "模型行为",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "auto",
                label: "自动摘要",
            },
            ChoiceOption {
                value: "concise",
                label: "简要",
            },
            ChoiceOption {
                value: "detailed",
                label: "详细",
            },
            ChoiceOption {
                value: "none",
                label: "关闭",
            },
        ],
    },
    ChoiceSpec {
        key: "model_verbosity",
        owner: SettingOwner::Provider,
        label: "回复详细度",
        group: "模型行为",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "low",
                label: "简洁",
            },
            ChoiceOption {
                value: "medium",
                label: "标准",
            },
            ChoiceOption {
                value: "high",
                label: "详细",
            },
        ],
    },
    ChoiceSpec {
        key: "personality",
        owner: SettingOwner::Provider,
        label: "助手个性",
        group: "模型行为",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "none",
                label: "中性",
            },
            ChoiceOption {
                value: "friendly",
                label: "友好",
            },
            ChoiceOption {
                value: "pragmatic",
                label: "务实",
            },
        ],
    },
    ChoiceSpec {
        key: CODEX_WEB_SEARCH_KEY,
        owner: SettingOwner::Provider,
        label: "网页搜索",
        group: "模型行为",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "disabled",
                label: "禁用",
            },
            ChoiceOption {
                value: "cached",
                label: "仅缓存",
            },
            ChoiceOption {
                value: "indexed",
                label: "索引",
            },
            ChoiceOption {
                value: "live",
                label: "实时",
            },
        ],
    },
    ChoiceSpec {
        key: "sandbox_mode",
        owner: SettingOwner::Client,
        label: "沙箱模式",
        group: "安全与审批",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "read-only",
                label: "只读",
            },
            ChoiceOption {
                value: "workspace-write",
                label: "工作区可写",
            },
            ChoiceOption {
                value: "danger-full-access",
                label: "完全访问",
            },
        ],
    },
    ChoiceSpec {
        key: "approval_policy",
        owner: SettingOwner::Client,
        label: "批准策略",
        group: "安全与审批",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "untrusted",
                label: "仅信任白名单代码",
            },
            ChoiceOption {
                value: "on-request",
                label: "按请求",
            },
            ChoiceOption {
                value: "never",
                label: "从不",
            },
        ],
    },
    ChoiceSpec {
        key: "approvals_reviewer",
        owner: SettingOwner::Client,
        label: "审批复核方式",
        group: "安全与审批",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "user",
                label: "用户",
            },
            ChoiceOption {
                value: "auto_review",
                label: "自动复核",
            },
        ],
    },
    ChoiceSpec {
        key: "windows.sandbox",
        owner: SettingOwner::Client,
        label: "Windows 沙箱权限",
        group: "安全与审批",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "unelevated",
                label: "非提升",
            },
            ChoiceOption {
                value: "elevated",
                label: "提升权限",
            },
        ],
    },
    ChoiceSpec {
        key: "history.persistence",
        owner: SettingOwner::Client,
        label: "会话历史",
        group: "隐私与数据",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "save-all",
                label: "保存全部",
            },
            ChoiceOption {
                value: "none",
                label: "不保存",
            },
        ],
    },
    ChoiceSpec {
        key: "file_opener",
        owner: SettingOwner::Client,
        label: "文件打开方式",
        group: "工具与功能",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "vscode",
                label: "VS Code",
            },
            ChoiceOption {
                value: "vscode-insiders",
                label: "VS Code Insiders",
            },
            ChoiceOption {
                value: "windsurf",
                label: "Windsurf",
            },
            ChoiceOption {
                value: "cursor",
                label: "Cursor",
            },
            ChoiceOption {
                value: "none",
                label: "不打开",
            },
        ],
    },
];
pub const CLAUDE_CHOICES: &[ChoiceSpec] = &[
    ChoiceSpec {
        key: "effortLevel",
        owner: SettingOwner::Provider,
        label: "推理强度",
        group: "模型行为",
        control: ChoiceControl::Slider,
        options: &[
            ChoiceOption {
                value: "low",
                label: "低",
            },
            ChoiceOption {
                value: "medium",
                label: "中",
            },
            ChoiceOption {
                value: "high",
                label: "高",
            },
            ChoiceOption {
                value: "xhigh",
                label: "极高",
            },
        ],
    },
    ChoiceSpec {
        key: "outputStyle",
        owner: SettingOwner::Provider,
        label: "输出风格",
        group: "模型行为",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "Proactive",
                label: "主动",
            },
            ChoiceOption {
                value: "Concise",
                label: "简洁",
            },
            ChoiceOption {
                value: "Explanatory",
                label: "讲解",
            },
            ChoiceOption {
                value: "Learning",
                label: "学习",
            },
        ],
    },
    ChoiceSpec {
        key: "preferredNotifChannel",
        owner: SettingOwner::Client,
        label: "通知渠道",
        group: "界面与交互",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "auto",
                label: "自动选择渠道",
            },
            ChoiceOption {
                value: "terminal_bell",
                label: "终端铃声",
            },
            ChoiceOption {
                value: "iterm2",
                label: "iTerm2",
            },
            ChoiceOption {
                value: "iterm2_with_bell",
                label: "iTerm2 + 铃声",
            },
            ChoiceOption {
                value: "kitty",
                label: "Kitty",
            },
            ChoiceOption {
                value: "ghostty",
                label: "Ghostty",
            },
            ChoiceOption {
                value: "notifications_disabled",
                label: "关闭通知",
            },
        ],
    },
];
