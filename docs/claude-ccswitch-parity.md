# Claude 功能对齐验收

## 目标与约束

用户目标：软件的 Claude 相关 UI 不变，复刻 CC Switch 有关 Claude 的功能直到完整实现，同时保留本项目可视化通用配置文件的特色设计。

- 源码基线：farion1231/cc-switch，`d695a2d77fd9081eafd3e9eedcbf2a97b3410928`（2026-09-11，v3.20.3）。
- 保留既有 Claude 界面、控件、样式与可视化配置方案，不复制上游 UI。
- 保留供应商参数 / 客户端偏好的唯一所有权，`configuration/client-settings/claude.json` 继续保存共享偏好。
- 真实客户端写入仍经预览、确认、锁、备份、原子替换、回读与恢复事务。不得用整份上游 settings JSON 覆盖用户配置。
- 不修改真实配置、凭据或账户来做测试。新增后端能力必须连通其生产者、消费者、验证和已有入口。
- 以下清单不能以本轮修了若干缺陷而整体标记完成。未验证项保持未完成；若后续发现上游 Claude 能力，补入清单而非排除。

## 功能清单

| ID | 功能与上游依据 | 完成证据要求 | 状态 |
| --- | --- | --- | --- |
| C01 | Claude 档案 CRUD、排序、复制、导入、预设、端点与元数据；`services/provider`、`claudeProviderPresets.ts` | 数据持久化、往返导入、重名与并发编辑测试，已有 UI 入口连通 | 后端 CRUD / 预设已接通，预设经档案片段保真未映射键 |
| C02 | 独立 AUTH_TOKEN/Bearer 与 API_KEY/x-api-key；`providers/claude.rs` | 导入、编辑保存、直连投影、网关、模型获取、测试请求认证一致 | 已有导入 / 保存 / 请求链路已验证 |
| C03 | 主模型、Haiku/Sonnet/Opus/Fable、子 agent、1M 与显示名；`model_mapper.rs`、`services/proxy.rs` | 档案往返、切换清理、角色映射、官方恢复及真实 CLI 请求验证 | 模型契约 / 热映射已实现，继续验收 CLI 连续切换 |
| C04 | 共用配置、模型参数、快速开关、用户外改保留；`services/provider/live.rs`、`ClaudeFormFields.tsx` | 本项目可视化配置与单一所有权保留，切换不会丢失公共偏好 | 档案级附加片段（2026-09-14）承载按供应商任意键与上下文窗口默认值；effortLevel 补 max；全局 config_snippets 导入已实现并验证（`claude_snippets`，管理弹窗入口） |
| C05 | Anthropic 原生 / Chat / Responses，其他上游形态与提供商扩展；`providers/claude.rs` | 每条 Claude 支持路径都有入口、能力校验和完整请求/响应契约 | 三个既有协议已验证；Gemini Native 与云原生（Bedrock/Vertex/Foundry）传输已交付（2026-09-14，Gemini 经网关转换含 OAuth 凭据与 CLI 客户端标识头，云原生走 CLI 自带 SDK）；真实 Google/云上游验收待用户凭据 |
| C06 | 工具调用、错误结果、图片/文档媒体、并行结果次序；`transform.rs`、`transform_responses.rs` | Claude 两个跨协议目标的多轮工具闭环，失败结果可继续纠错 | 隔离多轮与真实 CLI 用例通过 |
| C07 | 缓存提示、thinking/effort/budget、工具策略与客户端扩展字段 | 合法字段处理覆盖实际 CLI 首轮和续轮，无法承载的能力有明确契约 | 已支持字段回归通过，仍需补全供应商扩展 |
| C08 | SSE 空增量、延迟身份、并行工具、推理/文本、usage、终止与错误 | 标准首帧、交错工具、结束后 usage、截断及异常回归 | 流式回归通过，继续完整验收 |
| C09 | Responses 原生 reasoning item 与 Claude 不透明推理往返；`reasoning_bridge.rs` | 原生密文完整还原，ASB 信封不出站，JSON/SSE 两轮与跨路由边界测试 | 同路由续接通过；跨供应商续轮经真实 CLI 验收通过（2026-09-15） |
| C10 | 原生代理接管、手动热切换、稳定角色别名与当前供应商一致性 | 运行中的 Claude 连续切换，不错投上游，状态/配置/记录同一提交 | 隔离热切换 / 失败回滚通过；接管态真实 CLI 完整通过网关，运行中手动热切换经真实 CLI 验收通过（2026-09-15，同一 CLI 安装连续两轮分别命中切换前/后供应商，切换前后凭据零泄漏） |
| C11 | 代理停止恢复、退出、崩溃与状态损坏恢复；`services/proxy.rs` | 故障注入、重启、端口冲突、原文件保留及可操作修复入口 | 停止恢复 / 状态修复已实现；退出（网关关闭→直连还原）、文件外改恢复、硬崩溃（进程强杀→重启路由再水化）与端口冲突（占用识别→拒绝越址写入→修复后复用原址）均经真实 CLI 验收通过（2026-09-15） |
| C12 | 超时、重试、故障转移、熔断及路由健康；`provider_router.rs`、`forwarder.rs` | 完整配置契约、单请求路由稳定、可取消、错误分类、隔离多上游验证 | 策略 / 重试 / 熔断后端已实现；跨供应商故障转移续轮经真实 CLI 验收通过（2026-09-15，主上游 503→备用接续工具结果）；操作入口待齐 |
| C13 | 端点选择、完整 URL、请求头/UA 与请求覆盖；`forwarder.rs`、`provider/endpoints.rs` | 导入及编辑保真，请求实际出口一致，凭据不串线 | 已支持端点 / 请求覆盖 / 模型地址已接通；候选端点面板与批量无凭据测速已接（2026-09-14，管理弹窗「档案连接」下） |
| C14 | 请求级历史、token/cache、费用、首 token 延迟、模型归因；`proxy/usage` | 重启保留、分页/汇总、价格与来源语义不和本机会话用量混淆 | 账本、计费与「请求计量」页面入口已接通（管理弹窗，含筛选/汇总/分页/详情/价格编辑）；内置参考价已对齐上游 seed 全表 199 模型（2026-09-14，含 claude/gpt/gemini/kimi/glm/deepseek/qwen/grok 等族），v1 价格文件加载时补齐且不覆盖用户改价；CLI 会话用量持久账本已接（2026-09-14，`claude_session_usage`：增量游标 + 外改钉住、消息 id 去重、本地价格表计价、与网关账本 10 分钟窗口匹配标记，独立文件与备份，入口在「请求计量」页），与网关账本分列不相加；真实厂商账单核对待用户凭据 |
| C15 | Claude 相关官方/托管认证、账户绑定、订阅及余额能力 | 按上游实际支持的 Claude 认证清单逐项核对；模拟授权、刷新、失效、切换验证 | 独立托管认证 / 订阅后端已接通；官方登录（Claude CLI 自身凭据）订阅额度已接（2026-09-14，`claude_native_quota`，只读 `.credentials.json`/macOS 钥匙串，`api/oauth/usage`，入口在管理弹窗「认证」）；真实授权与真实订阅接口待用户凭据 |
| C16 | Claude 的 MCP、Skills、提示词、会话、环境与插件集成 | 核对已实现能力与上游差异，保留本项目可视化设计和独立写入所有权 | 新增独立 Prompt 库及恢复事务；插件集成标记（`config.json` `primaryApiKey=any`）与首次引导跳过（`~/.claude.json` `hasCompletedOnboarding`）已接（2026-09-14，`claude_integration`，预览/确认/锁/独立备份/原子替换，切换后按策略自动对齐插件标记）；环境变量冲突检测（`ANTHROPIC*`：Windows 注册表 HKCU/HKLM、Unix 进程环境与 shell 启动文件）含脱敏扫描、带完整备份的删除与恢复已接（2026-09-14，`claude_env_conflicts`）；MCP/Skills 由扩展工作区承载；`hooks`/`enabledPlugins` 按所有权目录保留不写入 |
| C17 | CC Switch 行导入的完整功能保真 | 支持字段不无声丢失，不以仅保存未执行元数据声称完成 | 未映射 settingsConfig 键经档案片段导入（2026-09-14）；扩展族键仍按所有权拒绝并具名提示；源排序、故障转移队列与代理策略导入已接（2026-09-14）；provider_endpoints 运行时候选（替换 meta 派生候选）与 icon/icon_color/category/created_at 展示列映射已接（2026-09-14，展示列入应用侧 `display` 元数据，不参与路由指纹与客户端投影）；来源 `mcp_servers` 表导入扩展库已接（2026-09-15，`claude_mcp_source`，只读扫描 + 双修订确认，不部署） |
| C18 | UI 不变与可视化通用配置保留 | 前端组件/样式 diff 审查、类型检查、相关测试及实际界面检查 | 对照基线全量 diff 审查：此前所有轮次主界面（App、主供应商页面、样式表）零改动，Claude 新能力全部落在管理弹窗或数据层透传；typecheck 零错误、Claude 切片 85/85、主界面编辑器套件 81/81（3 项旧断言因新增 `claudeFragment`/`display` 载荷字段补齐）、生产构建通过；隔离沙箱（环境重定向）实际渲染验证通过。2026-09-15 经用户明确指令完成唯一一次主界面布局改动：Codex 供应商列表与 Claude 统一为同一工作区骨架（legacy 双行工具条及其样式全删，typecheck、受影响切片 58/58、构建通过）。逐页浏览器走查按用户指示取消，未做 |

## 最终验证

- 核心契约、适配器、切换执行器、存储、网关及协议回归。
- 当前 Claude CLI 的隔离首轮、普通/并行工具、失败工具、图片、推理及跨供应商续轮。
- 接管/停用/退出/重启、状态损坏与文件外改恢复路径。
- 原生 Anthropic 与 Chat/Responses mock upstream 严格校验真实请求；外部服务未测试时如实标记，不能替代为普通文本 fixture。
- `npm run typecheck`、前端相关测试、生产构建、界面及最终 diff。

## 实施记录

- 2026-09-12：建立完整清单；开始请求转换、Chat SSE 与认证/模型契约实现。尚未完成整体目标。

### 当前实施检查点

- C02：独立认证字段已贯通档案、导入、配置投影、网关、模型获取、连接测试与用量查询；保留旧协议默认值对应的路由指纹。凭据控制字符在持久化及出站边界拒绝。新增认证 Rust 回归与前端保真测试，尚需最终整包复验。
- C03：Fable、子 agent、1M 与四档显示名纳入供应商所有权；两个导入入口共用模型解码器，切换缺省及官方恢复会清理旧覆盖。没有更改 Claude 表单布局，字段在原草稿/保存路径保留。网关角色热映射仍属于 C10 待办。
- C06–C09：工具错误标记、Chat 工具图片迁移、所有已支持节点的缓存标记、thinking/budget/effort、空参数/延迟身份/并行 SSE 已实现；Responses 原生 reasoning item 在本机封装后原样回放，禁止将 ASB 信封发送上游。完成相应纯转换回归，真实 CLI 多轮测试已加入，待执行。
- C11：现有重试操作可在保留坏状态原始字节后重建身份，并允许经正常执行器重新应用；合法旧端口保留、不可读目录不改写。停止接管/退出恢复等完整功能仍未实现。
- 用量计数正在统一为含缓存的输入总量，Anthropic 输出再拆分 fresh/read/create，防止重复计数；不等于已完成 C14 请求持久化与计费功能。
- 已执行：核心 285 项、执行器库单元 16 项；一次网关整组 261 通过 / 3 项新 Codex 压缩预算失败 / 9 项按需跳过；54 项 Claude 协议隔离检查；GatewayPage 12 项；认证前端两个文件 60 项；前端 typecheck。后续代码变更仍须复验，数字不作为整体完成证明。
- 2026-09-12 历史集成阻塞（当前已消除）：其他 Codex 任务在系统错误后留下 `ccswitch/codex.rs` 对不存在的 `codex/live.rs` 和 `CodexLiveConfig` 的引用。真实工作区未删除这些声明，未伪造模块。临时验证副本仅排除这两条尚无实现/消费者的声明以验证 Claude；最终整包构建仍必须使用修复后的真实工作区。
- C01、C04–C05 的完整功能、C10、C12–C17 大项仍保留原验收要求，不能以本检查点已完成替代。是否可在原可视化面板增加必要配置项（不改主界面/样式/现有控件）已询问用户，未确认前不新增控件。

### 验证更新

- 2026-09-12：验证副本中 152 项 Rust 专项全部通过（转换、原生推理、用量计数、认证出站及状态修复）。副本与当前 `asb-core/src`、`gateway`、`probe`、`provider_request`、`usage_query` 逐文件比对一致；唯一隔离差异仍为未实现的 Codex live 模块声明及重导出。
- 同一副本用项目固定的 Claude Code `2.1.259` 真正执行 4 项测试，全部通过：原生 Anthropic Bearer 工具往返、Chat 工具失败后纠正、Chat 图片读取结果、Responses 原生推理随工具结果回放。使用完整内置工具声明，执行权限只给 Read；HOME、用户配置、环境与上游全部隔离，不使用真实凭据。
- 当前工作区的 12 个相关前端测试文件共 134 项通过，覆盖现有编辑/参数/官方模式、认证和模型元数据保真、用量及网关页面。可选字段缺省会规范为原表单所需的 null，不改界面结构。
- 新增/重构测试文件与主要实现完成定向 rustfmt、长度检查及 diff 检查。未提交、打包或发布。
- 2026-09-12 历史验证记录：全量 `npm run typecheck` 被其他扩展改动阻挡：`ExtensionsPage.tsx` 仍传入已经从 `SkillSourceActions` 移除的 `onResolveGithub`，且新的 ZIP 选择入口尚未接齐。未用占位回调掩盖该功能缺口。
- 上述是可复现的阶段证据，不是完整完成声明。仍须补齐清单中的接管/热切换、故障转移与熔断、完整提供商扩展/预设、账号绑定和请求历史，并在可构建的真实工作区做最终端到端验收。

## 2026-09-13 当前实施检查点（总体仍未完成）

- 新的 Claude 业务分别位于 `claude_auth/`、`claude_prompts/`、`commands/claude_*`、`gateway/server/claude/` 和 `asb-core/src/claude_presets/`。Codex 仅共享类型、HTTP、原子写入等基础机制，不共享托管账号、Prompt 库或配置文件。接口及本地数据路径见 [Claude 本地后端契约](claude-local-backend.md)。
- C01/C03：离线保留上游 90 个预设的功能数据，不复制组件、样式、推广标识；87 个可准备为通过校验的本地 Claude 档案。Gemini Native 与两个 Bedrock 预设明确不可用。模板变量、独立模型查询地址、账号绑定和 Haiku 的 1M 映射可往返；切回官方会清理模型覆盖。隐藏的 Haiku 1M 标记只在用户主动改换该模型时清除，不破坏普通编辑的保真。
- C02/C15：GitHub Copilot、用于 Claude 的 ChatGPT OAuth、xAI OAuth 使用独立账号文件；设备码登录、轮询间隔/slow_down、取消、重登、默认账号、显式绑定、刷新、模型获取及订阅查询已接通桌面与开发 IPC。账号列表不返回令牌；原生 Codex/Claude 凭据缓存不读写。取消中的登录不会保存迟到令牌，新的默认账号选择不会被旧登录覆盖；刷新成功而落盘失败会暂存在进程内供恢复，退出前需完成保存。
- Claude 网关和既有测试请求均使用请求时解析的账号；测试请求准备后更换默认账号会要求重新准备。ChatGPT OAuth 强制 SSE/store=false，非流式调用会验证终止快照后返回普通 JSON；默认账号变化会隔离不透明推理密钥。Copilot 只做模型 ID 语法规范化，不静默改选其他模型。
- C12/C13：上游 401/403 会依照显式队列重试；请求参数错误不换供应商。请求使用同一策略快照过滤候选；未接管时保存策略会明确提示需重新预览应用。Header/UA 的显式覆盖与认证保护保持独立；模型列表的 modelsUrl 覆盖真实参与执行。托管账号不接受将凭据转投任意完整 URL、自定义端点或模型地址的覆盖。
- C16：新增独立 Claude Prompt 库 CRUD、排序、预览、启用/停用、备份、双修订校验及中断恢复；仅执行器写 CLAUDE.md。编辑活动预设只产生“待应用”，不偷偷改写文档。CC Switch prompts 表以只读事务扫描，导入核对来源快照与库修订，不将来源 enabled 直接当成本机启用。
- UI 约束：没有新增或替换可见控件、修改样式或通用配置可视化结构；只补客户端归属、已有控件的验证与数据保真。新增账号、预设、Prompt 库等独立接口尚未全部接到现有页面操作入口，不能因 IPC 可调用而宣称产品功能已完整对齐。

### 仍需完成

> 2026-09-15 用户决定：真实 Google/云上游验收与订阅/厂商账单核对**移出目标范围**——不再作为完成条件；最终证据以隔离 mock 契约回归与真实 CLI 矩阵 15/15 为准（见第 1、5 项注记）。主工作流直达入口经用户批准接入主界面（同日实施）。

1. Gemini Native、Google OAuth 凭据与 Bedrock/Vertex/Foundry 原生传输已交付（2026-09-14）。其完整媒体/工具/思考路径对**真实 Google/云上游**的验收已按用户决定（2026-09-15）移出目标范围；隔离 mock 契约证据与 CLI 传输用例为准。
2. 上游额外快捷开关/插件能力的逐项核对已完成（2026-09-14）：五个供应商表单快捷开关（`attribution`、`CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS`、`ENABLE_TOOL_SEARCH`、`CLAUDE_CODE_EFFORT_LEVEL=max`、`DISABLE_AUTOUPDATER`）经档案片段导入/投影/撤下回归钉住；插件集成与引导跳过标记已由 `claude_integration` 承载；全局 config_snippets 片段导入已接。`enabledPlugins`/`extraKnownMarketplaces` 按所有权目录仍为「保留不写入」，导入时具名拒绝——这是有意的所有权决定，不再列为缺口。
3. 新增操作入口已接齐（管理弹窗「供应商/认证/网关与 Failover/请求计量/客户端集成」五页 + 通用配置片段导入、候选端点面板、MCP 来源导入、环境冲突）；2026-09-15 经用户批准，主工作流直达入口接入主供应商页面（Claude 工作区头部直达管理弹窗）。交互验收的浏览器逐页走查按用户指示取消（2026-09-15），以组件测试与隔离沙箱渲染验证为准。
4. CC Switch 元数据导入保真已全部接齐（2026-09-14）：源排序、故障转移队列与代理策略、provider_endpoints 运行时候选、settingsConfig 未映射键（档案片段）与 icon/icon_color/category/created_at 展示列（应用侧 display 元数据）均已映射；`display.category` 已在管理弹窗档案选择器中作为来源分组标签展示（2026-09-15，主界面不改），icon/icon_color 仍只随档案往返；真实来源库多版本形态的导入验收待真实数据。
5. **真实服务授权与订阅验收已按用户决定（2026-09-15）移出目标范围**（含订阅/厂商账单核对）。CLI 侧故障矩阵已齐（2026-09-15，真实 CLI 矩阵 15/15）：跨供应商续轮、退出（网关关闭→直连还原）、文件外改恢复、硬崩溃（进程强杀→重启再水化）、端口冲突（识别/拒绝/修复）、运行中手动热切换。项目始终坚持不使用用户真实凭据做自动化验证。
6. 环境变量冲突检测已实现（2026-09-14）；Windows 注册表读写与 macOS 钥匙串读取路径只经静态核对，未在自动测试中触碰真实注册表/钥匙串。`hooks` 仍为保留不写入。

## 2026-09-14 第八轮检查点：官方订阅额度、客户端集成标记与环境冲突（总体仍未完成）

- **C15 官方登录订阅额度**：`claude_native_quota`（Claude 专属，独立于托管账号与 Codex 官方额度）只读 Claude CLI 登录缓存（`CLAUDE_CONFIG_DIR`/`~/.claude/.credentials.json`，macOS 先查钥匙串），以 Bearer + `anthropic-beta: oauth-2025-04-20` 查询 `api/oauth/usage`，解析四个已知窗口与未知窗口、`extra_usage`；凭据看似过期仍尝试，401/403 具名要求重新登录；令牌不进返回值/日志/错误。入口在管理弹窗「认证」。
- **C16 客户端集成标记**：`claude_integration` 承载上游「插件集成」（`config.json` `primaryApiKey=any`）与「跳过引导」（`~/.claude.json` `hasCompletedOnboarding=true`）：单键改写、其余内容保留、非对象拒绝；写入经执行器 `execute_rendered`（写入锁、内容+存在性外改校验、独立目录备份、原子替换），预览与提交逐字段一致；策略 `pluginIntegration` 开启后 Claude 切换提交时按新路由对齐插件标记（失败仅作为切换警告）。入口在管理弹窗新页「客户端集成」。
- **C16 环境变量冲突**：`claude_env_conflicts` 扫描 `ANTHROPIC*`（Windows 注册表 HKCU/HKLM Environment；Unix 进程环境与 `.bashrc/.bash_profile/.zshrc/.zprofile/.profile//etc/profile//etc/bashrc`），返回值经统一脱敏；删除需回显扫描修订、先写完整备份到 `state/backups/claude-env/`，逐条校验目标行仍定义该变量；恢复只接受该目录下的备份名。入口在「客户端集成」页。
- **C04 快捷开关逐项核对**：新增回归钉住上游五个供应商表单快捷开关经档案片段导入、投影与撤下。
- **C16 Windows MCP 启动器包装**：对齐上游 `wrap_command_for_windows`——Windows 主机的 Claude MCP stdio 投影把 `npx`/`npm`/`yarn`/`pnpm`/`node`/`bun`/`deno`（含 `.cmd`、大小写与完整路径）写成 `cmd /c <launcher> …`，从 `~/.claude.json` 导入/接管时还原为可移植形态，连接检测在 Windows 同样包装；扩展库始终保存可移植形态，Codex 投影不变。前端 MCP 预设不再自行按 UA 包装，统一由后端渲染承担（`asb_core::extensions::mcp::claude_launcher` 单一所有者）。
- **C14 CLI 会话用量账本**：对齐上游 `session_usage` 的 Claude 路径——`claude_session_usage` 增量扫描会话 JSONL（mtime + 字节偏移 + 末尾指纹游标，外改钉住不重放），按消息 id 去重并以本地价格表估参考价，与网关账本按 10 分钟窗口 + 四类 token + 模型匹配标记「已匹配网关」而不是写入同一账本（本项目坚持三种用量来源分列）；重建先备份。入口在「请求计量」页。
- **C13 候选端点面板与批量测速**：对齐上游 `speedtest` 与端点管理弹窗——管理弹窗「档案连接」下新增候选端点面板（列出主地址与候选、添加/删除走既有 `provider_endpoints` 修订守卫命令、当前生效档案拒绝改动），`test_claude_endpoints` 复用无凭据可达性探测按输入顺序并发（≤6）返回延迟/状态/失败原因，一次最多 32 个地址。主供应商界面零改动。
- **C16/C17 CC Switch MCP 表导入**：`claude_mcp_source` 只读扫描来源 `mcp_servers`（≤500 行），剥离上游 UI 键（enabled/source/id/name/description/tags/homepage/docs，兼容 `server` 嵌套形态）后经 `import_claude_server` 转成扩展库定义（Windows `cmd /c` 形态自动还原为可移植形态），description/homepage/docs/tags 进 `mcpMetadata`；`enabled_claude` 只作提示；同名已有定义内容相同计已有、内容不同具名拒绝不改名；来源修订双校验；只写扩展库，部署仍走扩展工作区预览。入口在「客户端集成」页。

### 本轮验证

- 第八轮（2026-09-14/15）新增代码此前从未在主树执行过完整测试（实现期间命令通道不可用；此后各轮全量数字来自各自隔离工作树或更早时点，不覆盖本轮文件）。2026-09-15 首次主树全量执行暴露并修复两处第八轮测试夹具缺陷：`claude_native_quota` 的「Debug 不含 tok」断言自始不可能通过（字段名 `access_token` 本身含 tok，改用唯一令牌值断言）；`claude_session_usage` 的 sonnet-5 期望价按第七轮定价前的旧价书写（价格所有者是内置 v2 seed，期望更新为 0.000629）。[claude-round8-verification.md](claude-round8-verification.md) 的待执行清单至此关闭；第八轮 Rust 文件同日补齐 rustfmt（Codex 域文件的既有漂移归并行会话处置）。
- 以下为第七轮及更早的已执行记录：
- 当前真实工作区核心测试已通过（不再使用排除 Codex 模块的验证副本），执行器全套测试通过。
- Claude 专项与新增前端/API/控件保真测试通过；五项真正的 Claude Code 2.1.259 隔离 CLI 用例通过。最终计数以本轮结束前的测试结果和 progress.md 记录为准。
- 全库后端有一条独立复现的 Codex 失败：`commands::switching::codex_policy::tests::taken_native_codex_can_restore_its_backup_and_participate_in_port_change`，恢复阶段返回 `codex-official-login-required`（API-key 登录模式）。未改动其认证/恢复业务，不能宣称全库回归全绿。
- 没有使用真实配置、凭据或账号做写入测试，没有提交、打包发布或部署。

最终实测记录：核心 327、执行器 76、Claude 后端专项 122、前端相关 145、隔离 CLI 5 项通过；typecheck、前端 build、Rust 非测试 check 通过。全库后端 905 通过 / 1 个上述 Codex 失败 / 14 默认跳过。新增 Claude 业务与测试遵守 500 行文件 / 80 行函数约束，样式目录相对本轮起点无差异。

## 2026-09-14 补充检查点：全局片段导入与故障转移队列导入（总体仍未完成）

- **C04 全局 config_snippets 导入**：`claude_snippets`（Claude 专属，Codex 不读）只读扫描来源 `settings.common_config_claude`，经 `claude_common::import_shared` 拆分为可视化偏好与自由 extra，凭据/扩展族/ASB 键具名拒绝；确认导入按 `expectedSettingsHash` 双修订校验写入应用内客户端偏好，不触真实客户端文件。入口在 Claude 管理弹窗「通用配置片段导入」。
- **C17 故障转移队列与代理策略导入**：`ccswitch_source/claude_failover`（Claude 专属文件）只读读取来源 `providers`（app_type=claude，按 `in_failover_queue` + `sort_index` 排序）与 `proxy_config` claude 行；队列成员经 `config_store::find_routing_match`（与导入去重同一路由身份规则，规则唯一所有者仍是档案存储）映射本地档案，未匹配成员具名提示；auto_failover/enabled(接管)/重试/超时/熔断映射为 `ClaudeFailoverPolicy`（错误率 0..1 → 百分比），越界字段保留本地值并具名提示；确认导入走既有 validate/保存/候选刷新路径与写入闸门，双修订（来源 revision + 本地策略哈希）防串写。入口在管理弹窗「网关与 Failover」。来源 provider_health 与 is_current 不导入：健康由本机实时计算，当前路由来自真实客户端文件。
- 本轮验证：agent-switchboard lib 全量 968 通过 / 1 失败 / 20 忽略，唯一失败为并行会话在途文件 `outbound_proxy::tests::system_mode_refuses_to_loop_through_the_gateway_itself`（本轮域外，未改动其文件）；claude_snippets 6 项、ccswitch_source 17 项（含新增 4 项队列导入用例）、网关 claude_failover 组 29 项定向通过；前端 typecheck 与 ClaudeToolsLauncher 14 项（含队列导入流用例）通过。未使用真实配置/凭据，未提交。

## 2026-09-14 当前实施检查点（总体仍未完成）

- **C05 Gemini Native 与云原生传输收口（2026-09-14）**：复核发现账本记录滞后于代码——Gemini Native 预设（`gemini_native` 协议）、Bedrock AKSK/API Key 预设（`claude_native` 原生 SDK 凭据）与 Vertex/Foundry 手动配置均可准备/切换，转换、路由、凭据（API key、结构化 Google OAuth JSON、`ya29.`）与 CLI 隔离用例此前已落地。本轮补齐最后缺口：网关、真实请求与模型获取三条出站在 Google OAuth（Bearer+Gemini）路径附 `x-goog-api-client: GeminiCLI/1.0`（上游同形，API key 路径不附）；新增 Gemini Native 预设准备→网关投影的契约测试（客户端只见 loopback 地址与能力令牌，Google 密钥不出现在客户端文件）；订正 local-backend 文档中已过时的「预设不可用」描述。真实 Google/云上游验收仍待用户凭据。
- **C04/C17 档案级附加配置片段**：`ProviderProfile.claudeFragment`（Claude 专属，Codex 拒绝）承载上游按供应商 settingsConfig 的任意自由字段；投影走第二个所有权清单 `env.ASB_CLAUDE_PROFILE_KEYS`，切换/停用整段撤下并清理空容器；与全局通用配置重叠路径在预览与提交两侧拒绝；网关身份重写（端口变更等）携带片段不丢失。校验、导入过滤、双清单投影的唯一所有者是 `asb-core/src/claude_common/`。
- **C17 导入保真**：cc-switch 行、90 个离线预设、本地发现导入共用 `claude_common::import_fragment`；未映射键（permissions/statusLine/includeCoAuthoredBy/额外 env 等）进片段，env 标量转字符串，扩展族/ASB 键仍具名拒绝；Kimi For Coding 与 gpt-5.6 ChatGPT Codex 路由导入时补写校准上下文窗口默认值（显式优先）。
- **C03/C07**：effortLevel 可视化档位补 `max`（对齐上游 `CLAUDE_CODE_EFFORT_LEVEL=max` 快捷开关的能力值）。
- **C18**：片段编辑入口在 Claude 管理弹窗的档案连接表单（与请求头/体覆盖同区），主供应商界面零改动；主编辑器 draft 往返保留片段；组件不出现配置文件名字面量。
- 顺带修复 22a1f47 两处遗留：`claude_gemini/request.rs` 的 `remember` 调用无法编译；`asb-switch` 恢复临时文件回读用例按恢复事务契约（restore-write + 预恢复回滚）对齐，未改恢复业务。
- 本轮验证：asb-core 347、asb-switch 全部测试二进制（含新增执行器级片段往返）、agent-switchboard lib 952/0/20 忽略、typecheck、前端本域 6 文件 95 项、生产构建、定向 rustfmt。真实 CLI 隔离用例本轮未运行（不涉及请求路径）。全量前端 vitest 受并行 Codex 会话在途改动影响存在波动失败（其域，未代改）。未提交。

## 2026-09-15 真实 CLI 验收矩阵检查点（总体仍未完成）

按验收要求以固定 Claude Code 2.1.259 完成真实 CLI 验收矩阵，全部使用隔离临时目录、假凭据与 loopback 上游，不触碰真实配置：

- **跨供应商续轮（C09/C12）**：`recovery::actual_claude_continues_the_turn_on_a_fallback_provider_after_the_primary_fails`——同一 CLI 调用内工具循环，主供应商（Chat 协议）第 1 请求交付工具调用、第 2 请求注入 503，网关按 `ClaudeFailoverPolicy` 切到备用（Anthropic 原生）接续；断言主 2 请求/备 1 请求、工具结果随续轮到达备用上游、任一上游凭据零泄漏。通过。
- **退出与外改恢复（C10/C11）**：`recovery::actual_claude_survives_gateway_exit_and_externally_edited_client_files`——三段真实 CLI 运行：接管态经网关完成；direct `SwitchPlan` 还原（含凭据归还客户端）后网关关闭，CLI 直连完成；外改新增 host 键后再投影保留该键，CLI 再次完成；末尾断言三次上游命中且未被接管残留污染。通过。
- **矩阵总量**：`cargo test -p agent-switchboard --lib actual_claude -- --ignored --test-threads=1` → **15 通过 / 0 失败**（既有 10 项 + 跨供应商续轮 + 退出/外改恢复 + 硬崩溃 + 端口冲突 + 运行中热切换）。
- **硬崩溃恢复（C11，2026-09-15）**：`resilience::actual_claude_recovers_when_the_gateway_process_dies_under_a_taken_over_client`——接管态 CLI 经网关完成后直接停掉监听 socket（等价进程死亡：无优雅退出、不改客户端文件），断言 CLI 此刻无法完成且上游零命中；随后以全新 `GatewayController::start` 重启，断言同址重绑、持久路由对未动过的客户端文件再水化（观察态 Running + Claude 路由在位），CLI 第三次运行完成。客户端文件逐字节未变，上游凭据零泄漏。
- **端口冲突（C11，2026-09-15）**：`resilience::actual_claude_reports_and_repairs_a_conflicted_gateway_port_for_real_clients`——保存端口被无关 socket 占用后重启网关：观察态 `PortConflict`、失败报告识别 `PortInUse` 与占用进程 PID（本测试进程）、监听宕机期间 `project_with_candidates` 拒绝生成网关依赖的客户端写入；占用退出后 `retry_bind` 复用原址并恢复 Running，CLI 完成，客户端文件逐字节未变。
- **运行中手动热切换（C10，2026-09-15）**：`resilience::actual_claude_hot_switch_moves_the_served_provider_while_the_client_stays_taken_over`——同一活跃控制器、同一 CLI 安装：切换前轮命中供应商 A（恰 1 请求）、`project_with_candidates` + 提交把路由切到 B 后的运行命中 B（恰 1 请求）；客户端文件始终只含 loopback 地址与能力令牌，切换前后任一上游凭据零泄漏。
- **测试设施**：runner 新增 `run_with_deadline`（到时杀进程并按非成功 Output 返回，不 panic），仅用于"客户端预期无法完成"的死网关轮；既有 12 项的超时诊断行为不变。接管夹具把客户端文件写在 `local.target(Claude)` 解析地址，使重启控制器的路由再水化与隔离 CLI 读到同一份文件。
- **第八轮遗留测试债同日清偿**：主树首次全量 lib 运行（1029 通过 / 0 失败 / 22 忽略）暴露两处第八轮测试夹具缺陷并修复——`claude_native_quota` 的 Debug 断言改为唯一令牌值（原「不含 tok」被字段名 `access_token` 自身命中，自始不可通过）；`claude_session_usage` 的 sonnet-5 期望价从 v2 seed 前旧价 0.000944 更新为 0.000629（价格唯一所有者是内置 seed）。第八轮 Claude 域 Rust 文件补齐 rustfmt；Codex 域仍存 37 处既有漂移，归并行会话处置。
- **域外披露**：并行会话遗留 15 小时的 `ccswitch_source/codex_failover.rs` 尾表达式借用错误（E0597）阻塞全库编译，按 rustc 建议做了机械修复（尾表达式绑定局部变量，语义不变）；该文件属 Codex 域，仅解阻塞，不承载任何 Claude 逻辑。首轮全量中 `config_store::codex_endpoints` 一项失败，经并行会话第十一轮证实为其第十轮引入的 route_mode 重算缺陷、已由其自行修复（本轮首跑恰在其修复落地前），未代改。
- 定位记录：恢复用例首跑失败是测试夹具自身缺陷（上游 fixture 对所有请求返回同一静态文本，而三段断言各期待不同短语），修复为统一 `recovery fixture ok` 应答——三段运行各自的端到端语义由「每段 CLI 成功 + 末尾上游命中计数 = 3」承载。
- 未覆盖仍如实保留：硬崩溃（进程强杀）注入、端口冲突故障矩阵、运行中手动热切换 CLI 验收、真实 Google/云上游与订阅/厂商账单（待用户凭据）、主工作流直达入口（待用户确认）。未提交。
