/** 官网可见文案与链接的唯一来源；界面截图来自隔离沙箱中的实际应用。 */
export type Locale = "zh-CN" | "en";

export interface SiteActionLink {
  label: string;
  href: string;
  variant: "primary" | "secondary";
  external: boolean;
  icon?: "github" | "star";
}

/** One real application screenshot captured in the isolated demo sandbox. */
export interface SiteScreenshot {
  src: string;
  alt: string;
  badge: string;
  caption: string;
  /** Intrinsic pixel size of the PNG; keeps layout stable before load. */
  width: number;
  height: number;
}

/** One showcase section: copy on one side, a sandbox screenshot on the other. */
export interface SiteShowcase {
  title: string;
  description: string;
  bullets: string[];
  shot: SiteScreenshot;
}

/** One capability panel (text-only; never a software-UI replica). */
export interface SiteCapability {
  title: string;
  description: string;
  bullets: string[];
  /** Optional real sandbox screenshot shown beneath the copy. */
  shot?: SiteScreenshot;
}

export interface SiteContent {
  brand: string;
  repoUrl: string;
  releasesUrl: string;
  document: { title: string; description: string };
  header: {
    navigationLabel: string;
    localeMenuLabel: string;
    themeToDarkLabel: string;
    themeToLightLabel: string;
    githubLabel: string;
    locales: Array<{ id: Locale; short: string; label: string }>;
  };
  nav: Array<{ label: string; href: string; external: boolean }>;
  hero: {
    title: string;
    fact: string;
    actions: SiteActionLink[];
    shot: SiteScreenshot;
  };
  preview: SiteShowcase;
  configuration: SiteShowcase;
  capabilities: {
    title: string;
    description: string;
    gateway: SiteCapability;
    subagent: SiteCapability;
  };
  writeLifecycle: {
    title: string;
    steps: string[];
    recovery: string;
  };
  final: { title: string; actions: SiteActionLink[] };
  footer: { note: string };
}

const repoUrl = "https://github.com/y4Nkk/agent-switchboard";
const releasesUrl = "https://github.com/y4Nkk/agent-switchboard/releases/latest";

export const siteContentByLocale: Record<Locale, SiteContent> = {
  "zh-CN": {
    brand: "Agent Switchboard",
    repoUrl,
    releasesUrl,
    document: {
      title: "Agent Switchboard — 拼好配置，再写入真实文件",
      description: "面向 Codex 与 Claude Code 的本地配置控制台。",
    },
    header: {
      navigationLabel: "页面导航",
      localeMenuLabel: "选择语言",
      themeToDarkLabel: "切换到深色外观",
      themeToLightLabel: "切换到浅色外观",
      githubLabel: "在 GitHub 查看 Agent Switchboard",
      locales: [
        { id: "zh-CN", short: "CN", label: "中文" },
        { id: "en", short: "US", label: "English" },
      ],
    },
    nav: [
      { label: "切换预览", href: "#preview", external: false },
      { label: "客户端配置", href: "#configuration", external: false },
      { label: "网关与子代理", href: "#capabilities", external: false },
      { label: "安全写入", href: "#safety", external: false },
    ],
    hero: {
      title: "拼好配置，再写入真实文件。",
      fact: "面向 Codex 与 Claude Code 的本地配置控制台。",
      actions: [
        { label: "GitHub", href: repoUrl, variant: "secondary", external: true, icon: "github" },
        { label: "下载版本", href: releasesUrl, variant: "primary", external: true },
      ],
      shot: {
        src: "/media/screenshots/providers.png",
        alt: "Agent Switchboard 供应商工作区实际界面：Codex 与 Claude Code 的档案列表、已应用状态与启用操作（隔离沙箱演示数据）",
        badge: "隔离沙箱 · 演示数据",
        caption: "供应商工作区：档案、启用状态与切换入口。",
        width: 1440,
        height: 1080,
      },
    },
    preview: {
      title: "写入前，先看变化。",
      description:
        "类型化差异预览列出每一个将写入的键，敏感值自动脱敏；只有点击确认，切换执行器才会触碰真实文件。",
      bullets: [
        "逐键差异：旧值 → 新值，未受管字段保持原样",
        "密钥与令牌自动脱敏，预览不泄露凭据",
        "确认后才写入，全程备份、校验、可恢复",
      ],
      shot: {
        src: "/media/screenshots/switch-preview.png",
        alt: "Agent Switchboard 切换确认界面实际截图：备用档案的脱敏配置差异、候选文件与确认操作（隔离沙箱演示数据）",
        badge: "隔离沙箱 · 演示数据",
        caption: "切换确认：对备用档案生成的真实类型化预览。",
        width: 1440,
        height: 1080,
      },
    },
    configuration: {
      title: "客户端配置，集中受控。",
      description:
        "两端通用配置、子 agent 运行设置与全局指令在同一个受控界面管理；界面未拥有的字段不会被覆盖。",
      bullets: [
        "通用设置与运行参数由界面状态归一化写入",
        "配置草稿可读取脱敏后的本机真实配置",
        "格式错误只提供可证明安全的修复候选",
      ],
      shot: {
        src: "/media/screenshots/client-configuration.png",
        alt: "Agent Switchboard 客户端配置工作区实际界面：通用设置分组与受控配置操作（隔离沙箱演示数据）",
        badge: "隔离沙箱 · 演示数据",
        caption: "客户端配置：分组设置与受控操作。",
        width: 1410,
        height: 985,
      },
    },
    capabilities: {
      title: "网关与子代理，边界清晰。",
      description:
        "协议转换只发生在本机回环；跨档案的子代理路由也经同一网关，凭据与计量不越过这条边界。",
      gateway: {
        title: "本机网关，只在需要时启动。",
        description:
          "第三方上游协议与 Codex / Claude Code 协议不一致时，应用只在 127.0.0.1 启动本地转换网关；跨供应商的子代理请求也必须经它路由。",
        bullets: [
          "仅在协议不一致时启动，平时不驻留",
          "只监听 127.0.0.1 本机回环，不提供公开代理",
          "无遥测、无云中转，转换全部在本机完成",
        ],
      },
      subagent: {
        title: "跨档案子代理模型。",
        description:
          "Codex 档案可把默认子 agent 模型指向另一个供应商档案的模型；高级开关默认收起，展开后才露出跨档案选择器。Claude Code 侧保持不变。",
        bullets: [
          "写入 config.toml 的永远是路由引用 asb:<档案>/<模型>",
          "跨供应商请求必须经本机网关；Direct 直连档案直接拒绝",
          "目标档案凭据按请求替换，计量按实际目标归因",
          "不参与跨供应商故障转移：目标档案失败即失败",
        ],
        shot: {
          src: "/media/screenshots/subagent-route.png",
          alt: "Agent Switchboard 的 Codex 运行参数实际界面：默认子 agent 模型以高级开关后的跨档案路由指向备用档案的模型（隔离沙箱演示数据）",
          badge: "隔离沙箱 · 演示数据",
          caption: "子 agent 模型路由：请求由本机网关转发，失败不回退主模型。",
        width: 1440,
        height: 1080,
        },
      },
    },
    writeLifecycle: {
      title: "确认后，按事务写入。",
      steps: ["创建备份", "临时写入", "临时回读", "语法校验", "最终冲突检查", "原子替换", "写后验证"],
      recovery: "写后验证失败时，恢复刚创建的备份",
    },
    final: {
      title: "从 GitHub 开始使用",
      actions: [
        { label: "查看源码", href: repoUrl, variant: "secondary", external: true, icon: "github" },
        { label: "点亮 Star", href: repoUrl, variant: "primary", external: true, icon: "star" },
      ],
    },
    footer: {
      note: "以 MIT License 开源。Codex 与 Claude Code 是其各自权利人的商标；Agent Switchboard 与 OpenAI、Anthropic 无隶属或认可关系。",
    },
  },
  en: {
    brand: "Agent Switchboard",
    repoUrl,
    releasesUrl,
    document: {
      title: "Agent Switchboard — Compose settings. Write real files.",
      description: "A local configuration console for Codex and Claude Code.",
    },
    header: {
      navigationLabel: "Page navigation",
      localeMenuLabel: "Choose language",
      themeToDarkLabel: "Use dark appearance",
      themeToLightLabel: "Use light appearance",
      githubLabel: "View Agent Switchboard on GitHub",
      locales: [
        { id: "zh-CN", short: "CN", label: "中文" },
        { id: "en", short: "US", label: "English" },
      ],
    },
    nav: [
      { label: "Preview", href: "#preview", external: false },
      { label: "Client settings", href: "#configuration", external: false },
      { label: "Gateway & sub-agents", href: "#capabilities", external: false },
      { label: "Safe writes", href: "#safety", external: false },
    ],
    hero: {
      title: "Compose settings. Write real files.",
      fact: "A local configuration console for Codex and Claude Code.",
      actions: [
        { label: "GitHub", href: repoUrl, variant: "secondary", external: true, icon: "github" },
        { label: "Download", href: releasesUrl, variant: "primary", external: true },
      ],
      shot: {
        src: "/media/screenshots/providers.png",
        alt: "Agent Switchboard providers workspace: profile lists for Codex and Claude Code with applied state and activate actions (isolated sandbox demo data)",
        badge: "Isolated sandbox · demo data",
        caption: "Providers workspace: profiles, applied state, and switching.",
        width: 1440,
        height: 1080,
      },
    },
    preview: {
      title: "Inspect changes before writing.",
      description:
        "A typed diff preview lists every key about to be written, with sensitive values redacted; the switch executor touches real files only after you confirm.",
      bullets: [
        "Key-by-key diff — old → new, unmanaged fields untouched",
        "Keys and tokens redacted, so previews never leak credentials",
        "Writes only after confirmation — backed up, validated, recoverable",
      ],
      shot: {
        src: "/media/screenshots/switch-preview.png",
        alt: "Agent Switchboard switch confirmation dialog: redacted configuration diff for a backup profile, candidate file, and confirm actions (isolated sandbox demo data)",
        badge: "Isolated sandbox · demo data",
        caption: "Switch confirmation: a real typed preview of the backup profile.",
        width: 1440,
        height: 1080,
      },
    },
    configuration: {
      title: "Client settings, under control.",
      description:
        "Shared settings, sub-agent runtime options, and global instructions live in one controlled workspace; fields the UI does not own are never overwritten.",
      bullets: [
        "Shared settings and runtime parameters are normalized on write",
        "Drafts can read the redacted real local configuration",
        "Only provably safe repair candidates for broken files",
      ],
      shot: {
        src: "/media/screenshots/client-configuration.png",
        alt: "Agent Switchboard client configuration workspace: grouped common settings and controlled configuration actions (isolated sandbox demo data)",
        badge: "Isolated sandbox · demo data",
        caption: "Client configuration: grouped settings, controlled actions.",
        width: 1410,
        height: 985,
      },
    },
    capabilities: {
      title: "Gateway and sub-agents, clear boundaries.",
      description:
        "Protocol conversion happens on the loopback only; cross-profile sub-agent routing goes through the same gateway, with credentials and metering kept inside that boundary.",
      gateway: {
        title: "A local gateway, started only when needed.",
        description:
          "When a third-party upstream speaks a different protocol than Codex or Claude Code, the app starts a local conversion gateway on 127.0.0.1 only; cross-provider sub-agent requests must route through it too.",
        bullets: [
          "Started only on protocol mismatch — nothing resident otherwise",
          "Loopback-only listener on 127.0.0.1, never a public proxy",
          "No telemetry, no cloud relay — conversion stays on this machine",
        ],
      },
      subagent: {
        title: "Cross-profile sub-agent models.",
        description:
          "A Codex profile can point its default sub-agent model at another provider profile's model; an advanced toggle — collapsed by default — reveals the cross-profile picker. The Claude Code side stays unchanged.",
        bullets: [
          "config.toml always receives the route reference asb:<profile>/<model>",
          "Cross-provider requests go through the local gateway; Direct profiles are rejected",
          "Target-profile credentials swapped per request; usage metered to the actual target",
          "No cross-provider failover: target-profile failure is failure",
        ],
        shot: {
          src: "/media/screenshots/subagent-route.png",
          alt: "Agent Switchboard Codex run parameters: the default sub-agent model configured as a cross-provider route to the backup profile's model behind the advanced toggle (isolated sandbox demo data)",
          badge: "Isolated sandbox · demo data",
          caption: "Sub-agent model route: forwarded by the local gateway, never falling back to the main model.",
        width: 1440,
        height: 1080,
        },
      },
    },
    writeLifecycle: {
      title: "After confirmation, the transaction writes.",
      steps: [
        "Create backup",
        "Write temporary file",
        "Verify temporary file",
        "Validate syntax",
        "Final conflict check",
        "Atomic replace",
        "Verify write",
      ],
      recovery: "If post-write verification fails, restore the just-created backup",
    },
    final: {
      title: "Start from GitHub",
      actions: [
        { label: "View source", href: repoUrl, variant: "secondary", external: true, icon: "github" },
        { label: "Star on GitHub", href: repoUrl, variant: "primary", external: true, icon: "star" },
      ],
    },
    footer: {
      note: "Released under the MIT License. Codex and Claude Code are trademarks of their respective owners; Agent Switchboard is not affiliated with or endorsed by OpenAI or Anthropic.",
    },
  },
};

export function actionProps(action: { href: string; external: boolean }) {
  return action.external
    ? { href: action.href, target: "_blank", rel: "noreferrer" }
    : { href: action.href };
}
