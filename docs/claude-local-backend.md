# Claude 本地后端契约

状态：实施中。保持既有 Claude 页面、控件和可视化通用配置设计；新接口可调用不等于页面入口已经全部交付。完整差距见 [验收清单](claude-ccswitch-parity.md)。

## 所有权

| 内容 | 唯一所有者 / 存储位置 |
| --- | --- |
| 档案 | `configuration/providers/claude/`，现有配置存储 |
| 档案附加配置片段 | `providers/claude/{id}.json` 的 `claudeFragment` 字段 + `claude_common`（校验/导入过滤/双清单投影） |
| 可视化客户端偏好 | `configuration/client-settings/claude.json`，现有可视化设置目录 |
| 托管账号 | `claude_auth/`，`state/claude-accounts.json` |
| Prompt 库 | `claude_prompts/`，`state/configuration/claude-prompts.json` |
| Prompt 中断事务 | `state/configuration/claude-prompts.pending.json` |
| 流量/接管策略 | `gateway/failover.rs`、`claude_settings.rs`，`state/claude-failover.json` |
| 健康与本地价格 | `state/claude-health.json`、`state/claude-pricing.json`（用户改价；内置参考价在 `gateway/claude_pricing_seed.rs`，随应用更新） |
| CC Switch 片段与队列来源扫描 | `claude_snippets/`、`ccswitch_source/claude_failover/`（只读来源扫描；真实客户端文件不参与） |
| 客户端集成标记 | `claude_integration/`：策略 `state/claude-integration.json`；写入目标为 Claude 自己的 `config.json`（`primaryApiKey`）与 `~/.claude.json`（`hasCompletedOnboarding`），备份在 `state/backups/claude-integration/` |
| 官方登录订阅额度 | `claude_native_quota/`：只读 `.credentials.json`（macOS 优先钥匙串），不落盘、不刷新 |
| 环境变量冲突 | `claude_env_conflicts/`：只读扫描系统环境/shell 启动文件；备份在 `state/backups/claude-env/` |
| CLI 会话用量账本 | `claude_session_usage/`：`state/claude-session-usage.json`（条目 + 每文件游标），重建备份在 `state/backups/claude-session-usage/` |
| CC Switch MCP 表导入 | `claude_mcp_source/`：只读扫描 `mcp_servers`，写入扩展库定义（`state/extensions/definitions/`），不部署 |
| 真正的客户端写入 | `asb-switch` 执行器；Claude 配置和 CLAUDE.md 都由后端解析目标路径 |

账号文件包含敏感凭据，应按本地供应商密钥文件保护，不提交到版本库、不写入日志。这里只管理 **Claude 上游**的账号：`codex_oauth` 表示 Claude 使用 ChatGPT 上游，不表示读写 Codex 客户端的账号。不会读取或覆盖 `~/.codex/auth.json`、Claude 的 `.credentials.json` 来实现这些托管请求。Claude 原生官方登录仍由原来的独立登录入口负责。

## 供应商与认证

- `src/api/claude-providers.ts`：离线预设列表、准备、克隆与搜索。准备仅产生 `ProviderDraft` 和 warnings，不保存或激活；后续使用现有档案保存和预览切换路径。
- 预设数据固定在 CC Switch `d695a2d77fd9081eafd3e9eedcbf2a97b3410928`。`scripts/import-claude-presets.mjs` 只接受该版本的本地源码；不联网，不复制 UI。`NOTICE.md` 保留数据来源与 MIT 声明。
- 90 项预设全部可准备。Gemini Native 经 `gemini_native` 协议接入网关转换；Bedrock AKSK/API Key 两个预设展开为 `claude_native` 原生 SDK 凭据（`CLAUDE_CODE_USE_BEDROCK` 等，凭据不进 Anthropic 认证头）；Bedrock/Vertex/Foundry 原生模式也可在管理弹窗连接表单手动配置，由 CLI 自带 SDK 认证、不经本机网关。
- **Gemini Native 凭据（2026-09-14）**：`claude_gemini::credential` 接受 API key（`x-goog-api-key`）或结构化 Google OAuth JSON（取 `access_token`，过期/缺失具名拒绝）或 `ya29.` 裸令牌（Bearer）。该协议无法从 settings.json 交付 `x-goog-api-key`，因此此类档案恒经本机网关转换；网关、真实请求与模型获取三条出站在 OAuth（Bearer）路径附 `x-goog-api-client: GeminiCLI/1.0`，API key 路径不附。真实 Google 上游验收待用户凭据。
- `connection.providerType` / `authBinding.authProvider` 只接受 `github_copilot`、`codex_oauth`、`xai_oauth`。显式 `accountId` 固定该账号；缺省跟随该服务的本地默认账号；账号缺失不会回退到其他账号或原生 CLI 缓存。
- 托管档案 `apiKey` 为空，由请求时解析的账号提供 Bearer；ChatGPT/xAI 必须使用 Responses。账号返回的服务端点具有权威性，因此托管模式拒绝 `isFullUrl`、`customEndpoints`、`claudeModelsUrl` 覆盖。Header/UA/body 的有效请求覆盖仍由既有机制执行，认证头不能被覆盖。
- 普通档案支持 `claudeModelsUrl` 完整模型查询 URL，导入上游 `modelsUrl` 后不只是保存元数据。Bearer 与 x-api-key 的选择独立于 body 协议。
- **档案附加配置片段（2026-09-14）**：每个 Claude 档案可携带 `claudeFragment`（settings.json 自由字段，等价上游按供应商 settingsConfig）。片段只能包含结构化档案字段、可视化偏好与扩展模块都不拥有的键；经 `env.ASB_CLAUDE_PROFILE_KEYS` 清单投影，切换/停用整段撤下并清理空容器，与全局通用配置重叠的路径拒绝。cc-switch 行、离线预设与本地发现导入共用 `claude_common::import_fragment`：未映射键进片段、env 标量转字符串、扩展族/ASB 键具名拒绝；Kimi For Coding 与 gpt-5.6 ChatGPT Codex 路由补写校准的上下文窗口默认值（显式优先）。编辑入口在 Claude 管理弹窗的档案连接表单；主编辑器只保证往返保留，空片段按后端 wire 形态省略。
- **全局通用配置片段导入（2026-09-14）**：`claude_snippets` 只读扫描 CC Switch `settings.common_config_claude`，经 `claude_common::import_shared` 拆为可视化偏好与自由 extra 后按双修订确认写入应用内偏好；凭据与扩展族键具名拒绝。入口在管理弹窗「通用配置片段导入」。
- **展示列映射（2026-09-14）**：CC Switch `providers` 表的 `icon`/`icon_color`/`category`/`created_at` 与离线预设 `category` 经 `ccswitch` 行映射进应用侧 `ProviderDisplay` 元数据（`ProviderDraft.display`，`ProviderProfile`/`ProviderFile` 同形往返）。展示列永不写入客户端配置、不参与路由指纹与去重、不算触碰活跃配置；旧来源库缺列时按 NULL 容错读取。`category` 只是来源分组标签，本地 official/custom 的唯一所有者仍是 `route_mode`。
- **价格覆盖（2026-09-14）**：`ClaudePriceBook` 内置上游对齐的 199 模型参考价表（官方牌价，非账单），计费仍精确匹配「计价模型」（Request 来源用映射模型，Response 来源优先响应模型）；`claude-pricing.json` 只保存用户改价/新增/删除，缺失条目按内置表补齐，旧 v1 文件（仅 3 模型）加载时一次性补齐并刷新未被用户改过的默认值，v2 文件按原样加载（删除有效）。
- **故障转移队列导入（2026-09-14）**：`ccswitch_source/claude_failover` 只读读取来源队列顺序与代理策略行；队列成员按路由身份（`config_store::find_routing_match`）映射本地档案后，经既有策略保存/候选刷新路径写入 `state/claude-failover.json`。越界策略字段保留本地值并具名提示；来源 provider_health 与 is_current 不导入。入口在管理弹窗「网关与 Failover」。
- 新的测试连接和模型获取契约显式包含客户端：`ProviderRequestTarget.draft.connection.app` 和 `fetch_provider_models.request.app` 必填。所有直接调用方与测试夹具已同步更新，不能靠端点或密钥推断客户端。

## 账号接口

`src/api/claude-accounts.ts` 对应同名 snake_case Tauri 命令与开发 IPC：

- 读取账号、显式保存/导入、设置默认账号、删除账号；写入使用 `expectedFileHash` 和 `confirmWrite`。账号列表不返回 access/refresh token。
- `startClaudeAccountLogin` / `pollClaudeAccountLogin` / `cancelClaudeAccountLogin`：支持服务端间隔、slow_down、过期、拒绝、重新登录和取消。设备码留在后端，仅返回用户码、验证地址、会话 ID 与阶段。
- 登录轮询不占住账号锁等待网络；取消后迟到的授权不会保存。重新登录期间目标账号被改动时拒绝覆盖；更晚的默认账号选择优先于尚未完成的旧登录。
- 令牌刷新失败保留旧文件；刷新成功而保存失败时，新凭据暂留当前进程，修复后重试即可保存，不重复使用已轮换的旧 refresh token。**退出前应完成这次保存**，进程内暂存不是持久备份。
- `getClaudeAccountModels` / `getClaudeAccountQuota`：只用对应的托管账号；订阅额度与网关请求计费、本机会话用量是三种不同来源。未知窗口/未知价格不当作免费或零使用。SuperGrok 只识别明确的 billing 字段路径，不从任意浮点字段猜测额度。

Claude 网关、已有“获取模型”和“真实请求”入口都已使用这些请求时凭据。测试准备绑定了账号和真实目标；准备后更换账号须重新准备。ChatGPT 上游的强制 SSE 会被转换为调用方要求的流式或非流式响应，并验证终止快照。推理续接还绑定账号/上游工作区，不能把另一账号的密文回放出去。Copilot 只规范模型 ID，不静默换成更贵或不同的模型。

## Prompt 库与事务

`src/api/claude-prompts.ts` 提供列表、保存、删除、排序、预览、激活、恢复、来源扫描与导入。

1. 保存预设只更新库。活动预设被编辑后标为 `pendingContent`，不隐式覆盖 CLAUDE.md。
2. 预览返回库修订、文档修订、文档存在性、目标摘要，以及 before/after 文本。激活必须提交同一计划和明确确认；不能从 UI 指定任意客户端路径。
3. 执行器负责文档锁、备份、原子替换、回读和提交失败恢复；应用事务记录协调库状态。中断后只在文档/库与已知前后状态匹配时恢复，未知外改保留现场并返回可操作错误。
4. 停用需要预览后清空当前受管理的 CLAUDE.md，并保留备份；没有活动预设时拒绝清空用户自有文档。删除活动预设前必须先停用。
5. CC Switch prompts 表通过只读事务扫描，只选择 `app_type=claude`。来源修订由本次行快照计算，能发现扫描后的变化；导入同时核对本地库修订，重名异内容不部分导入。来源 enabled 仅作为提示，不能直接授权本机写入。

## 客户端集成标记、官方订阅额度与环境冲突

`src/api/claude-integration.ts` 对应 `get_claude_integration` / `set_claude_integration_policy` / `preview_claude_integration` / `apply_claude_integration`：

1. 两个标记各只改写 Claude 客户端文件中的一个键：`config.json` 的 `primaryApiKey: "any"`（Claude Code 插件 / VS Code 扩展据此不再要求登录）与 `~/.claude.json` 的 `hasCompletedOnboarding: true`（跳过首次引导）。文件其余内容原样保留；非 JSON 对象的文件拒绝改写。
2. 写入走执行器 `execute_rendered`：写入锁、外改校验（内容哈希 + 存在性）、备份、临时文件原子替换、回读。备份放在 `state/backups/claude-integration/`，不出现在配置切换的备份/恢复列表；`~/.claude.json` 与扩展工作区共用同一锁路径。预览与提交必须逐字段一致，否则要求重新预览；无变化的预览不写文件。
3. 策略 `pluginIntegration` 开启后，Claude 切换成功提交时按新路由对齐插件标记（custom → 写入，official → 清除）；这一步在切换事务之外、带自己的锁与备份，失败只作为切换结果警告，不回滚已提交的切换。恢复/撤回不自动对齐，页面可手动预览。
4. `get_claude_native_quota`：读取 Claude CLI 自身登录缓存（`CLAUDE_CONFIG_DIR` 或 `~/.claude/.credentials.json`，macOS 先查钥匙串 `Claude Code-credentials`），以 `Bearer` + `anthropic-beta: oauth-2025-04-20` 查询 `https://api.anthropic.com/api/oauth/usage`。返回 `five_hour`/`seven_day`/`seven_day_opus`/`seven_day_sonnet` 与未知窗口的 `utilization`/`resets_at` 及 `extra_usage`；凭据看似过期仍会尝试（CLI 自行刷新缓存），401/403 提示重新登录。令牌不进入返回值、日志或错误消息；与托管账号额度、网关请求计量和本机会话用量是不同来源。
5. `src/api/claude-env.ts`（`scan_claude_env_conflicts` / `remove_claude_env_conflicts` / `list_claude_env_backups` / `restore_claude_env_backup`）：扫描只读，`ANTHROPIC*` 前缀不区分大小写；Windows 读注册表 `HKEY_CURRENT_USER\Environment` 与 `HKEY_LOCAL_MACHINE\...\Session Manager\Environment`（系统级删除/恢复需管理员权限），其他平台读进程环境（只报告，不可删除）与 shell 启动文件的 `export NAME=value` / `NAME=value` 行。删除必须回显扫描修订并逐项选择；先把完整值写入 `state/backups/claude-env/env-<时间>.json`，再按文件从高行号到低行号删除，删除前核对该行仍定义该变量。恢复只接受该目录下的备份名，文件来源追加 `export NAME='value'`。界面只见脱敏值。
6. `get_claude_session_usage` / `rebuild_claude_session_usage`（`src/api/claude-ledger.ts`）：增量扫描 `~/.claude/projects/**/*.jsonl` 的 `type=assistant` 行，按 `message.id` 去重（带 `stop_reason` 或更大 `output_tokens` 的后写行覆盖先写行），`input_tokens` 记为含两类缓存的总输入以与网关账本同形；无时间戳、无 usage 或零用量的行忽略；未以换行结尾的尾行留待下次。每文件游标含 mtime、字节偏移与末尾 4 KiB 指纹：偏移超出文件或指纹不符视为外部改写，游标钉到当前 EOF 不重放。参考价按本地价格表以默认计费（倍率 1、响应模型）估算，未知模型不计价。与网关请求账本最近 200 条按「±10 分钟、四类 token 完全相等、模型一致」匹配的条目标记 `gatewayMatched`，界面提示不应重复相加；不写入网关账本。重建先把现有账本备份到 `backups/claude-session-usage/` 再清空重扫。这是 Claude 专属实现，不复用 `codex_metering` 的会话同步。
7. `test_claude_endpoints`（`src/api/claude-providers.ts`）：对一组候选地址各做一次无凭据、不读响应体的可达性探测（复用 `probe::probe`，含超时一次重试），并发上限 6、单次最多 32 个地址，结果按输入顺序返回 `{url, result: ProbeResult | null, error}`；空地址或非 http(s) 地址就地具名报错，不发网络请求。管理弹窗「档案连接」下的候选端点面板用它测速，添加/删除仍走 `provider_endpoints` 的修订守卫命令。
8. `scan_claude_mcp_source` / `import_claude_mcp_source`（`src/api/claude-mcp-source.ts`）：只读打开 CC Switch 数据库的 `mcp_servers` 表（缺表具名报错，≤500 行），每行 `server_config` 去掉上游 UI 键（`enabled/source/id/name/description/tags/homepage/docs`，兼容 `server` 嵌套）后经 `asb_core::extensions::mcp::import_claude_server` 转成库定义——与从 `~/.claude.json` 发现导入是同一转换器，Windows `cmd /c <launcher>` 形态自动还原；`description/homepage/docs/tags` 进 `mcpMetadata`；`enabled_claude` 只在扫描结果里提示。导入需回显扫描修订并逐项选择；扩展库已有同名定义时内容相同计「已有」、内容不同具名拒绝且不改名；无法转换的行具名跳过。只写扩展库定义，不创建绑定、不写客户端文件。

## 当前未完成的范围

新增账号管理、预设、Prompt 库、完整请求账本等页面操作入口尚未全部接齐，没有为此增加、替换 Claude 可见控件（档案片段编辑器与片段/队列导入位于 Claude 管理弹窗，主界面零改动）。Gemini Native 与云原生（Bedrock/Vertex/Foundry）传输已交付（2026-09-14），真实 Google/云上游与完整跨供应商/退出崩溃矩阵仍在验收清单中；CC Switch 侧元数据（端点候选、排序、展示列）已全部映射，display 元数据尚无可见控件消费。真实外部授权和订阅尚未用用户账号验证；自动测试只使用隔离目录、假凭据和本机上游。
