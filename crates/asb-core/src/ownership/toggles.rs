use crate::ownership::spec::ToggleSpec;

/// The official general-config toggles offered on the settings page. Keys
/// must stay inside the owned-key tables above.
pub const CODEX_TOGGLES: &[ToggleSpec] = &[
    ToggleSpec {
        key: "hide_agent_reasoning",
        label: "在界面中隐藏推理摘要",
        group: "模型行为",
    },
    ToggleSpec {
        key: "show_raw_agent_reasoning",
        label: "显示模型的原始推理内容",
        group: "模型行为",
    },
    ToggleSpec {
        key: "tui.animations",
        label: "终端动画（欢迎页与加载动效）",
        group: "终端界面",
    },
    ToggleSpec {
        key: "tui.show_tooltips",
        label: "欢迎页功能引导提示",
        group: "终端界面",
    },
    ToggleSpec {
        key: "tui.notifications",
        label: "终端通知（回合结束时）",
        group: "终端界面",
    },
    ToggleSpec {
        key: "tui.raw_output_mode",
        label: "原始滚动模式（不切换交替屏幕）",
        group: "终端界面",
    },
    ToggleSpec {
        key: "tui.vim_mode_default",
        label: "默认启用 Vim 输入模式",
        group: "终端界面",
    },
    ToggleSpec {
        key: "disable_paste_burst",
        label: "关闭多行粘贴突发检测",
        group: "终端界面",
    },
    ToggleSpec {
        key: "tools.view_image",
        label: "启用本地图片查看工具",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "features.memories",
        label: "启用 Memories 跨会话记忆",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "features.prevent_idle_sleep",
        label: "会话运行期间阻止系统休眠",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "check_for_update_on_startup",
        label: "启动时检查更新",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "allow_login_shell",
        label: "允许登录 Shell 环境",
        group: "安全与审批",
    },
    ToggleSpec {
        key: "sandbox_workspace_write.network_access",
        label: "工作区沙箱允许网络访问",
        group: "安全与审批",
    },
    ToggleSpec {
        key: "sandbox_workspace_write.exclude_tmpdir_env_var",
        label: "工作区沙箱忽略 TMPDIR 环境变量",
        group: "安全与审批",
    },
    ToggleSpec {
        key: "sandbox_workspace_write.exclude_slash_tmp",
        label: "工作区沙箱不映射 /tmp",
        group: "安全与审批",
    },
    ToggleSpec {
        key: "windows.sandbox_private_desktop",
        label: "Windows 沙箱使用私有桌面",
        group: "安全与审批",
    },
    ToggleSpec {
        key: "feedback.enabled",
        label: "允许提交产品反馈",
        group: "隐私与数据",
    },
    ToggleSpec {
        key: "analytics.enabled",
        label: "允许使用分析数据",
        group: "隐私与数据",
    },
    ToggleSpec {
        key: "tui.alternate_screen",
        label: "使用终端交替屏幕",
        group: "终端界面",
    },
    ToggleSpec {
        key: "tui.resume_cwd",
        label: "恢复会话时沿用工作目录",
        group: "终端界面",
    },
    ToggleSpec {
        key: "features.apps",
        label: "启用 Apps",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "features.hooks",
        label: "启用 Hooks",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "features.shell_tool",
        label: "启用 Shell 工具",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "features.enable_request_compression",
        label: "启用请求压缩",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "features.skill_mcp_dependency_install",
        label: "允许 Skill 安装 MCP 依赖",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "features.fast_mode",
        label: "启用快速模式",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "features.shell_snapshot",
        label: "启用 Shell 快照",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "features.unified_exec",
        label: "启用统一执行器",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "features.multi_agent",
        label: "启用多智能体协作",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "features.goals",
        label: "启用目标管理",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "features.remote_plugin",
        label: "启用远程插件",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "features.personality",
        label: "启用助手个性设置",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "agents.enabled",
        label: "启用多智能体执行",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "agents.allow_interrupt",
        label: "允许中断子智能体",
        group: "工具与功能",
    },
    ToggleSpec {
        key: "memories.generate_memories",
        label: "自动生成记忆",
        group: "隐私与数据",
    },
    ToggleSpec {
        key: "memories.use_memories",
        label: "在会话中使用记忆",
        group: "隐私与数据",
    },
    ToggleSpec {
        key: "memories.disable_on_external_context",
        label: "外部上下文时禁用记忆",
        group: "隐私与数据",
    },
];

pub const CLAUDE_TOGGLES: &[ToggleSpec] = &[
    ToggleSpec {
        key: "alwaysThinkingEnabled",
        label: "默认开启扩展思考",
        group: "模型行为",
    },
    ToggleSpec {
        key: "autoCompactEnabled",
        label: "上下文自动压缩",
        group: "模型行为",
    },
    ToggleSpec {
        key: "showThinkingSummaries",
        label: "思考过程摘要",
        group: "模型行为",
    },
    ToggleSpec {
        key: "spinnerTipsEnabled",
        label: "加载动画提示语",
        group: "界面与交互",
    },
    ToggleSpec {
        key: "autoScrollEnabled",
        label: "输出自动滚动",
        group: "界面与交互",
    },
    ToggleSpec {
        key: "emojiCompletionEnabled",
        label: "输入框表情补全",
        group: "界面与交互",
    },
    ToggleSpec {
        key: "promptSuggestionEnabled",
        label: "提示词建议",
        group: "界面与交互",
    },
    ToggleSpec {
        key: "showTurnDuration",
        label: "显示每轮回复耗时",
        group: "界面与交互",
    },
    ToggleSpec {
        key: "syntaxHighlightingDisabled",
        label: "关闭输出语法高亮",
        group: "界面与交互",
    },
    ToggleSpec {
        key: "terminalProgressBarEnabled",
        label: "终端底部进度条",
        group: "界面与交互",
    },
    ToggleSpec {
        key: "fileCheckpointingEnabled",
        label: "文件检查点（对话内回滚）",
        group: "文件与 Git",
    },
    ToggleSpec {
        key: "respectGitignore",
        label: "文件选择遵守 .gitignore 规则",
        group: "文件与 Git",
    },
    ToggleSpec {
        key: "includeGitInstructions",
        label: "注入内置 Git 使用指南",
        group: "文件与 Git",
    },
    ToggleSpec {
        key: "autoMemoryEnabled",
        label: "自动记忆",
        group: "文件与 Git",
    },
    ToggleSpec {
        key: "ultracode",
        label: "Ultracode 动态工作流",
        group: "模型行为",
    },
];
