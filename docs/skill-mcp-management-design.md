# Skill 与 MCP 管理方案

日期：2026-09-06。状态：目标契约与设计依据；实际代码完成范围、验证证据和未交付项以 [当前状态](skill-mcp-management-status.md) 与 [进度记录](../progress.md) 为准。适用产品：Agent Switchboard，仅 Codex 与 Claude Code。

本方案基于当前工作区代码、参考项目的固定提交和当天官方文档。产品范围以 [README](../README.md) 为准，视觉基线以 [DESIGN](../DESIGN.md) 为准，实施状态以 [progress](../progress.md) 为准；本文不把计划中的功能计为已完成。

## 1. 方案决策

增加一个顶层「扩展」工作区，提供 `Skills / MCP` 两个页签。用户从这里发现已有扩展、加入自己的库、选择客户端与作用域、查看实际文件变更，再确认应用。

核心模型是：**扩展定义 → 目标绑定 → 客户端文件投影 → 读取验证**。保存到库、部署到客户端、当前会话可用，是三个不同事实。

| 决策 | 本方案 |
| --- | --- |
| 支持对象 | 独立 Skill 目录、用户配置的 MCP 服务；插件内的资源可识别来源并只读展示 |
| 与供应商关系 | 扩展按客户端和作用域绑定；切换模型、供应商或协议不改变扩展 |
| 存储 | 扩展独立保存在应用数据 `state/extensions/`，不放入现有供应商快照 |
| Skills 分发 | 应用库保存不可变内容版本，客户端部署独立副本；首版不提供软链模式 |
| MCP 投影 | 共用带类型定义，Codex 与 Claude 分别渲染原生配置；不建立 MCP 转换代理 |
| 写入 | 沿用 `asb-switch` 作为真实配置唯一写入层，增加文件与目录事务能力 |
| 生效 | 保存、应用、原生客户端重载分别呈现，不以“写入成功”推断连接成功 |
| 自动化 | 本地发现不写配置；远端刷新、更新与连接检测由用户发起 |
| 本次交付 | 完整设计与落地顺序；代码交付范围由审查记录逐项确认 |

完整范围包括用户级管理和用户明确选择的项目级管理。先交付用户级闭环，再交付项目范围；后者有独立验收条件，不能把只有全局功能的版本称为完整方案完成。

## 2. 参考项目与实现证据

GitHub API 当天读取的 Star 数仅作为选型背景，不代表实现正确性或兼容保证。

| 项目 | Star 快照 | 固定提交 | 检查范围 |
| --- | ---: | --- | --- |
| 参考实现：资源库与多目标分发 | 131,253 | `db34612807244643d85ccedf9704c965facc4cba` | Skills 服务、MCP 服务与双客户端适配、Skill 同步测试及手册 |
| [Codex++](https://github.com/BigPizzaV3/CodexPlusPlus) | 30,274 | `48d43158688f5096c7059c690f8cd1daab340681` | Skills 文件管理、MCP 表单转换、README |
| [Vercel Skills](https://github.com/vercel-labs/skills) | 30,484 | `435076e78988e1e6ec40d00b0b1d76bdbbc5419a` | 安装器、客户端目录映射、来源解析、版本锁文件 |

### 参考实现：借鉴资源库与多目标分发

Skills 服务维护中心目录、客户端安装状态、内容摘要、更新检测和卸载备份；MCP 服务将统一定义按客户端投影。值得采用的是资源定义与目标启用状态分离，以及从本机导入的明确选择。对照源码为 `src-tauri/src/services/skill.rs` 与 `src-tauri/tests/skill_sync.rs`（基线 `db34612807244643d85ccedf9704c965facc4cba`）。

其 `McpService::upsert_server` 先保存数据库，再逐客户端同步；`toggle_app` 也先更新库中的应用状态，再写目标文件。本产品应将这些步骤纳入可恢复事务，否则第二个目标失败时，库记录可能先于真实配置变化。这里是对所检查调用顺序的判断，不推断参考项目所有路径都缺少恢复机制。对照源码为 `src-tauri/src/services/mcp.rs`（第 19 行起，基线 `db34612807244643d85ccedf9704c965facc4cba`）。

### Codex++：借鉴原生字段映射和可恢复卸载

`mcp_config.rs` 明确区分 Claude JSON 的 `headers` 与 Codex 的 `http_headers`，并处理原生 `enabled`；这说明跨客户端导入需要显式字段映射和逐项诊断。[MCP 源码](https://github.com/BigPizzaV3/CodexPlusPlus/blob/48d43158688f5096c7059c690f8cd1daab340681/crates/codex-plus-core/src/mcp_config.rs)。

其 Skills 管理采用中心目录、链接或复制、卸载移入备份的方式。检查版本中，更新先移除旧目录再替换，启停靠部署目录；本方案改为保留旧版本、事务部署和原生启停规则。代码中的 `$CODEX_HOME/skills` 路径说明不能替代当前官方文档。[Skills 源码](https://github.com/BigPizzaV3/CodexPlusPlus/blob/48d43158688f5096c7059c690f8cd1daab340681/crates/codex-plus-core/src/skills.rs#L356)。

只参考行为与工程经验，不复制源码、界面、交互或文案。项目声明 AGPL-3.0-only，并包含 CDP/注入能力；这些不进入本仓库的实现。[项目说明](https://github.com/BigPizzaV3/CodexPlusPlus/blob/48d43158688f5096c7059c690f8cd1daab340681/README.md)。

### Vercel Skills：借鉴来源身份与内容锁定

安装器区分来源目录、共享目录和客户端目录；锁文件记录来源 URL、Skill 路径及文件夹摘要。适合借鉴来源可追溯和整个目录的变化检测，不仅检查 `SKILL.md`。本产品不把 `npx skills` 作为内部执行器，避免产生第二套真实配置写入入口。[安装器](https://github.com/vercel-labs/skills/blob/435076e78988e1e6ec40d00b0b1d76bdbbc5419a/src/installer.ts)、[锁文件](https://github.com/vercel-labs/skills/blob/435076e78988e1e6ec40d00b0b1d76bdbbc5419a/src/skill-lock.ts)。

该提交的目录表也仍包含 Codex 的历史全局路径，因此目录规则最终由官方规范和目标客户端验收拥有。[客户端映射](https://github.com/vercel-labs/skills/blob/435076e78988e1e6ec40d00b0b1d76bdbbc5419a/src/agents.ts#L219)。

## 3. 本仓库现状与接入原则

已检查的直接所有者：

| 位置 | 当前事实 | 新模块如何接入 |
| --- | --- | --- |
| `crates/asb-core/src/ownership.rs` | MCP、Skills 与插件仍归 `PreserveOnly`；另有 Skill 安装 MCP 依赖的客户端偏好 | 将实际可管理路径拆成独立资源，权限、Hooks、插件、受管策略继续保留 |
| `crates/asb-core/src/contracts.rs` | 当前共享契约集中于 core | 新增 `extensions` 模块作为扩展契约唯一所有者 |
| `crates/asb-switch/src/executor.rs` | 文件预览、哈希冲突检查、备份、替换及回读；Codex 有配置与认证配对写入 | 复用锁和 I/O 边界，增加扩展事务，不在 Tauri 服务中直接写客户端文件 |
| `crates/asb-switch/src/prompt_documents.rs` | 全局指令已有专属文件事务 | 证明扩展不必借供应商重新应用才能生效 |
| `crates/asb-switch/src/io.rs` | `SwitchIo` 主要面向文本文件 | 增加有必要的字节、目录和持久化操作，配套故障注入 |
| `src-tauri/src/local_state.rs` | 已解析 `CODEX_HOME`、`CLAUDE_CONFIG_DIR`，现有 Claude 目标是 `settings.json` | 扩展路径解析共享入口，但不能把 MCP 错写到提供商目标文件 |
| `src-tauri/src/config_store/{mod,snapshot}.rs` | 重置会删除整个 `state/configuration/`，快照只含供应商（含独立运行参数）、客户端偏好、历史 | 扩展放独立根目录，现有重置和云备份契约不扩大 |
| `src/api/client.ts` | 唯一前端后端调用边界 | 扩展命令继续由这里暴露，组件不生成 TOML、JSON 或目标路径 |
| `src/app/AppShell.tsx` | 已有十个顶层入口 | 仅增「扩展」一个入口；Skills/MCP 位于工作区内 |

当前已存在独立的扩展工作区实现；本节保留接入原则，不能据此推断本设计的所有项目都已经交付。已交付范围和仍缺少的用户流程由审查记录逐项列出。

## 4. 原生配置兼容矩阵

以下为默认路径。`<project>` 是用户明确选取且已解析真实路径的项目根，不是随意猜测的当前目录。

| 资源 | Codex | Claude Code |
| --- | --- | --- |
| 用户 Skills | `~/.agents/skills/<name>/SKILL.md` | `~/.claude/skills/<name>/SKILL.md` |
| 项目 Skills | `<project>/.agents/skills/<name>/SKILL.md`；还需识别 CWD 到仓库根的相关层级 | `<project>/.claude/skills/<name>/SKILL.md`；识别客户端的父级与嵌套发现规则 |
| Skill 启停 | `config.toml` 的 `[[skills.config]]` 路径规则 | 对应 settings 文件中的 `skillOverrides.<name>` |
| 用户 MCP | `$CODEX_HOME/config.toml` 的 `mcp_servers.<key>`；默认 `~/.codex/config.toml` | `~/.claude.json` 顶层 `mcpServers.<key>` |
| 项目共享 MCP | `<project>/.codex/config.toml` 的 `mcp_servers.<key>`；受项目信任限制 | `<project>/.mcp.json` 的 `mcpServers.<key>` |
| 项目私有 MCP | 首版不创造 Codex 私有项目文件格式 | `~/.claude.json` 的 `projects[绝对项目路径].mcpServers.<key>` |
| 原生 MCP 停用 | `enabled = false` | `/mcp` 的停用状态属于项目级 `disabledMcpServers`，不是全局服务字段 |
| 传输 | stdio、Streamable HTTP | stdio、HTTP；另有已弃用 SSE 与 WebSocket |

路径与状态依据：[Codex Skills](https://developers.openai.com/codex/skills)、[Codex MCP](https://developers.openai.com/codex/mcp)、[Claude Skills](https://code.claude.com/docs/en/skills)、[Claude MCP](https://code.claude.com/docs/en/mcp)。

### 必须保留的语义差异

1. **Codex 用户 Skill 根不等于 `CODEX_HOME/skills`。** 当前文档使用 `~/.agents/skills`；历史目录与内置 `.system` 可以作为只读发现来源，不能因发现了目录就自动迁移或认定为当前加载路径。`~/.agents/skills` 也可能被其他工具发现，界面说明这是原生共享目录，不承诺仅 Codex 能读取。
2. **Codex Skill 路径存在文档不一致。** Skills 指南示例指向 `SKILL.md`，配置参考文字描述为目录。检查 OpenAI 固定提交的配置规则以 canonical document path 进行匹配；实现按该逻辑生成文件路径，并以选定发行版的隔离加载测试作为开放写入条件。不能靠“配置解析未报错”证明启停有效。[配置参考](https://developers.openai.com/codex/config-reference)、[规则源码](https://github.com/openai/codex/blob/9587c9ef366bd678ea5e9310f59ec33fdc44df7e/codex-rs/config/src/skills_config.rs)。
3. **Claude 的手动调用与停用不同。** `skillOverrides` 的 `on / name-only / user-invocable-only / off` 分别表示正常、仅名称、仅手动和关闭；`disable-model-invocation` 仅限制自动调用。基础 UI 提供启停，高级详情保留四态，不改第三方 `SKILL.md`。[可见性设置](https://code.claude.com/docs/en/skills#override-skill-visibility-from-settings)。
4. **同名规则不统一。** Codex 可能同时列出同名 Skill；Claude 的 Skill 覆盖顺序与 MCP 的 local/project/user 优先级不是同一套。显示来源和被覆盖关系，不能用一个通用“项目优先”算法。[Codex 同名行为](https://developers.openai.com/codex/skills#where-codex-loads-local-skills)、[Claude Skill 来源](https://code.claude.com/docs/en/skills#where-skills-live)。
5. **MCP 两端只把 stdio/HTTP 作为共同可编辑核心。** SSE 与 WebSocket 可作为 Claude 专用类型管理，并明确不支持投影到 Codex；跨客户端操作在预览前阻止不支持的目标。导入未知传输仍可只读展示，不能静默转换成 HTTP。
6. **配置路径覆盖要逐资源解析。** Claude 的 `settings.json`、用户 MCP 状态文件、凭据文件不是同一文件。自定义 `CLAUDE_CONFIG_DIR` 下的 MCP 状态位置必须经该发行版的隔离夹具核实，未核实只读并给出原因，不硬编码为相邻文件，也不回退误写默认家目录。[环境变量规范](https://code.claude.com/docs/en/env-vars)。
7. **Windows 上 Claude 的 stdio 启动器要包成 `cmd /c`。** 原生 Windows 的 Claude Code 直接 spawn `command`，`npx`/`npm`/`yarn`/`pnpm`/`node`/`bun`/`deno` 这类 `.cmd` 垫片无法启动。扩展库只保存可移植形态（`npx …`）；`asb_core::extensions::mcp::claude_launcher` 是唯一所有者：Windows 主机的 Claude 投影（用户级与项目私有）写成 `cmd /c <launcher> …`，从 `~/.claude.json` 导入/接管时按同一规则还原为可移植形态，连接检测在 Windows 也用同样包装启动进程。Codex 投影与非 Windows 主机不受影响；已是 `cmd`/`cmd.exe` 或非垫片命令原样写入。[Claude Windows 说明](https://code.claude.com/docs/en/mcp#windows-setup)。

发布时维护一张小型 `ClientCapabilities` 表，记录实际测试过的客户端版本、资源路径规则、传输、启停字段和验证日期。未知版本可发现和预览；有已知不支持项时禁止相应写入。不要维护历史路径自动双写，不读客户端 SQLite 来猜测能力。

## 5. 产品信息架构与页面

### 5.1 扩展工作区

2026-09-07 按用户要求重构：参考同类工具的管理流程，视觉由本项目 Frosted Relay 决定。对照的 UI 源码为 `UnifiedSkillsPanel`、`UnifiedMcpPanel` 与 `SkillsPage`（基线 `389dd96cb8567f41d05ba4aaebce8a73dc524040`）。具体视觉与交互事实由 [DESIGN 扩展工作区](../DESIGN.md) 拥有。

```text
扩展 [Skills] [MCP]                 [从本机发现] [发现 Skills / 添加 MCP] [更多]
当前类型总数                         [Codex 数量] [Claude 数量]
[搜索名称、描述或命令                                      ] [客户端]
名称与摘要                         客户端开关            更新 / 编辑 / 删除
```

Skills 的来源发现独立成页，GitHub / 本地目录经显式读取后展示可筛选的候选卡片，支持连续加入扩展库。新建、已有编辑、本机导入、项目注册、便携包和操作历史通过弹窗进入。页面保留本项目蓝紫身份色、实体表面、字体和控件，不复制参考项目配色。

### 5.2 行状态与交互

- 资源列表与管理详情分离；点击条目打开详情，行内编辑直接打开编辑器。弹窗统一管理焦点和 Esc，嵌套预览只关闭最上层；执行期间不能关闭。
- 行内客户端开关聚合现有作用域为未启用 / 部分启用 / 全部启用，部分启用点击后启用其余绑定；无绑定才安装到用户作用域。已有绑定保持自己的目标，额外项目安装从详情选择。
- 数量条与批量操作都覆盖当前类型的整个库，不受搜索结果裁剪；不支持的客户端不可点击。全部启用包括新安装和停用绑定的恢复，操作请求共用一个状态计算所有者。
- 状态以最后读取的库和实际文件状态为准，点击不会预先亮起；任何客户端写入必须先预览再确认。Claude 项目共享 Skill 停用先选择规则范围；复用原执行器、备份和恢复记录。
- 检查更新由用户显式触发；行内呈现可更新与失败。更新入库后清除过期提示，并对有启用绑定的更新生成一份合并预览。
- 本机发现弹窗复用同一完整扫描快照，内部页签和过滤只改变显示范围；复制与管理现有安装仍是两种明确动作，警告和修复继续按当前结果收集。
- 空库、搜索无结果和加载失败分别呈现；来源读取失败保留上次成功候选。删除仍有绑定的定义时先引导移除安装。
- 原添加卡片、列表下详情、常驻历史、独立旧批量选择面板与相关样式整体移除，无双入口或过渡实现。

### 5.3 状态不能混成一个 enabled

| 维度 | 示例状态 | 依据 |
| --- | --- | --- |
| 管理关系 | 本机发现、应用管理、客户端内置、插件提供 | 来源和所有权记录 |
| 应用意图 | 未绑定、启用、停用 | 保存的目标绑定 |
| 文件一致性 | 未部署、已一致、待应用、外部改动、文件缺失、不可读 | 本次读取与上次部署摘要 |
| 客户端限制 | 待信任、被策略限制、被同名项覆盖、能力未核实 | 可读原生配置与能力表；无法证明时为未知 |
| 运行验证 | 未检测、检测中、协议检测通过、需原生登录、检测失败 | 明确的检测时间与定义摘要 |
| 会话生效 | 等待客户端重载、当前会话状态未知 | 不从配置文件推断当前进程状态 |

检测成功显示「本次独立检测通过」，不显示“Codex/Claude 已连接”。定义、环境引用或目标改变后，旧检测记录标记过期。

## 6. Skills 管理全流程

### 6.1 发现、导入与来源

支持公开 GitHub 仓库、仓库子目录、明确 ref、用户选择的本地目录。私有仓库首版通过用户已检出的本地目录导入，不新增 GitHub 账号系统或把 Token 放入仓库 URL。

默认来源建议是 `openai/skills` 与 `anthropics/skills` 的可选入口，首次进入不自动请求它们，也不默认安装。官方标识只授予明确维护者来源，热门或第三方精选单独标记。

发现规则：逐个含 `SKILL.md` 的目录建立候选；来源身份为规范化仓库身份 + 子路径，内容身份为已解析 commit + 内容摘要。仓库名称、目录名和 frontmatter `name` 分开保存，不能用显示名作主键。

本机候选可选择「加入库」或「接管此安装」：前者只复制来源进应用库，原目录不变；后者要预览目标、记录基线与所有权后才允许以后修改。发现本身不创建目录、链接、索引标记或客户端配置。

### 6.2 校验与兼容

- 新建通用 Skill 按 Agent Skills 规范要求 `name`、`description`，保留 `scripts/`、`references/`、`assets/` 等完整目录。[规范](https://agentskills.io/specification)。
- 导入宿主已有 Skill 时按宿主真实规则解析：例如 Claude 允许省略某些 frontmatter 字段，不能因不满足可移植规范就把有效原生内容判为损坏。此类记录标记为 Claude 专用，跨端前需创建符合通用规范的本地副本。
- `agents/openai.yaml`、Claude 的 `context`、`agent`、`allowed-tools`、动态命令等原样保留并展示兼容诊断。能复制文件不等于两端执行语义一致，不自动改写内容以伪造通用支持。
- 阅读说明只渲染 Markdown 和静态文本，不执行 Skill 脚本、动态命令、安装钩子或引用里的指令；不自动加载远程图片。
- 导入归档需限制展开后体积、文件数、单文件大小和目录深度，拒绝路径穿越、绝对路径、越界链接、大小写碰撞及 Windows 保留名。对正常本地链接先展示解析目标，首版只接收可完整物化的普通文件目录，不遍历不明 reparse point。

### 6.3 安装与启停

1. 选择 Skill、已解析版本、客户端和作用域。
2. 展示文件清单、关键声明、兼容结果、同名冲突和目标位置；不兼容目标不可勾选。
3. 下载或复制到应用暂存区，计算整个目录摘要，产生预览。
4. 确认后按第 10 节事务部署副本，并记录客户端路径与内容基线。
5. 回读文件与原生启停规则，显示「已部署，当前会话需重新加载或重新开始」。

**首版固定复制部署。** 应用库放在客户端发现目录之外，保证“入库但不启用”不会被自动加载。复制带来的磁盘成本换取 Windows 权限一致性、明确所有权和不同客户端独立更新。源目录与目标目录相同或父路径实际指向同一位置时，阻止自我覆盖。

Codex 停用保留文件，并设置对应文档路径的 `enabled = false`；重新启用仅撤销本应用拥有的停用规则，外部规则或策略仍限制时报告限制。Claude 停用使用目标作用域中的 `skillOverrides[name] = "off"`；启用恢复本应用修改前的值或删除本应用新增的 override。已有高级可见性状态不能被一个启用动作强行重置成 `on`。

Claude override 按名字生效，可能影响同名不同来源；存在歧义时必须先解决名称或范围，不允许把它伪装成只针对某个目录的开关。项目 Skill 的个人停用写项目的 `.claude/settings.local.json`，预览说明该文件可能被版本控制，不擅自编辑 `.gitignore`。项目共享可见性变化需用户明确选择共享设置目标。

### 6.4 更新、修改、卸载与恢复

更新先解析来源 ref 到新 commit，对比整个 Skill 子树。三方依据是上次部署内容、当前目标内容、来源新内容：只有目标未变时可直接产生更新计划；本地改动必须选择保留本地副本或显式用新版替换，不能自动丢弃。

支持单项和批量检查更新、锁定版本、查看新旧 commit、文件级差异、应用到选定目标。批量计划有统一预览，任一阻断冲突使整批不可执行；用户可移除冲突项后重新预览剩余集合。

编辑器只修改「本地副本」或自建 Skill，保存形成新内容版本。远端来源副本编辑后标记 `localModified`，仍保留原始来源作为比较基线，但不再无提示覆盖式更新。

「从客户端移除」只处理选定绑定及其自有启停规则；「从库删除」先列出所有部署引用并一并规划移除。所有权不符或目标被外部修改时不直接删除。旧内容移入事务备份；恢复先预览，若原位置被新内容占用则报告冲突。

### 6.5 Skill 与 MCP 的关联

读取明确声明的工具依赖，并允许用户手工关联库里的 MCP。依赖状态为已绑定、待配置、目标不支持或来源不明；不得依据自然语言推断后自动执行命令。

点击「配置所需 MCP」进入带来源说明的 MCP 草稿，凭据由用户补充。用户选择后可与 Skill 安装合成一个计划；不改变 `features.skill_mcp_dependency_install` 等客户端原有偏好。停用 MCP 时列出受影响 Skill，但不连带停用；界面说明依赖可能不可用。

## 7. MCP 管理全流程

### 7.1 创建、导入与编辑

提供结构化表单、带类型的 JSON/TOML 片段导入，以及读取本机已有定义。解析结果先作为草稿，逐项列出识别字段、不支持字段和作用域；不提供任意整份配置覆盖入口。

| 公共字段 | Codex 输出 | Claude 输出 |
| --- | --- | --- |
| 本地命令、参数 | `command`、`args` | `type: "stdio"`、`command`、`args` |
| 显式环境值 | `env` | `env` |
| HTTP 地址 | `url` | `type: "http"`、`url` |
| 静态请求头 | `http_headers` | `headers` |
| Bearer 环境变量引用 | `bearer_token_env_var` | Authorization 中的原生环境变量表达式 |
| 其他请求头环境引用 | `env_http_headers` | 对应 header 的原生环境变量表达式 |
| stdio 宿主环境引用 | 支持的同名变量输出到 `env_vars` | 对应 env 字段使用原生变量表达式 |

映射只在明确支持的字段进行。Codex 不把 `${VAR}` 字符串当通用环境插值；跨名称环境映射不受支持时拒绝该目标，或由用户改为明确的独立凭据值，不能悄悄读取环境后固化到磁盘。Claude 的插值行为以其官方 MCP 文档为准。[Codex 字段](https://developers.openai.com/codex/mcp#configure-with-configtoml)、[Claude 插值](https://code.claude.com/docs/en/mcp#environment-variable-expansion-in-mcpjson)。

Codex 的 `cwd`、启动/工具超时、`required`、工具允许/拒绝列表归 Codex 专属字段。Claude 的传输专属字段归 Claude 类型；没有已核实等价项就不跨端翻译。认证头辅助命令、远程 executor、工具自动批准等高级输入先保留只读，不用通用 JSON 字段偷偷接受。

同一配置文件内新增时检查 `serverKey`，保留用户导入的原键；编辑显示名不改变键。改键属于显式“重命名服务”，须同步自有绑定和已保存的依赖引用，并提示原生 OAuth 可能需要重新登录。

### 7.2 启停与移除

- Codex：绑定停用写原生 `enabled = false`，定义留在客户端。启用恢复本应用先前管理的值；外部配置变化先解决冲突。
- Claude 用户级：没有把 `enabled: false` 写进服务定义的约定。本应用的“停止向 Claude 全局提供”撤下受管用户级服务项，定义保留在应用库。按钮与预览明确写「从 Claude 用户配置移除，库中保留」。
- Claude 项目私有服务可撤下该项目的受管定义；对用户级/项目共享服务的“仅在此项目停用”，管理该项目原生 `disabledMcpServers` 的单个成员，并保存原始成员状态。
- 全局撤下并不保证某个项目不可使用同名服务；项目、插件或托管来源可能仍提供它。展示仍存在的候选来源，未知范围不声称已彻底禁用。
- `disabledMcpjsonServers` 属于项目服务器批准策略，不能拿它替代普通停用，也不自动授予项目信任或批准。内置默认关闭服务的 opt-in 状态不接管。[Claude 原生停用语义](https://code.claude.com/docs/en/mcp#disable-a-server-without-removing-it)。

卸载先规划移除所有被选择的绑定，成功后再删除库定义；OAuth 凭据留给原生客户端管理，不把删除配置等同于撤销远端授权。

### 7.3 验证分三层

1. **静态验证**：协议、字段、命令路径、运行时存在性、环境变量名及必填项。只做读取；不能通过执行 `npx`、`uvx` 或服务器进程来“检查是否安装”。
2. **显式连接检测**：用户确认要运行的命令或访问的端点后，使用有界 MCP 测试客户端读取服务元数据和工具/资源/提示词目录，不调用业务工具。`npx`/`uvx` 可能下载并执行包，属于本次检测的明确副作用；依赖版本由用户提供或模板锁定，不自动升级。
3. **原生客户端确认**：引导用户在客户端 `/mcp` 等原生入口确认实际可用状态。独立测试不能证明宿主的环境继承、项目信任、OAuth 或运行时完全相同。

协议检测必须区分协议版本。当前 MCP `2026-07-28` 采用逐请求版本元数据，早期版本使用 initialize 流程；不能把成功执行 initialize 写死为唯一通过条件。用官方 SDK 已验证的传输及版本能力，持久化本次检测版本，未支持版本给出明确错误。[协议版本规范](https://modelcontextprotocol.io/specification/2026-07-28/basic/versioning)。

检测给定总时限、输出大小及分页上限；结果截断要标示。stdio 进程归本次检测所有，结束、超时和取消后清理自己的进程树，不终止原生客户端持有的进程。Windows 默认隐藏检测窗口。网络错误显示分类、状态码和耗时，stderr 与响应内容先脱敏再展示；握手成功但目录读取失败为部分结果。

### 7.4 凭据和 OAuth

公共定义只保存环境变量引用或 `SecretRef`；新输入的敏感值通过专属写入接口送入系统凭据存储。非敏感参数可直接保存，私有端点可存在后端定义中，但所有普通列表、预览、导出与日志均显示稳定脱敏标记。

系统凭据存储不可用时，可保存不含秘密的草稿并使用已有环境变量引用；需要 SecretRef 的应用操作明确失败，不回退成明文秘密文件。确需静态凭据时，执行器在确认后将其写入客户端原生 env/header 字段，预览准确列出哪些目标会含凭据；不得把“库中是引用”说成“客户端不会落盘”。

不自动设置系统环境变量，也不把 Agent Switchboard 的进程环境当作未来终端的确定环境。检测输出只说“本次检测环境可解析”，无法证明宿主环境时显示未知。

OAuth 完全交给客户端原生登录。只在用户点击「到客户端授权」后引导交互；不导入、复制、读取展示其 OAuth Token。若官方 CLI 登录还可能改配置，交由用户在客户端完成，不绕开执行器自动运行。独立检测没有原生授权凭据时显示「需要在客户端验证」，不认定服务不可用。

## 8. 数据契约与存储

以下是拟新增契约名称，不是已经存在的类型。Rust `asb-core::extensions` 拥有可序列化契约与验证，前端只消费其 IPC 投影；禁止另写一份客户端兼容表。

| 类型 | 主要字段/职责 |
| --- | --- |
| `ExtensionId` / `BindingId` / `SourceId` | 不可变 UUID；展示名和服务键不作为内部 ID |
| `ExtensionDefinition` | 带标签的 `Skill` 或 `Mcp` 联合类型，schemaVersion、revision、name、来源 |
| `SkillDefinition` | sourceId、subpath、resolvedCommit、本地版本、contentDigest、manifest 元数据、兼容诊断、显式依赖 |
| `McpDefinition` | 带标签的 stdio/http/claudeSse/claudeWs 类型；结构化参数、认证引用、专属客户端选项 |
| `ExtensionTarget` | `app` + 用户/项目共享/项目私有作用域 + 可选 projectId；非法组合直接拒绝 |
| `ExtensionBinding` | resourceId、target、原生键或部署名、期望启停、锁定内容版本、上次应用 revision |
| `ManagedBaseline` | 自有文件/键/集合成员、原始值或原始不存在、上次写入值、文件与内容摘要、备份引用；仅后端可读 |
| `ObservedExtension` | 本次发现来源、规范化身份、实际状态、诊断及时间；不是期望状态的第二份持久事实 |
| `ExtensionPlan` | 不透明 planId、定义 revision、绑定变化、前置条件、文件/目录步骤、凭据版本、到期时间、脱敏差异 |
| `ExtensionOperation` | 操作类型、阶段、逐目标结果、备份引用、恢复结果、可操作错误码 |
| `McpCheckResult` | 定义摘要、目标/检测环境摘要、时间、协议版本、阶段结果、目录摘要、错误分类 |
| `ClientCapabilities` | 客户端已验收能力、路径规则及核实版本；界面投影由后端派生 |

内部 schema 严格解析；不接受旧字段或任意 `extra: JSON`。宿主文件中的未识别字段保留在原始文档树中，属于只读 pass-through，不能通过导入进入可编辑契约。改已存在项时只补丁自有字段；不支持字段导致跨端信息丢失时阻止相应转换，并指出字段。

```text
<app-data>/
  state/
    configuration/                  # 现有供应商（含独立运行参数）、客户端偏好、历史
    extensions/
      manifest.json                 # schemaVersion、generation
      definitions/<uuid>.json       # Skill 来源身份内嵌在定义中
      bindings/<uuid>.json
      projects/<uuid>.json          # 本机项目位置及显示名；不云传输
      baselines/<binding-id>.json    # 受限本地元数据
      library/<skill-id>/<digest>/   # 不可变 Skill 内容版本
      transactions/<operation-id>/   # journal、受限恢复载荷
      history/<operation-id>.json    # 脱敏可展示记录
      checks/<definition-id>.json    # 最新 MCP 检测结果
    backups/extensions/<operation-id>/ # 必需的本地恢复副本
```

定义 revision 单调递增；Skill 内容版本按文件相对路径、文件类型、字节内容和有业务意义的权限位计算稳定摘要，不使用修改时间判定版本。读取发现只生成观察结果，不悄悄把外部变更收编为已应用。

秘密引用存入系统凭据库，文件路径、私有 URL、恢复原值和原始备份留在后端受限本地存储；原始备份可能包含凭据，绝不进入可展示历史或普通日志。备份保留遵循已有用户选择，未解决事务和仍被引用的版本不得自动清理。

扩展数据库与现有 `ConfigurationSnapshot` 分离，现有“清空旧档案”和供应商云备份恢复不能删除扩展。完整本地备份入口增加扩展分类；便携导出只带用户选择的 Skill 文件、非敏感定义和待补充凭据声明，不带绝对路径、SecretRef 实体、基线或 OAuth。导入只进入库，重新绑定并预览后才部署。首版不把扩展加入云备份。

## 9. 命令边界

所有命令通过 `src/api/client.ts`，Tauri 命令只编排；纯解析与差异归 core，真实配置写入归 switch。

| 命令组 | 输入 | 输出与副作用 |
| --- | --- | --- |
| `list_extensions` / `discover_extensions` | 类型、客户端、已登记项目、筛选 | 只读 DTO；不创建用户目录、不运行 MCP |
| `save_extension` / `save_extension_source` | 当前 revision、严格草稿 | 只写扩展库；不更新客户端 |
| `resolve_skill_source` / `check_skill_updates` | sourceId、ref、选择的 Skill | 用户请求的远端读取、应用缓存 |
| `prepare_extension_plan` | 扩展/绑定 ID、操作、revision | 准备版本、前置条件和脱敏差异；不写客户端 |
| `apply_extension_plan` | planId | 执行已准备内容；不接受任意路径或替换文本 |
| `get_extension_operation` | operationId | 逐阶段进度、错误与恢复结果 |
| `prepare_extension_restore` | operationId、选择的目标 | 恢复也产生新 plan，仍有冲突检查 |
| `check_mcp_connection` / `cancel_mcp_check` | 定义 revision、目标或明确测试上下文 | 用户发起的网络/进程检测；不修改客户端配置 |
| `put_extension_secret` | 新敏感值、用途 | 返回 opaque SecretRef；常规读取接口不返回敏感原值 |

plan 在后端持有可执行载荷，前端拿到的始终是脱敏视图。apply 重新核对计划中的源版本、目标存在性、原始摘要、目录身份、权限和秘密版本；过期或发生变化返回 `PlanStale`，用户重新预览，不能自动重算后直接写入。

## 10. 写入事务、冲突与恢复

```mermaid
flowchart LR
  A[读取现状和草稿] --> B[校验并准备不可变候选]
  B --> C[展示脱敏差异]
  C --> D[用户确认]
  D --> E[按固定顺序加锁和复核]
  E --> F[备份与持久化日志]
  F --> G[暂存 回读 校验]
  G --> H[最终冲突检查和逐目标替换]
  H --> I[写后验证并提交绑定记录]
  I --> J[完成并提示客户端重载]
  H --> K[逆序恢复]
  I --> K
  K --> L[已恢复或需要手动恢复]
```

### 10.1 对现有执行器的具体扩展

- 新增有限步骤集合：文档键补丁、受管目录部署、受管目录撤下、绑定/基线提交。不要建立可执行任意系统命令的通用工作流引擎。
- 共享客户端文件必须使用与供应商写入相同的锁命名和冲突策略。Codex MCP、Skill override、供应商都可能修改 `config.toml`，Claude Skill override 与供应商都可能修改 `settings.json`，不能分别各拿一把互不相识的锁。
- 一个批次按规范化目标路径排序加锁，另持有扩展库 generation 锁；两条绑定解析到同一物理文件时合并补丁。原生客户端不遵循这些锁，最终摘要复核仍必需。
- 网络下载在持锁前完成。暂存文件在目标文件系统内创建，校验后再原子替换单个文件；跨盘不能把 rename 当可行前提。
- 非空目录跨平台不存在统一的覆盖式原子替换：先写新目录，备份旧目录，再进行同盘重命名切换，journal 记录每一步。明示短暂切换窗口，运行中的客户端可能观察到中间状态；提示停止相关会话或之后重载，不能承诺运行时无缝切换。
- journal 在首次变更前持久化，并在每次替换/提交边界记录可恢复阶段；扩展绑定最后提交。如果库提交失败，客户端变更也回滚，不能让磁盘和 UI 各成功一半。读取扩展库时遵循 generation 与事务提交标记，禁止把正在写入的半批绑定投影成完成状态。
- 多文件批次是**可恢复事务**，不是文件系统原子事务。一个客户端成功、另一个失败时回滚整个已批准批次，结果逐目标列出；不把部分成功当全部成功。

### 10.2 冲突处理

操作前比较原始文档和已拥有字段。发现变化时展示“库中目标 / 磁盘现状 / 上次应用”三方差异；用户可采用磁盘值为新草稿、保留库值重新生成计划，或取消。新预览是下一次写入授权依据，不提供跳过冲突检查的强制写入。

TOML 使用现有 `toml_edit` 机制保留未识别节点和尽可能多的格式；JSON 保留所有未修改字段和业务值，若需重新排版在预览中说明。导入含未知字段的既有 MCP，只编辑可理解字段，删除整个资源必须在预览中包含其完整移除范围。

卸载以 bindingId 和上次内容基线证明所有权，不按目录名递归删除。删除前再检查规范化路径、文件类型和 reparse point 是否发生变化；有未知新增文件时转冲突。

### 10.3 失败与启动恢复

- 替换前失败：移除本次暂存，原配置不变。
- 替换后失败：逆序恢复本事务最近备份；原来不存在的目标恢复为不存在，原有内容、权限与自有规则原值一并恢复。
- 回滚前若目标已经不是本事务刚写入的内容，停止覆盖，保留备份并报告 `RecoveryRequired`，避免回滚吞掉原生客户端后续写入。
- 进程崩溃后读取未完成 journal。在确认锁持有者已退出、目标仍匹配可恢复状态时完成回滚；不确定则显示恢复入口并阻止对相关目标继续写入，不静默删除活动锁。
- 回滚自身失败时，显示失败目标、已恢复目标、恢复副本位置和下一步；保留诊断，不能只显示“应用失败”。

## 11. 凭据与文件展示规则

沿用现有稳定脱敏标记，并扩展到 Skill 内容、MCP URL、query、headers、env、args、原始错误及来源身份。新输入值只能出现在用户正在编辑的专属输入控件，保存后常规 DTO 仅返回“已设置”。

不能只按变量名含 TOKEN/KEY 来决定是否敏感；用户标记、已存秘密匹配、认证位置及私有端点分类共同决定隐藏范围。公开 URL 可展示，私有 URL 用稳定标记；不可确定的导入原始文本默认按敏感处理。复制配置提供脱敏版本；不得把脱敏星号再写回客户端。

Skill 文件可能夹带凭据；便携导出先提供排除列表和扫描结果，发现已知秘密则阻止该内容导出或由用户删去相关文件。扫描不是无秘密保证，因此不自动上传 Skill 内容或将它纳入现有云备份。

## 12. 范围、策略与外部安装器

全局视图只证明用户级配置；项目视图读取选定项目的相关层级、信任限制和可访问的受管策略。不能扫描全盘项目或汇总未知项目后宣称全局没有冲突。

受管/系统/插件来源只读：显示来源及原生管理入口，不编辑插件缓存、包管理器锁文件、Codex 安装包、数据库或 Claude 受管配置。若用户要修改其 Skill 内容，应「复制为本地 Skill」并解决名称与加载冲突。

项目迁移、目录改名、工作树或符号链接解析变化后，旧 projectId 绑定标记位置失效；用户重新选择位置并预览，不自动把字符串替换到全局状态。

其他管理器改过目标文件或部署副本后，显示外部改动。没有文件监听器自动反向同步、启动自动修复、供应商切换时自动重装或定时更新。监听只使本地快照失效，刷新后重新计算观察状态。

## 13. 实施拆分与文件所有权

| 阶段 | 交付范围 | 退出条件 |
| --- | --- | --- |
| A：契约与只读发现 | core 类型、路径能力表、扩展库、用户级 Skill/MCP 发现、单页骨架 | 真实配置零写入；来源/同名/损坏配置/未知字段可解释；当前稳定版能力夹具通过 |
| B：MCP 用户级闭环 | stdio/HTTP 表单、导入、原生启停、秘密引用、预览、应用、恢复 | 两端隔离配置通过；供应商切换保留 MCP；第二目标失败可恢复；静态校验不执行命令 |
| C：Skills 用户级闭环 | 本地与 GitHub 来源、完整副本、启停、版本更新、冲突、卸载恢复 | Windows/macOS/Linux 目录故障场景通过；停用实际不可调用；外部内容不被误删 |
| D：项目与高级能力 | 项目共享/私有范围、原生策略观察、Claude SSE/WS、独立连接检测、依赖关联与便携导出 | 范围优先级、重名、隐私、OAuth 引导与协议版本测试通过；完整方案验收 |

具体代码落点建议：

| 模块 | 拟新增/调整 |
| --- | --- |
| core | `crates/asb-core/src/extensions/{mod,contracts,validate,skill,mcp,plan}.rs`；`adapter/` 添加纯扩展补丁函数 |
| 执行器 | `crates/asb-switch/src/extensions/{mod,transaction,recovery}.rs`；按所需能力扩展 `io.rs`，复用现有锁 |
| 桌面服务 | `src-tauri/src/extensions/{store,paths,discovery,sources,secrets,checks}.rs`；`commands/extensions.rs` |
| 前端 | `src/pages/ExtensionsPage.tsx`、`src/components/extensions/`、`src/app/useExtensions.ts`；仅 `src/api/client.ts` 调用 IPC |
| 应用装配 | `commands/mod.rs`、Tauri 命令注册、`dev_api.rs` 及其隔离开发夹具、`App.tsx`、`AppShell.tsx` |
| 测试 | core 适配测试、switch 事务故障测试、Tauri 范围测试、前端流程测试、真实版本隔离加载夹具 |

这不是预先要求把每个功能拆成很多文件；实际按单一所有者与可读性收束，避免建立第二套库、第二套配置写入或重复类型映射。

可能需要的依赖仅限实际缺失的 YAML/归档解析、系统凭据存储和官方 MCP SDK。先核对当前 Cargo 依赖复用情况，确有必要再选维护活跃且目标平台可用的包，并同步锁文件；不在设计阶段安装运行器或引入一套 Node 后端。

### 同步更新的直接消费者

实施时，`ownership.rs` 将 MCP 与 Skill 自有路径改成独立模块，Hooks/插件保持原分类；官方设置目录组件和测试同步更新。`website_assembly.rs` 当前从 `PreserveOnly` 派生官网示例，需把“供应商切换仍保留的范围”和“全产品是否可管理”分开表达，防止新增独立模块后官网误称供应商会覆盖 MCP。

客户端设置只提供跳转，不重复编辑扩展；`features.skill_mcp_dependency_install` 由客户端偏好拥有。供应商适配器和恢复流程继续保留扩展字段。备份页明确区分供应商历史、扩展操作历史和云备份范围，README 不宣称扩展已上云。

`DESIGN.md` 在开始实现页面时登记本方案落定的交互规范；`progress.md` 按阶段登记。当前提案不修改已批准视觉规则或现有已完成状态。

## 14. 验收矩阵

所有写入与登录边界测试使用临时 HOME、配置根和项目目录；不读取或复制真实用户秘密作为夹具。测试通过依赖进程参数注入临时环境，不能改当前用户环境或注册表。

| 场景 | 必须证明的结果 |
| --- | --- |
| 全新环境发现 | 返回空结果；不创建 `.agents`、`.codex`、`.claude` 或任何用户文件 |
| 自定义配置根 | 只命中明确目标；未知映射只读，不写默认目录 |
| TOML/JSON 扩展字段 | 对目标服务的修改不改变模型、OAuth、未知键、其他 MCP 或 Skill override |
| 供应商切换 | 应用、切换、恢复供应商后，扩展非受管内容仍保留 |
| 保存与取消 | 保存草稿只改变库；取消预览没有客户端写入 |
| 单目标和双目标 | 双端映射正确；第二目标失败回滚第一目标和绑定状态 |
| 并发编辑 | 预览后修改任一文件、库 revision、秘密版本或路径身份，apply 返回 PlanStale |
| 同文件并发 | 供应商与扩展共享锁；同批 Skill 与 MCP 的 Codex 配置补丁不会相互覆盖 |
| 各阶段故障 | 备份、暂存、回读、替换、写后校验、库提交、回滚均可注入失败并得到精确结果 |
| 崩溃恢复 | 每个 journal 边界中断后重启，要么恢复完整旧状态，要么给出 RecoveryRequired |
| Windows 目录 | 无管理员权限复制成功；占用失败可恢复；跨盘、Unicode、长路径及重解析点有明确结果 |
| macOS/Linux 目录 | 执行位保留；符号链接边界正确；不以 Windows 大小写假设归并路径 |
| Skill 原生启停 | 指定测试版实际加载清单/调用验证停用；不能只断言配置文本里有 false/off |
| 同名 Skill | Codex 多项、Claude 来源覆盖与名称 override 影响范围显示正确 |
| 未管理 Skill | 发现和入库不接管原目录；移除只删除通过基线验证的自有内容 |
| Skill 更新 | 只改 references/assets 也能发现；ref 固定；本地修改不被覆盖；可恢复旧版 |
| Skill 归档 | 路径穿越、越界链接、体积超限、大小写碰撞等拒绝，目标零写入 |
| Claude MCP 停用 | 用户级撤下与项目级 disabledMcpServers 分别正确；不写无效 enabled 字段 |
| 项目范围 | Codex trusted config、Claude local/project/user、受管限制和共享文件影响均可解释 |
| MCP 输入转换 | headers 映射正确；未支持的 cwd/传输/辅助命令等不能静默丢失或跨端接受 |
| 静态检测 | 不启动服务器、不安装依赖、不访问端点 |
| 独立 MCP 检测 | 本地假服务器覆盖支持的协议、认证失败、超时、分页、取消与进程清理；零业务工具调用 |
| OAuth | 不读取导出 OAuth Token；仅引导原生授权；独立检测缺授权不误报宿主失败 |
| 脱敏 | URL、env、headers、args、错误、Skill 文本和备份摘要不泄露夹具秘密；脱敏值不能回写 |
| 库重置/云恢复 | 现有供应商重置与快照恢复不删除扩展、不重新部署扩展 |
| UI 与可及性 | 40px 命中区、焦点恢复、键盘页签、200% 缩放、减少动效、失败后草稿保留 |

每阶段先跑对应 Rust/前端定向测试和类型检查；实际修改契约、IPC、注册与全局导航后补全相关构建。完成前检查 changed diff、旧字段/路径引用及直接消费者，不因文档设计就运行整仓测试或宣称原生兼容已验证。

## 15. 实施前仍需实测的边界

以下已有决策和处理策略，剩下的是发布验证，不需要先扩展产品范围：

- 确定项目要支持的 Codex/Claude Code 最低发行版，验证 Skill 路径、启停与自定义配置根。当前没有运行用户客户端做真实加载验证，不能给出虚假的最低版本号。
- Codex 指南与配置参考对 `skills.config.path` 的文字不一致，已由源码形成文件路径方案，但仍必须验证发行构建实际行为。
- `CLAUDE_CONFIG_DIR` 下用户 MCP 状态文件位置需使用官方 CLI 在隔离目录中创建无凭据假服务确认；普通设置路径不能代替这个实验。
- Claude SSE/WS 及 MCP 新旧协议检测能力必须以选定 SDK 的实际支持为准；不支持的组合保持只读并显示原因。
- 三平台目录切换、进程占用与崩溃恢复需要实机/CI 故障夹具，当前文件事务测试不能直接充当目录事务证据。

本方案的完成标准是：用户能清楚知道扩展来自哪里、写到哪里、对谁生效，以及失败时如何恢复；库、磁盘和运行状态各有真实依据。最先值得实现的是 A → B，先让结构化 MCP 配置完成事务闭环，再复用已验收机制处理 Skill 目录。
