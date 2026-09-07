use crate::ownership::spec::{OfficialSettingDisposition, OfficialSettingEntry};

/// Official user-level configuration families that require a dedicated
/// contract, or are intentionally preserved because their scope is not the
/// user's global preferences. The parameter form receives only `Direct`
/// entries derived from `setting_specs`; this table prevents the rest of the
/// official surface from silently becoming an accidental "unknown key".
pub(super) const CODEX_DIRECTORY_FAMILIES: &[OfficialSettingEntry] = &[
    OfficialSettingEntry {
        title: "全局指令",
        path: "$CODEX_HOME/AGENTS.md",
        related_paths: &[],
        disposition: OfficialSettingDisposition::SeparateModule,
        detail: "在“官方设置目录”中通过独立文档事务管理。",
    },
    OfficialSettingEntry {
        title: "自定义模型提供商",
        path: "model_providers.<id>",
        related_paths: &[],
        disposition: OfficialSettingDisposition::SeparateModule,
        detail: "由供应商档案与切换事务拥有，不能与通用参数重复写入。",
    },
    OfficialSettingEntry {
        title: "MCP 服务器",
        path: "mcp_servers.<id>",
        related_paths: &[],
        disposition: OfficialSettingDisposition::SeparateModule,
        detail: "由扩展工作区以独立契约和事务管理；供应商切换完整保留这些表。",
    },
    OfficialSettingEntry {
        title: "Skill 启停规则",
        path: "skills.config",
        related_paths: &[],
        disposition: OfficialSettingDisposition::SeparateModule,
        detail: "由扩展工作区按 canonical document path 管理启停；Skills 文件本身在应用库与客户端目录之间部署副本。",
    },
    OfficialSettingEntry {
        title: "Hooks 与插件",
        path: "hooks",
        related_paths: &["plugins.<id>", "apps.<id>"],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "命令、路径与事件规则需独立契约；当前保留但不写入。",
    },
    OfficialSettingEntry {
        title: "权限配置与高级沙箱",
        path: "permissions.<name>",
        related_paths: &["sandbox_workspace_write", "default_permissions"],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "权限配置档、可写路径与网络规则不能压扁为普通开关；当前保留但不写入。",
    },
    OfficialSettingEntry {
        title: "Shell 环境与网络代理",
        path: "shell_environment_policy",
        related_paths: &["features.network_proxy"],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "环境过滤、注入和域名规则具有结构与凭据边界；当前保留但不写入。",
    },
    OfficialSettingEntry {
        title: "TUI 键位与布局",
        path: "tui.keymap",
        related_paths: &["tui.status_line", "tui.terminal_title"],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "有序布局和按上下文键位映射需要专用编辑器；当前保留但不写入。",
    },
    OfficialSettingEntry {
        title: "运行限额与终端超时",
        path: "model_auto_compact_token_limit",
        related_paths: &[
            "model_auto_compact_token_limit_scope",
            "history.max_bytes",
            "tool_output_token_limit",
            "background_terminal_max_timeout",
            "agents.max_concurrent_agents",
            "memories.*_limit",
        ],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "数值限额与保留策略需在同一资源预算契约中校验；当前保留但不写入。",
    },
    OfficialSettingEntry {
        title: "通知规则与终端状态",
        path: "tui.notification_method",
        related_paths: &["tui.notification_conditions", "tui.status_line"],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "通知方法和条件为关联结构；普通偏好之外的部分当前保留但不写入。",
    },
    OfficialSettingEntry {
        title: "项目配置与信任状态",
        path: ".codex/config.toml",
        related_paths: &["projects.<path>.trust_level"],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "属于具体项目，不作为跨项目用户基座写入。",
    },
    OfficialSettingEntry {
        title: "受管要求、登录与运行状态",
        path: "requirements.toml",
        related_paths: &["auth.json", "SQLite / 会话状态"],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail:
            "属于组织策略与运行态；通用设置只观测不写入。专用切换事务可更新 auth.json 的 Codex API-key 登录字段，官方登录流程才写 OAuth 凭据；两条路径均不触及 SQLite / 会话状态。",
    },
];

pub(super) const CLAUDE_DIRECTORY_FAMILIES: &[OfficialSettingEntry] = &[
    OfficialSettingEntry {
        title: "全局指令",
        path: "~/.claude/CLAUDE.md",
        related_paths: &[],
        disposition: OfficialSettingDisposition::SeparateModule,
        detail: "在“官方设置目录”中通过独立文档事务管理。",
    },
    OfficialSettingEntry {
        title: "权限规则",
        path: "permissions",
        related_paths: &[],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "allow / ask / deny 规则具有合并与优先级语义；当前保留但不写入。",
    },
    OfficialSettingEntry {
        title: "Hooks",
        path: "hooks",
        related_paths: &[],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "事件、匹配器和命令构成规则集合；当前保留但不写入。",
    },
    OfficialSettingEntry {
        title: "环境变量",
        path: "env",
        related_paths: &[],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail:
            "供应商拥有的 ANTHROPIC 路由字段与其他环境变量不能混为一个表单；当前保留未声明字段。",
    },
    OfficialSettingEntry {
        title: "模型映射与选择器",
        path: "modelOverrides",
        related_paths: &["modelSettings", "modelPicker", "fallbackModel"],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "模型映射和回退链是有序结构，不能与供应商主模型形成双重所有权。",
    },
    OfficialSettingEntry {
        title: "按模型推理强度",
        path: "modelSettings.<model>.effortLevel",
        related_paths: &["modelSettings.<model>.fastMode"],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "Claude 按模型保存的推理偏好和快速模式属于模型映射；全局 effortLevel 与 Ultracode 由基础参数直接管理。",
    },
    OfficialSettingEntry {
        title: "自定义输出风格",
        path: "outputStyles.<name>",
        related_paths: &["outputStyle"],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "内置输出风格可直接选择；自定义风格是命名文档资源，当前保留但不写入。",
    },
    OfficialSettingEntry {
        title: "MCP 服务器",
        path: "~/.claude.json",
        related_paths: &[
            ".mcp.json",
            "projects.<path>.mcpServers",
            "projects.<path>.disabledMcpServers",
        ],
        disposition: OfficialSettingDisposition::SeparateModule,
        detail: "由扩展工作区以独立契约和事务管理；用户级停用撤下受管条目，项目级停用管理 disabledMcpServers 成员。",
    },
    OfficialSettingEntry {
        title: "Skills 与可见性",
        path: "~/.claude/skills/<name>/",
        related_paths: &["skillOverrides.<name>", ".claude/skills/<name>/"],
        disposition: OfficialSettingDisposition::SeparateModule,
        detail: "由扩展工作区部署完整副本并按名字管理可见性；不执行也不改写第三方 SKILL.md。",
    },
    OfficialSettingEntry {
        title: "插件与市场",
        path: "enabledPlugins",
        related_paths: &["extraKnownMarketplaces"],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "插件缓存与市场注册属于客户端；当前保留但不写入。",
    },
    OfficialSettingEntry {
        title: "语音与外部凭据脚本",
        path: "voice",
        related_paths: &["apiKeyHelper", "otelHeadersHelper"],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "语音对象和会执行外部命令的凭据/遥测脚本需要独立契约；当前保留但不写入。",
    },
    OfficialSettingEntry {
        title: "项目、本地与受管配置",
        path: ".claude/settings.json",
        related_paths: &[".claude/settings.local.json", "managed-settings.json"],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail: "分别属于仓库、项目个人例外和组织策略，不作为全局设置写入。",
    },
    OfficialSettingEntry {
        title: "官方登录与运行状态",
        path: "~/.claude/.credentials.json",
        related_paths: &["~/.claude.json"],
        disposition: OfficialSettingDisposition::PreserveOnly,
        detail:
            "官方身份、项目信任和运行态不由应用复制或导入为供应商档案；.credentials.json 仅在用户发起官方登录或重新登录时由专用登录流程写入，其余时间只读观测。",
    },
];
