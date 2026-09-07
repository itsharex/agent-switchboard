use crate::ownership::spec::{ChoiceControl, ChoiceOption, ChoiceSpec};
use crate::ownership::CODEX_WEB_SEARCH_KEY;

pub const CODEX_CHOICES: &[ChoiceSpec] = &[
    ChoiceSpec {
        key: "model_reasoning_effort",
        label: "推理强度",
        group: "模型行为",
        control: ChoiceControl::Slider,
        options: &[
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
        ],
    },
    ChoiceSpec {
        key: "plan_mode_reasoning_effort",
        label: "计划模式推理强度",
        group: "模型行为",
        control: ChoiceControl::Slider,
        options: &[
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
        ],
    },
    ChoiceSpec {
        key: "model_reasoning_summary",
        label: "推理摘要",
        group: "模型行为",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "auto",
                label: "自动",
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
        label: "通知渠道",
        group: "界面与交互",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "auto",
                label: "自动",
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
