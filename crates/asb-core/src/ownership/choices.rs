use crate::ownership::spec::{ChoiceControl, ChoiceOption, ChoiceSpec, SettingOwner};
use crate::ownership::CODEX_WEB_SEARCH_KEY;

pub const CODEX_SUBAGENT_REASONING_EFFORT_KEY: &str = "agents.default_subagent_reasoning_effort";

/// The one real Codex reasoning-effort value domain. The same domain backs
/// every reasoning-effort setting and the catalog reasoning levels; values are
/// model-dependent upstream and are never inferred, only user-declared.
const CODEX_REASONING_EFFORT_OPTIONS: &[ChoiceOption] = &[
    ChoiceOption {
        value: "none",
        label: "ownership.option.reasoningEffort.none",
    },
    ChoiceOption {
        value: "minimal",
        label: "ownership.option.reasoningEffort.minimal",
    },
    ChoiceOption {
        value: "low",
        label: "ownership.option.reasoningEffort.low",
    },
    ChoiceOption {
        value: "medium",
        label: "ownership.option.reasoningEffort.medium",
    },
    ChoiceOption {
        value: "high",
        label: "ownership.option.reasoningEffort.high",
    },
    ChoiceOption {
        value: "xhigh",
        label: "ownership.option.reasoningEffort.xhigh",
    },
    ChoiceOption {
        value: "max",
        label: "ownership.option.reasoningEffort.max",
    },
    ChoiceOption {
        value: "ultra",
        label: "ownership.option.reasoningEffort.ultra",
    },
];

pub const CODEX_CHOICES: &[ChoiceSpec] = &[
    ChoiceSpec {
        key: "model_reasoning_effort",
        owner: SettingOwner::Provider,
        label: "ownership.label.model_reasoning_effort",
        group: "ownership.group.modelBehavior",
        control: ChoiceControl::Slider,
        options: CODEX_REASONING_EFFORT_OPTIONS,
    },
    ChoiceSpec {
        key: CODEX_SUBAGENT_REASONING_EFFORT_KEY,
        owner: SettingOwner::Provider,
        label: "ownership.label.agentsDefault_subagent_reasoning_effort",
        group: "ownership.group.subagent",
        control: ChoiceControl::Slider,
        options: CODEX_REASONING_EFFORT_OPTIONS,
    },
    ChoiceSpec {
        key: "plan_mode_reasoning_effort",
        owner: SettingOwner::Provider,
        label: "ownership.label.plan_mode_reasoning_effort",
        group: "ownership.group.modelBehavior",
        control: ChoiceControl::Slider,
        options: CODEX_REASONING_EFFORT_OPTIONS,
    },
    ChoiceSpec {
        key: "model_reasoning_summary",
        owner: SettingOwner::Provider,
        label: "ownership.label.model_reasoning_summary",
        group: "ownership.group.modelBehavior",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "auto",
                label: "ownership.option.model_reasoning_summary.auto",
            },
            ChoiceOption {
                value: "concise",
                label: "ownership.option.model_reasoning_summary.concise",
            },
            ChoiceOption {
                value: "detailed",
                label: "ownership.option.model_reasoning_summary.detailed",
            },
            ChoiceOption {
                value: "none",
                label: "ownership.option.model_reasoning_summary.none",
            },
        ],
    },
    ChoiceSpec {
        key: "model_verbosity",
        owner: SettingOwner::Provider,
        label: "ownership.label.model_verbosity",
        group: "ownership.group.modelBehavior",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "low",
                label: "ownership.option.model_verbosity.low",
            },
            ChoiceOption {
                value: "medium",
                label: "ownership.option.model_verbosity.medium",
            },
            ChoiceOption {
                value: "high",
                label: "ownership.option.model_verbosity.high",
            },
        ],
    },
    ChoiceSpec {
        key: "personality",
        owner: SettingOwner::Provider,
        label: "ownership.label.personality",
        group: "ownership.group.modelBehavior",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "none",
                label: "ownership.option.personality.none",
            },
            ChoiceOption {
                value: "friendly",
                label: "ownership.option.personality.friendly",
            },
            ChoiceOption {
                value: "pragmatic",
                label: "ownership.option.personality.pragmatic",
            },
        ],
    },
    ChoiceSpec {
        key: CODEX_WEB_SEARCH_KEY,
        owner: SettingOwner::Provider,
        label: "ownership.label.webSearch",
        group: "ownership.group.modelBehavior",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "disabled",
                label: "ownership.option.webSearch.disabled",
            },
            ChoiceOption {
                value: "cached",
                label: "ownership.option.webSearch.cached",
            },
            ChoiceOption {
                value: "indexed",
                label: "ownership.option.webSearch.indexed",
            },
            ChoiceOption {
                value: "live",
                label: "ownership.option.webSearch.live",
            },
        ],
    },
    ChoiceSpec {
        key: "sandbox_mode",
        owner: SettingOwner::Client,
        label: "ownership.label.sandbox_mode",
        group: "ownership.group.safetyAndApprovals",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "read-only",
                label: "ownership.option.sandbox_mode.read-only",
            },
            ChoiceOption {
                value: "workspace-write",
                label: "ownership.option.sandbox_mode.workspace-write",
            },
            ChoiceOption {
                value: "danger-full-access",
                label: "ownership.option.sandbox_mode.danger-full-access",
            },
        ],
    },
    ChoiceSpec {
        key: "approval_policy",
        owner: SettingOwner::Client,
        label: "ownership.label.approval_policy",
        group: "ownership.group.safetyAndApprovals",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "untrusted",
                label: "ownership.option.approval_policy.untrusted",
            },
            ChoiceOption {
                value: "on-request",
                label: "ownership.option.approval_policy.on-request",
            },
            ChoiceOption {
                value: "never",
                label: "ownership.option.approval_policy.never",
            },
        ],
    },
    ChoiceSpec {
        key: "approvals_reviewer",
        owner: SettingOwner::Client,
        label: "ownership.label.approvals_reviewer",
        group: "ownership.group.safetyAndApprovals",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "user",
                label: "ownership.option.approvals_reviewer.user",
            },
            ChoiceOption {
                value: "auto_review",
                label: "ownership.option.approvals_reviewer.auto_review",
            },
        ],
    },
    ChoiceSpec {
        key: "windows.sandbox",
        owner: SettingOwner::Client,
        label: "ownership.label.windowsSandbox",
        group: "ownership.group.safetyAndApprovals",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "unelevated",
                label: "ownership.option.windowsSandbox.unelevated",
            },
            ChoiceOption {
                value: "elevated",
                label: "ownership.option.windowsSandbox.elevated",
            },
        ],
    },
    ChoiceSpec {
        key: "history.persistence",
        owner: SettingOwner::Client,
        label: "ownership.label.historyPersistence",
        group: "ownership.group.privacyAndData",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "save-all",
                label: "ownership.option.historyPersistence.save-all",
            },
            ChoiceOption {
                value: "none",
                label: "ownership.option.historyPersistence.none",
            },
        ],
    },
    ChoiceSpec {
        key: "file_opener",
        owner: SettingOwner::Client,
        label: "ownership.label.file_opener",
        group: "ownership.group.toolsAndFeatures",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "vscode",
                label: "ownership.option.file_opener.vscode",
            },
            ChoiceOption {
                value: "vscode-insiders",
                label: "ownership.option.file_opener.vscode-insiders",
            },
            ChoiceOption {
                value: "windsurf",
                label: "ownership.option.file_opener.windsurf",
            },
            ChoiceOption {
                value: "cursor",
                label: "ownership.option.file_opener.cursor",
            },
            ChoiceOption {
                value: "none",
                label: "ownership.option.file_opener.none",
            },
        ],
    },
];
pub const CLAUDE_CHOICES: &[ChoiceSpec] = &[
    ChoiceSpec {
        key: "effortLevel",
        owner: SettingOwner::Provider,
        label: "ownership.label.effortLevel",
        group: "ownership.group.modelBehavior",
        control: ChoiceControl::Slider,
        options: &[
            ChoiceOption {
                value: "low",
                label: "ownership.option.effortLevel.low",
            },
            ChoiceOption {
                value: "medium",
                label: "ownership.option.effortLevel.medium",
            },
            ChoiceOption {
                value: "high",
                label: "ownership.option.effortLevel.high",
            },
            ChoiceOption {
                value: "xhigh",
                label: "ownership.option.effortLevel.xhigh",
            },
            ChoiceOption {
                value: "max",
                label: "ownership.option.effortLevel.max",
            },
        ],
    },
    ChoiceSpec {
        key: "outputStyle",
        owner: SettingOwner::Provider,
        label: "ownership.label.outputStyle",
        group: "ownership.group.modelBehavior",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "Proactive",
                label: "ownership.option.outputStyle.Proactive",
            },
            ChoiceOption {
                value: "Concise",
                label: "ownership.option.outputStyle.Concise",
            },
            ChoiceOption {
                value: "Explanatory",
                label: "ownership.option.outputStyle.Explanatory",
            },
            ChoiceOption {
                value: "Learning",
                label: "ownership.option.outputStyle.Learning",
            },
        ],
    },
    ChoiceSpec {
        key: "preferredNotifChannel",
        owner: SettingOwner::Client,
        label: "ownership.label.preferredNotifChannel",
        group: "ownership.group.interfaceAndInteraction",
        control: ChoiceControl::Segment,
        options: &[
            ChoiceOption {
                value: "auto",
                label: "ownership.option.preferredNotifChannel.auto",
            },
            ChoiceOption {
                value: "terminal_bell",
                label: "ownership.option.preferredNotifChannel.terminal_bell",
            },
            ChoiceOption {
                value: "iterm2",
                label: "ownership.option.preferredNotifChannel.iterm2",
            },
            ChoiceOption {
                value: "iterm2_with_bell",
                label: "ownership.option.preferredNotifChannel.iterm2_with_bell",
            },
            ChoiceOption {
                value: "kitty",
                label: "ownership.option.preferredNotifChannel.kitty",
            },
            ChoiceOption {
                value: "ghostty",
                label: "ownership.option.preferredNotifChannel.ghostty",
            },
            ChoiceOption {
                value: "notifications_disabled",
                label: "ownership.option.preferredNotifChannel.notifications_disabled",
            },
        ],
    },
];
