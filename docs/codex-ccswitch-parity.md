# Codex / CC Switch 完整目标对齐清单

> **这是完整验收账本，不是完成报告。** 修复若干 bug、存在相似模块或旧文档声称完成，均不能关闭本清单。

## 权威范围与证据口径
- 用户目标原文：“软件的codex相关ui不要变，其他的功能等，把ccswitch有关codex的复刻进来实现直到完整实现。保留我们的可视化配置通用文件的特色设计。”
- CC Switch 唯一基线：`farion1231/cc-switch@d695a2d77fd9081eafd3e9eedcbf2a97b3410928`；参考目录 `C:/Users/28308/AppData/Local/Temp/asb-ccswitch-review-20260912`，已核 HEAD 与干净工作树；下文 CC 链接全部固定该提交。
- ASB：`D:/Project/agent-switchboard`，2026-09-12 静态读取，HEAD `147353e5128722e890975cc399bda07cfc43c24e` **加当时脏工作区**；并行修复仍在变化，不将 HEAD 当作全部现状，也不根据 README/DESIGN/progress 的完成描述判定功能。
- 状态：**待验收**＝已找到实现证据但未运行；**部分**＝已有子能力但仍有明确缺口；**待实现**＝已核入口/契约没有该能力或明确拒绝；**待核查**＝尚未追完调用链/语义，不能视为齐备；**进行中**＝用户明确指定的在修事项，仍未验收。
- 本次只读两份源码并编辑本文件；没有运行应用、测试、构建或账户请求，没有读取个人配置或操作真实凭据。测试文件/测试名仅为静态证据，不表示本次通过。
- 下表路径缩写：`T/`＝`src-tauri/src/`，`C/`＝`crates/asb-core/src/`，`X/`＝`crates/asb-switch/src/`，均相对 ASB 根目录；“邻近”不是已存在的功能入口。
- **UI 硬约束**：保留 `src/pages/CodexProvidersPage.tsx`、`src/components/codex-provider-editor/`、共享工作区壳与 `src/styles/` 的现有结构/样式；只对齐功能与行为，不移植 CC 的页面、卡片、Auth Center 或原始配置编辑器。缺交互仍记缺口，不以改版绕过约束。
- **所有权硬约束**：`C/contracts/codex.rs` 拥有第三方路由/目录/能力；`C/ownership/` 拥有可视化字段归属，`T/config_store/client_settings.rs` 拥有客户端通用设置文件；MCP、Skills、全局指令各归原模块。真实配置变更仍经 `X/` 执行器的预览、确认、锁、外改校验、备份与恢复，不复制 CC 的整文件覆盖或第二套配置所有者。
- 根 AGENTS 旧有账号、failover 等禁区不能抵销本次明确扩展的 **Codex** 产品范围；相关差距必须实现/验收。只属于 Claude、Gemini、GrokBuild 等客户端的能力不纳入；共享模块只计算其 Codex 路径。旧输入可在显式导入/迁移边界转换，不新增静默兼容的内部契约。
- 2026-09-12 的分工和只读审计为历史记录；2026-09-14 补充实现按当前脏工作区验证，保留并行 Claude 改动，不据此替其他工作宣布完成。

## 供应商、模型与通用配置
| ID / 用户功能与后端行为 | CC 固定源码 | ASB 当前相应入口 | 状态与证据 / 缺口 | 必要验收 |
|---|---|---|---|---|
| P01 供应商创建、读取、编辑、删除、排序及当前项 | [ProviderService][cc-provider] | `src/pages/CodexProvidersPage.tsx` → `T/commands/profiles.rs`、`T/config_store/codex_providers.rs` | **待验收**：专用 CRUD、全量版本排序、活跃删除保护及 store 单测可见。 | 新建/重命名/重排/删除跨重启一致；陈旧 hash 拒绝；不误删官方记录或另一客户端；活跃编辑走 R03。 |
| P02 复制供应商、元信息与列表查找 | [复制入口][cc-app]、[Provider][cc-meta] | `T/commands/codex_management.rs`、`T/config_store/codex_management.rs`、`src/components/codex-management/ProvidersPane.tsx` | **部分（已补复制/搜索入口并验证）**：独立 ID、完整目录/连接/参数复制、按名称/备注/端点/模型搜索；不激活、不搜索密钥。原 Codex 页面布局不变；分类管理仍未完全对齐。 | 副本独立 ID，模型/能力/查询配置不丢；不会顺带启用或共享可变状态；查找沿用原布局，不照搬 CC 图标皮肤。 |
| P03 全部 Codex 内置供应商预设与配置模板 | [Codex presets][cc-presets]、[template][cc-template] | `C/codex_presets/`、`T/commands/codex_management.rs`、`src/components/codex-management/ProvidersPane.tsx` | **部分**：固定基线的全部 API-key 预设转换与逐项单测、官方档案入口、模板创建界面已接通；xAI OAuth 预设明确不可准备，不能算全量完成。 | 固定基线每个 Codex 预设逐一生成夹具，验证端点、协议、认证、模型、推理档位、媒体能力和用量模板；不能只验五家或默认模型。 |
| P04 模型目录生成、展示名、窗口、模态、推理档位、并行工具及模型指令 | [catalog generation][cc-catalog] | `C/contracts/codex.rs::model_catalog_entry_json`、`src/components/codex-provider-editor/CodexModelSection.tsx` | **部分**：目录/上限/档位有结构化实现；显示名与 base instructions 固定生成，未承载全部 CC 逐模型字段；目录事务见 R04。 | 保存/导入/再编辑不丢逐模型声明；文件目录与 `/models` 一致；Codex 实际显示并能选用；不把获取模型列表当能力证明。 |
| P05 请求模型路由与多模型选择 | [apply_codex_upstream_model][cc-adapter] | `C/contracts/codex.rs::CodexRouteSnapshot::resolve_model`、`T/gateway/server/codex.rs` | **部分**：显式映射已有；Chat/Anthropic 未映射的目录模型仍退到默认模型，与 CC 保留目录内请求模型不同。 | 同一供应商选择两模型，HTTP/WS/compact 均打到对应上游；默认、别名、未知模型规则明确且计量记录真实模型。 |
| P06 获取模型、端点可达性与草稿连接检查 | [model fetch][cc-model-fetch]、[reachability][cc-check] | `src/components/codex-provider-editor/useCodexProviderEditor.ts` → `T/commands/query.rs`、`T/probe/models.rs` | **部分**：草稿获取模型、探测及连接测试已有；CC 批量检查、重试配置及各认证路径未齐。 | 不保存即可检查；取消/旧响应不覆盖新草稿；区分“HTTP 可达”与“推理成功”；错误保留状态码、端点和脱敏诊断。 |
| P07 从 CC 数据导入供应商、目录、认证选择与查询配置 | [provider/live 数据契约][cc-live]、[metadata][cc-meta] | `T/ccswitch_source/`、`T/commands/discovery.rs`、`C/ccswitch/codex.rs` | **已实现，待全量验收**：SQLite 只读扫描、官方/第三方 Codex 一键导入、三类上游、目录字段、认证选择、连接元数据、usage query 导入、重复路由富集与行级未导入警告均有后端实现和隔离夹具。OAuth-only 行可发现并明确告警，但不会把 OAuth token 写入普通 API-key 供应商。 | 覆盖固定基线所有 Codex preset、全部 Meta 变体、重复导入与真实历史数据库；逐项核对未迁移字段理由，确认导入不激活且不携真实 OAuth。 |
| P08 从当前 Codex live 配置导入与首次默认档案发现 | [import_default_config / read_live_settings][cc-live] | `C/discovery/codex.rs`、`C/discovery/inspect.rs`、`C/discovery/import.rs`、`T/commands/discovery.rs` | **已实现，待全量验收**：发现链路会读取并组合 `config.toml`、`auth.json` 与 `model_catalog_json`，生成不含密钥的安全 proposal；导入命令重新读取 live 文件并生成严格 Codex 档案，官方登录单独进入通用官方记录。OAuth-only live 配置仍可展示，但因缺少普通 API key 只告警并拒绝落库。 | 覆盖有效/缺失/损坏 `auth/config/catalog`、官方/第三方 provider、默认能力补全、外部修改和未初始化场景；确认不会制造登录、激活或额外文件。 |
| P09 通用配置抽取、应用/停用、修改后同步 | [common commands][cc-common]、[merge/remove][cc-live] | `C/adapter/codex/common.rs`、`T/codex_common/`、`T/config_store/client_settings.rs`、`src/components/codex-management/CommonPane.tsx` | **部分（本地可视化路径已接通）**：只抽取现有目录的客户端字段，复用唯一通用文件；每档启停、保存与预览应用分离；停用投影为自动值。未知宿主键/资源所有权不扩大，CC 任意片段与全部嵌套字段语义仍未齐。 | 用 ASB 通用文件表达一次配置、多档共享；清除/自动值/数组嵌套及冲突有明确规则；不把通用键固化进供应商；未知/宿主键与注释保留。 |
| P10 切走前 live 回填与所有权剥离 | [strip_common_config_from_live_settings][cc-backfill] | `T/commands/switching/plan.rs`、`C/adapter/codex/overlay.rs`（当前单向投影） | **待实现**：未见与 CC 对等的 live → 原供应商回填闭环。 | 外部修改的自有字段回填原档案；正确回收 provider bearer；不回填 OAuth、网关占位、统一历史桶、MCP/通用键；已存模型目录不被有损 live 清空。 |
| P11 通用供应商生成/同步 Codex 档案 | [UniversalProvider::to_codex_provider][cc-meta]、[sync_universal_to_apps][cc-provider] | 邻近 `T/config_store/codex_providers.rs`、`C/contracts/codex.rs` | **待实现**：未见 universal provider 实体及 Codex 扇出入口。 | 通用连接及 Codex 模型变更只更新所属投影；幂等、保留本端覆盖；不新增 Gemini 等客户端，也不合并 ASB 两套档案契约。 |

## 认证与账号
| ID / 用户功能与后端行为 | CC 固定源码 | ASB 当前相应入口 | 状态与证据 / 缺口 | 必要验收 |
|---|---|---|---|---|
| A01 原生 API key：官方 API 与第三方独立于 ChatGPT 登录 | [plan_codex_live_write][cc-config] | `T/gateway/codex_routing.rs`、`T/codex_auth/policy.rs`、`X/codex_auth/` | **部分（API-key 独立登录已验证）**：Responses 直连与本地网关不依赖 ChatGPT；文件 API-key 模式与 OAuth 明确分开；已用仓库 Codex CLI + 隔离 loopback 验证无 ChatGPT 登录的网关请求。非文件认证仍见 A02。 | 无 OAuth / API-key 模式也能用合法 key；provider 范围 bearer 有效；官方 API-key 与 ChatGPT 账号模式分开；不得将官方 token 发给第三方。 |
| A02 可选保留官方登录与认证存储语义 | [preserve/auth plan][cc-config]、[setting][cc-settings] | `T/codex_auth/policy.rs`、`X/codex_auth.rs`、`X/codex_auth/synchronize.rs` | **部分**：保留开关已实现；不保留时确认切换后清理 OAuth，保留时可回切已有登录。写入、备份、还原及刷新同步都有外改/hash 保护。keyring/auto/ephemeral 明确拒绝写入而非假成功，尚未对齐。 | 隔离夹具验证开/关、缺失/旧/损坏 auth、存储模式与 `requires_openai_auth`；保留/清理必须符合明确选择，不把仅保护 auth 的测试当完整认证实现。 |
| A03 原生官方登录流程、取消、重新登录与官方行 | [device auth commands][cc-auth] | `T/commands/official_login.rs`、`T/official_login/codex.rs`、`src/components/OfficialLoginPanel.tsx` | **待验收**：已有 device flow、pending/过期/取消、写原生凭据与 ensure 官方记录，含假服务单测；仅单份原生登录。 | pending/网络抖动/拒绝/过期/取消均可恢复；成功出现官方行；token 不过 IPC/日志；创建官方行不改已有用户元信息。 |
| A04 托管 OAuth、多账号、默认账号、显式绑定与原生未绑定选择 | [auth_*][cc-auth]、[CodexOAuthManager][cc-oauth]、[apply_codex_official_auth][cc-auth-live] | `T/codex_auth/{contracts,store,bindings,login,manager}.rs`、`T/commands/codex_accounts.rs`、`src/components/codex-management/AccountsPane.tsx` | **部分**：本地多账号、默认/显式/原生未绑定选择、定向重认证、取消、删除与绑定验证已接通；令牌不走 IPC。删除保留原生登录，专门的原生退出事务及跨存储模式仍开放。 | 增删/列举/默认/定向重认证/解绑/退出；显式绑定不随默认漂移；缺失账号明确失败；未绑定官方遵循原生登录，普通供应商只保存引用。 |
| A05 托管身份、刷新竞争、删除与 live token 同步 | [manager lifecycle][cc-oauth]、[managed auth marker][cc-config] | `T/codex_auth/{identity,manager,native_sync,projection}.rs`、`X/codex_auth/synchronize.rs`、`X/restore/` | **部分（文件链路已实现并测试）**：按用户+工作区识别身份；刷新锁/代际、旧预览拒绝、删除不复活、同链 native token 同步及持久重试；保存新 token 后才同步，API-key 模式/外部登录/退出保持。非文件存储与整套迁移仍开放。 | 同 workspace 不同用户不可混同；CLI/管理器并发刷新不降代；重认证保留绑定 ID；旧缺 id_token 要重认证；删除与切换/刷新串行，不复活账号、不覆盖新原生登录。 |
| A06 按托管账号查询 Codex 可用模型 | [get_codex_oauth_models][cc-oauth-query] | `T/codex_auth/{query,quota_history}.rs`、`T/commands/codex_accounts.rs` | **已实现，待真实服务验收**：按显式账号或默认账号快照请求固定 ChatGPT Codex 模型端点，携带版本/账号头；模型与额度结果绑定账号，额度缓存/基线独立于原生。未使用真实账号请求作为验收证据。 | 显式账号优先于默认；使用正确后台端点/版本/账号头；刷新失败可诊断；请求期间切换默认不会将结果归错账号。 |
| A07 xAI OAuth 作为 Codex 原生 Responses 上游 | [CodexAdapter xAI 分支][cc-adapter]、[auth_*][cc-auth] | 邻近 `C/contracts/codex.rs`、`T/gateway/server/route.rs` | **待实现**：仅静态 API key；没有 xAI 账号绑定/刷新与逐请求注入。 | 无 key 的托管卡只能由所绑账号出站；固定 xAI 目标不受可编辑 URL 劫持；过期/失效可重认证；这是 Codex 上游，不引入 GrokBuild 客户端。 |
| A08 Codex → Anthropic 的 Bearer / x-api-key 选择 | [extract_auth][cc-adapter] | `C/validate/codex.rs`、`C/contracts/codex/validate.rs`、`src/components/codex-management/ProviderAuthentication.tsx` | **已实现，待完整三协议运行矩阵**：Codex 独立选择默认/Bearer/x-api-key，严格验证与通用 client projection 已统一；活跃修改走原保存预览事务，非活跃不动 live。未改变原编辑器布局。 | 草稿测试、保存、导入、HTTP/WS 与热切换使用同一显式认证选择；两种认证头互斥且无回退误发。 |

## 直连、接管与事务
| ID / 用户功能与后端行为 | CC 固定源码 | ASB 当前相应入口 | 状态与证据 / 缺口 | 必要验收 |
|---|---|---|---|---|
| R01 原生直连与可选本地接管分离 | [provider switch][cc-provider]、[takeover][cc-proxy] | `T/gateway/routing.rs`、`T/commands/switching/plan.rs`、`C/contracts/codex.rs` | **待实现**：第三方一律网关，不能称与可选接管等价。 | Responses 原生第三方无需 ASB 常驻可用；Chat/Anthropic 仅在转换路径接管；启动代理不等于接管；单独取消 Codex 接管不影响 Claude。 |
| R02 官方/托管 Codex 在接管中仍可显式选择 | [official authorization][cc-forwarder]、[takeover][cc-proxy] | `T/gateway/routing.rs`、`T/commands/switching/plan.rs` | **待实现**：当前官方仅直连，不建官方代理路由。 | 接管官方只到 ChatGPT Codex 后端；核对请求 native Authorization 与绑定账号；旧会话身份不匹配时拒绝；官方账号不得参与自动跨账号 failover。 |
| R03 热切换、活跃档案保存与请求快照 | [hot_switch_provider][cc-hot]、[live sync][cc-live] | `T/commands/switching/codex_profile_save.rs`、`T/gateway/routing.rs`、`T/gateway/server/tests/route_revisions.rs` | **部分**：稳定 capability、修订快照、保存预览/补偿已有；必须结合 R04/R05 收口，不能据此关闭完整路由目标。 | A→B 和同档改连接时新请求取新快照，在途请求不混 key/模型/计量；取消或失败不留下“档案新、live 旧”；陈旧预览拒绝。 |
| R04 备份恢复的供应商身份、修订与模型目录产物 | [restore / backup][cc-proxy]、[catalog][cc-catalog] | `T/commands/switching/{codex_restore_auth.rs,projection_transaction.rs}`、`X/restore/`、既有目录事务 | **部分（新增配对恢复修复已验证）**：认证与配置恢复、撤销恢复保持关联；应用/执行器 journal 的 auth 身份一致，拒绝旧托管代际，回滚不覆盖新登录。完整目录/端口/启动恢复矩阵仍需逐项收口。 | 恢复必须对应备份记录的 ID/修订/目录内容；旧端口可重绑定但不得换供应商；目录丢失/变更、多个档案、重启中断均正确拒绝或恢复；保留外部文件。 |
| R05 官方切换事务、回滚与真实生效状态 | [official live / switch][cc-auth-live]、[provider][cc-provider] | `T/codex_auth/projection.rs`、`T/commands/status/codex.rs`、`T/commands/switching/plan.rs` | **部分**：官方原生与托管绑定接入切换；实际身份参与状态匹配，直连身份不再以全量通用设置是否一致判定。目录/失败补偿测试已有，本轮不宣称所有真实客户端状态矩阵完成。 | 第三方→官方→第三方、失败补偿、启动恢复及状态/托盘一致；auth 与目录按所属路径处理；“使用中”依据实际 live，不依据 UI 选择或旧激活记录。 |
| R06 配置目录覆盖与各子系统路径一致 | [get_codex_config_dir][cc-config]、[state DB paths][cc-state-db] | `T/local_state/paths.rs`、`T/extensions/paths.rs`、`T/session_manager.rs::session_roots` | **部分**：配置/扩展支持 CODEX_HOME；会话/用量根仍硬编码 home/.codex；CC 设置目录覆盖与 sqlite_home 未对齐。 | 两套隔离 Codex 根切换时 auth/config/catalog/MCP/会话/额度均一致；sqlite_home/归档按源规则读取，不扫错目录。 |
| R07 启停、退出恢复、崩溃恢复、端口与可观察状态 | [proxy lifecycle][cc-proxy]、[commands][cc-proxy-command] | `T/gateway/{lifecycle.rs,state.rs,port_change/mod.rs}`、`T/commands/gateway.rs` | **部分**：监听/恢复状态与端口事务已有；CC stop-with-restore、保留接管状态等组合未验齐。 | 监听失败不改 live；显式停用可恢复可用直连；残留占位/缺备份可诊断；重启、改端口失败不丢路由；其他客户端不被顺带恢复。 |
| R08 旧 Codex provider 表/保留 ID/无 name 配置的显式迁移 | [normalize/preflight][cc-config] | `T/gateway/restore_validation.rs::validate_codex_provider`、`C/adapter/codex/mod.rs` | **待实现**：当前拒绝部分旧 provider 契约，尚非 CC 的迁移能力。 | 代表性旧配置先解析、预览转换再写；活动与闲置表都通过 Codex 校验；保留无关自定义表/项目覆盖；不能静默丢 key 或改为官方认证。 |

## 协议、推理与续接
| ID / 用户功能与后端行为 | CC 固定源码 | ASB 当前相应入口 | 状态与证据 / 缺口 | 必要验收 |
|---|---|---|---|---|
| G01 原生 Responses、models、Chat、alpha/search、图像生成/编辑路由 | [server routes][cc-server]、[handlers][cc-handlers] | `T/gateway/server/{codex.rs,route.rs}`、`T/gateway/server/tests/operations.rs` | **待验收**：路由与声明能力门控、目录响应均可见；附属 API 不可被核心 Responses 测试替代。 | 每个操作独立验 method/path/query/auth/body/response；原生 Responses 扩展不误删；models 返回已激活目录；未声明操作明确拒绝。 |
| G02 Responses → Chat 请求与 JSON/SSE 回转 | [Chat transform][cc-chat]、[stream][cc-chat-stream] | `T/gateway/transform/{request_parse/tools.rs,request_render/protocols.rs,stream/chat.rs}` | **部分**：工具/namespace/custom/tool_search 映射与测试可见；全 CC 输入形态未验齐。 | instructions/system/developer、工具选择/并行调用、自由文本参数、搜索后动态工具、名称反解均可多轮使用；finish/error/usage/分片 UTF-8 不丢。 |
| G03 Responses → Anthropic 请求与 JSON/SSE 回转 | [Anthropic transform][cc-anthropic]、[stream][cc-anthropic-stream] | `T/gateway/transform/{request_render/protocols.rs,stream/anthropic/mod.rs}` | **部分**：基础桥接已有；前导 user、历史工具配对、停止原因与所有工具形态待逐项验收。 | system/developer 优先级不降级；工具输出关联不乱；max_tokens/refusal/context-limit 转为正确 incomplete；空/损坏/中断流不得报 completed。 |
| G04 Chat reasoning none 与完整推理方言 | [reasoning options][cc-chat]、[per-model resolution][cc-adapter] | `T/gateway/transform/chat_reasoning.rs`、`T/gateway/transform/request_reasoning_tests.rs`、`C/contracts/codex.rs` | **进行中（Chat 代理）**：none 正在修；ASB 仅四种 effort mode，CC Zen 逐模型档位及输出字段方言未齐/待核查。 | 区分缺省/null/none/off/disabled；thinking、enable_thinking、reasoning_split、平铺/嵌套 effort 不冲突；所有合法档位及逐模型夹具通过，不静默恢复思考。 |
| G05 Anthropic 逐请求模型预算与推理设置 | [budget injection][cc-forwarder]、[thinking transform][cc-anthropic] | `T/gateway/server/codex.rs::apply_anthropic_output_budget`、`T/gateway/transform/anthropic_reasoning.rs` | **进行中（Anthropic 代理）**：请求目录模型预算正在修；不据此宣布 adaptive/extended thinking、sampling 互斥和预算夹紧全部完成。 | HTTP/WS、别名及非默认模型按本次请求模型取预算；显式上限的优先级/越界规则一致；thinking 预算小于 max_tokens；none、ultra 与签名历史组合验收。 |
| G06 跨请求工具历史补全与 previous_response_id | [CodexChatHistoryStore][cc-chat-history] | `T/gateway/server/websocket/context/mod.rs`、`T/gateway/transform/request_parse/protocols/responses/history.rs` | **部分**：WS 连接内续接已有；未见对等 HTTP 跨请求工具缓存/唯一 call_id 回填，未知引用会拒绝。 | HTTP 与 WS 均验证短输入续接、并行工具、缺/改 previous ID、唯一/冲突 call_id、缓存淘汰；不得串会话/账号/修订或丢 reasoning。 |
| G07 compact 与压缩后继续会话 | [compact handler][cc-handlers]、[conversion dispatch][cc-forwarder] | `T/gateway/{compaction/,server/compact.rs}`、`T/gateway/server/tests/{native_compaction.rs,codex_compact_cli.rs}` | **部分**：原生 compact、桥接摘要与 ASB V2 均有代码；尚未用 CC 全矩阵验收。 | `/responses/compact` 三协议 JSON/SSE、错误、使用量及后续工具回合闭环；原生密文透传，桥接摘要真实生成；保留 ASB V2，不用占位摘要冒充压缩。 |
| G08 reasoning 内容、签名与不透明历史回放 | [thinking envelopes][cc-anthropic]、[Chat history][cc-chat] | `T/gateway/transform/reasoning.rs`、`T/gateway/transform/request_reasoning_tests.rs` | **待验收**：签名/redacted/Chat 推理封装与回放测试可见，ASB 按路由修订隔离。 | 文本、reasoning_content/details、签名分片、redacted 多轮不丢；跨账号/后端密文明确拒绝；不得将原生不透明值当普通文本。 |
| G09 严格上游兼容：xAI namespace/schema 与 Moonshot $ref | [targeted rewrites][cc-forwarder] | 邻近 `T/gateway/transform/{tool_names.rs,request_render/}` | **待实现**：通用跨协议名称转换不等于 native xAI 改写；未见对应 xAI/Moonshot 定向策略。 | namespace 展平/回转、xAI 根组合 schema/整数参数/agent_message、Moonshot `$ref` sibling 均用实际参考夹具；不影响其他上游及缓存前缀。 |
| G10 多模态、工具结果媒体与拒图降级 | [media handling][cc-chat]、[prevention/retry][cc-forwarder] | `T/gateway/server/codex.rs`、`T/gateway/transform/request_parse/` | **部分**：图片能力门控和文本/图像转换已有；file/audio/工具结果媒体未核齐，未见 CC 拒图一次降级重试。 | 覆盖文本模型、图片模型、file/audio 与嵌套工具媒体；降级有标记且受设置约束；不损坏 JSON schema，不伪造模型能力。 |
| G11 稳定 prompt cache routing 与显式开关 | [inject_codex_chat_prompt_cache_key][cc-adapter] | `T/gateway/transform/request_parse/fields.rs`、`C/contracts/codex.rs` | **待实现**：未见 auto/enabled/disabled 路由契约和真实 session 派生闭环；仅识别参数不算支持。 | 显式 key 优先、真实 session 次之，无真实 session 不造随机 key；未知上游默认不注入；切换与重试遵守所属账号/路由。 |
| G12 Codex → Anthropic 缓存、1M 与可选请求兼容 | [Codex Anthropic 出站分支][cc-forwarder] | 邻近 `T/gateway/transform/request_render/`、`T/gateway/server/route.rs` | **待实现/待核查**：CC 此分支有缓存 TTL/断点、`[1m]` 清理与 beta、可选 Claude Code UA/system 兼容；ASB 未见完整接线。 | 只在 Codex→Anthropic 生效；5m/1h、断点预算、模型后缀/头、用户覆盖优先级明确；不把 Claude 专属优化器整套搬入。 |

## 网络、失败重试与熔断
| ID / 用户功能与后端行为 | CC 固定源码 | ASB 当前相应入口 | 状态与证据 / 缺口 | 必要验收 |
|---|---|---|---|---|
| N01 出站 HTTP/HTTPS/SOCKS 代理、系统代理、探测与热更新 | [global proxy][cc-global-proxy]、[HTTP client][cc-http-client] | `T/gateway/server/route.rs::UpstreamClient::new`、`T/probe/transport.rs` | **待实现**：reqwest 默认行为不等于全局代理配置；未见对应设置/状态/测试/扫描命令。 | 显式代理与系统代理优先级、socks5/socks5h DNS、带认证代理、无代理及防自环；推理/模型/账号/用量请求一致，失败可恢复且凭据脱敏。 |
| N02 自定义 headers、User-Agent 与本地请求 body overrides | [ProviderMeta][cc-meta]、[final outbound request][cc-forwarder] | `C/contracts/codex.rs`、`T/gateway/server/route.rs::upstream_headers` | **待实现**：只有生成认证和 native 头白名单，无供应商 headers/UA/body 覆盖契约。 | 编辑/导入/预览/出站一致；覆盖优先级明确；不泄露本机 capability、官方账号头或内部字段；body 覆盖不破坏协议转换及真实模型归因。 |
| N03 多端点、测速择优、完整 URL 与查询参数 | [endpoint commands][cc-provider-command]、[URL dispatch][cc-forwarder] | `C/endpoint.rs`、`T/commands/query.rs`、`C/ccswitch/codex.rs::normalize_endpoint` | **部分**：API 根解析/探测已有；无端点集合/择优/last-used；完整操作 URL 与 query 在部分入口被拒绝。 | CC 输入在显式边界归一到 ASB 唯一契约；版本前缀不重复、大小写/query 不丢；附属 API 正确取兄弟路径；择优不会混凭据。 |
| N04 流式首包、空闲、非流式超时与取消 | [AppProxyConfig][cc-proxy-types]、[forwarder][cc-forwarder] | `T/gateway/server/transport.rs::Timeouts`、`T/provider_request/transport.rs` | **部分**：有分阶段固定超时及暂停流测试；缺 CC 可配置项与开关/零值语义对齐。 | 首包/流间隔/完整响应分别计时；设置变更生效；客户端取消释放连接；超时不报成功、不让在途流因普通热切换被截断。 |
| N05 显式 failover 队列、优先级与失败分类重试 | [queue commands][cc-failover]、[router][cc-router]、[retry][cc-forwarder] | `T/gateway/codex/`、`T/commands/switching/codex_policy/`、`src/components/codex-management/GatewayPane.tsx` | **部分（既有后端已补操作入口）**：独立接管/开关/队列/顺序/重试，预览与确认共用执行器；取消与一次性准备一致，前端测试验证提交不被 cleanup 误取消。全矩阵验收仍开放。 | P1→P2 顺序、关闭保留队列、开启先成功切 P1；可重试/客户端错误/中断分类；已发出的流不能重放；官方账号排除，客户端取消不污染健康度。 |
| N06 熔断、健康状态、半开许可与人工重置 | [circuit breaker][cc-circuit]、[health/reset commands][cc-proxy-command] | `T/gateway/codex/health.rs`、`T/gateway/provider_health.rs`、`src/components/codex-management/GatewayPane.tsx` | **待全量验收**：独立健康配置、熔断状态及人工 reset 已接通；非请求计数冒充健康度。本轮新增界面，不替代全部半开/取消/并发许可的参考矩阵。 | 按 Codex+provider 隔离闭合/打开/半开；阈值、冷却、探测成功恢复、手动 reset 生效；失败/取消均释放许可；全熔断给出明确原因。 |
| N07 压缩编码、流式边界与错误诊断 | [content encoding][cc-encoding]、[Codex errors][cc-handlers] | `T/gateway/content_encoding.rs`、`T/gateway/server/tests/transport_encoding.rs`、`T/provider_diagnostics.rs` | **待验收**：gzip/deflate/br/zstd、叠加编码与流式限界实现可见；诊断已有独立类型。 | 请求、JSON响应、SSE及错误体分别验证；raw/zlib deflate、损坏/未知编码/超限不混为成功；保留 HTTP 状态、request ID 和可操作原因，精确脱敏。 |

## 用量与本地会话
| ID / 用户功能与后端行为 | CC 固定源码 | ASB 当前相应入口 | 状态与证据 / 缺口 | 必要验收 |
|---|---|---|---|---|
| U01 余额/套餐、自定义脚本、独立查询凭据与定时缓存 | [queryProviderUsage][cc-provider-command]、[UsageScript][cc-meta] | `T/usage_query/`、`C/ccswitch/usage.rs`、`src/components/UsageQueryWorkspace.tsx` | **部分**：声明式/QuickJS、调度和多种内建模板已有；NewAPI、GitHub、火山控制面等独立凭据模板仍被跳过，不得以旧账号边界排除。 | 逐个 Codex 可用模板/自定义脚本对照；多套餐/无效套餐、超时/重试、禁用、缓存过期/修改后失效；凭据自有与查询覆盖分离，导入即执行须显式确认。 |
| U02 原生与托管账号订阅额度、模型列表之外的账号缓存 | [native quota][cc-subscription]、[managed quota][cc-oauth-query] | `T/codex_official_quota/`、`T/usage_history/`、`src/components/CodexOfficialQuotaPanel.tsx` | **部分**：原生 file 登录额度、窗口历史与重置基线已有；无托管多账号额度；CC macOS Keychain 读取未对齐。 | 5h/7d/免费30d窗口、重置时间、未登录/过期/拒绝/瞬时失败；缓存绑定真实账号而非全局 Codex 标签；默认/绑定切换不串读数，失败保留策略明确。 |
| U03 持久请求用量、请求明细与真实计价模型 | [UsageLogger][cc-usage-log]、[request commands][cc-usage] | `T/codex_metering/`、`T/gateway/metrics/codex.rs`、`src/components/codex-management/MeteringPane.tsx` | **部分**：独立持久请求账本与分页/筛选/汇总入口已有；真实模型、token/cache/尝试与费用不保存正文/密钥。会话增量账本及跨源去重未完成，界面明确不与会话统计相加。 | 每次请求仅落一次，含时间/来源/供应商/请求与实际模型/input/output/cache/状态/延迟/首 token；重启保留，流尾/错误/重复 ID 不漏不重；不保存密钥和原始会话正文。 |
| U04 Codex JSONL 增量计量、去重、归档与重建 | [sync_codex_usage][cc-session-usage]、[rebuild_codex_usage][cc-usage] | `T/model_usage/{mod.rs,parse.rs,tests.rs}`、`T/model_usage_cache.rs` | **部分**：只读全量 token 聚合/缓存与按 session 去重已有；无持久 offset、跨来源去重及重建命令。 | 精确 last usage 与累计 delta、revert/resume 双 UUID、归档移动、mtime不变追加/截断、fork/重放不重复；proxy/session 不双算；重建先备份且只清 Codex 来源。 |
| U05 用量筛选、趋势/汇总、定价同步/回填、成本倍率与日/月限额 | [usage/pricing/limits][cc-usage]、[provider cost fields][cc-meta] | `T/codex_metering/{pricing,settings,budget,query}.rs`、`src/components/codex-management/{MeteringPane,PriceEditor}.tsx` | **部分**：手工本地定价、倍率、日/月预算、按已应用筛选回填和请求汇总已接通；未知价显示未计价。models.dev 同步及完整会话计量口径未完成。 | 日期/供应商/模型/来源过滤与分页统计同账；cache-inclusive 口径不双扣；缺价不伪装已知零价；价格更新可重算，限额查询与参考调用链一致。 |
| S01 会话列表、标题、正文、目录与项目分组 | [Codex scanner][cc-session]、[session UI][cc-session-ui] | `T/session_manager/{parser.rs,resume.rs}`、`T/session_manager.rs`、`src/components/SessionManager.tsx` | **部分**：两类 JSONL 根、去重、搜索及提问目录已有；未读 session_index/state DB 标题；工具调用/输出、子代理过滤和分组语义未齐。 | 与 CC 同夹具比较自定义标题/首问、IDE包装清洗、时间、cwd、工具消息和子代理；索引/SQLite锁定可降级；不复制 CC 分组 UI。 |
| S02 会话恢复、复制与单个/批量删除 | [session commands][cc-session-command] | `T/session_manager.rs::{resume_session,delete_session}`、`src/components/SessionManager.tsx` | **部分**：后端解析 session ID、恢复终端及单删已有；未见批量删除结果契约。 | 正确 cwd 与跨平台引用；启动失败保留可复制命令；单删/批删逐项结果；不能删源根外文件或误删同名其他会话；删除需明确确认。 |
| S03 跨供应商统一历史、迁移已有历史及还原 | [history migration][cc-history]、[history setting][cc-settings-command] | 邻近 `T/session_manager.rs`、`T/commands/switching/backups.rs` | **待实现**：固定 openai provider 与浏览历史不等于统一历史迁移；未见开关、ledger 与还原命令。 | 只转换可信旧 provider/官方记录的 JSONL 与 state DB；每个目录独立标记/备份；开关重投影不污染档案；还原仅恢复迁移对象、不覆盖之后新增会话。 |

## MCP、Skills、Prompts 与项目编排
| ID / 用户功能与后端行为 | CC 固定源码 | ASB 当前相应入口 | 状态与证据 / 缺口 | 必要验收 |
|---|---|---|---|---|
| E01 Codex MCP CRUD、导入、启停与字段投影 | [Codex MCP][cc-mcp]、[MCP service][cc-mcp-service] | `T/commands/extensions/`、`C/extensions/mcp/{import.rs,codex_patch.rs}`、`src/components/extensions/McpEditForm.tsx` | **待验收**：条目级补丁、stdio/URL/headers/env/cwd/超时、启停和发现导入已有代码与 MCP 测试。 | JSON↔TOML 各可用字段往返、密钥槽/环境头正确；未知字段保留；同 ID 导入不覆盖其他客户端；目录缺失/坏配置/外改均有确定结果。 |
| E02 MCP 独立所有权与供应商切换后的同步 | [sync_enabled_for_app][cc-mcp-service]、[剥离防复活][cc-backfill] | `C/adapter/codex/overlay.rs`、`C/extensions/mcp/codex_patch.rs`、`X/extensions/` | **部分**：ASB 已按资源分权，不整份替换 MCP；与未来 live 回填及统一历史开关的闭环仍须验收。 | 删除/禁用 MCP 后切换供应商、恢复备份不复活旧条目；用户未托管条目不被删除；通用配置与扩展执行器不争写同一字段。 |
| E03 Skill 来源、发现/搜索、导入与更新检查 | [skill commands][cc-skill]、[source services][cc-skill-service] | `T/extensions/sources/`、`T/commands/extensions/{sources.rs,skills/mod.rs}`、`src/components/extensions/SkillSourceBrowser.tsx` | **部分**：本地/GitHub候选、固定commit、更新diff已有；CC 多仓库聚合/skills.sh搜索未齐；ZIP入口等**待核查**。 | 单/多Skill、仓库分支/子目录/ZIP、来源启停、更新差异与内容hash正确；导入不执行脚本；异常归档/符号链接不越界，不把未更新报最新。 |
| E04 Skill 按客户端安装/移除、同步模式、备份恢复与存储迁移 | [sync/backup/storage][cc-skill-service] | `T/commands/extensions/planner/`、`X/extensions/`、`T/extensions/paths.rs` | **部分**：ASB 内容版本、复制部署、撤销/恢复已有；无 CC Auto/Symlink/Copy 与存储位置迁移设置。 | 开关/卸载/更新/恢复闭环，外改先保护；路径按 Codex 实际读取验证，不强搬 CC 内部目录；保留 ASB 内容库所有权，迁移原子且不覆盖外部技能。 |
| E05 Prompt 多预设 CRUD/互斥启用/live回填与 AGENTS.md | [PromptService][cc-prompt]、[Codex path][cc-prompt-path] | `T/codex_prompts/`、`T/commands/codex_prompts.rs`、`X/prompt_documents.rs`、`src/components/codex-management/PromptsPane.tsx` | **已实现，待更广工作流验收**：独立 Codex 命名预设 CRUD、互斥启停、live（含空内容）回填、首次原文保留、未启用编辑不写 live、活动编辑确认、备份/外改保护及崩溃/遗留锁恢复均有隔离测试；原全局指令编辑器保留。 | 预设可建改删/导入；启用前保存 live 外改，未激活编辑不动 live；全局文档归原执行器，不写仓库 AGENTS.md，不因切供应商覆盖指令。 |
| E06 Codex 项目 Profile 快照及应用 | [ProfileScope::Codex][cc-profile] | 邻近 `T/commands/extensions/workspace.rs`、`T/config_store/snapshot/` | **待实现**：登记项目/配置快照不等于供应商+MCP+Skill+Prompt 联动 Profile。 | 创建/更新快照/改名/删除/清当前/应用；None 与空集严格区分；仅作用 Codex 分组，复用各所有者，逐项失败报告可操作且不影响 Claude。 |

## Codex 适用的共享管理能力
| ID / 用户功能与后端行为 | CC 固定源码 | ASB 当前相应入口 | 状态与证据 / 缺口 | 必要验收 |
|---|---|---|---|---|
| O01 配置导入/导出、备份管理与恢复后的 Codex 投影 | [import/export/backups][cc-backups] | `T/config_store/snapshot/`、`T/commands/switching/backups.rs`、`src/pages/BackupsPage.tsx` | **部分/待核查**：配置快照及客户端文件恢复已有；整库便携导入导出、备份改名/保留策略与新增账号数据覆盖尚未验齐。 | 导出再导入恢复 Codex 全部自有数据；恢复前备份、版本校验、外改保护；投影失败可恢复；扩展/账号数据不遗漏也不误携其他客户端原始凭据。 |
| O02 WebDAV/S3 同步中的 Codex 数据与状态 | [WebDAV][cc-webdav]、[S3][cc-s3] | `T/commands/cloud_backup.rs`、`T/cloud_backup/` | **待实现**：ASB Supabase 加密备份不是 WebDAV/S3 双向同步；不能以根 AGENTS 旧禁区排除。 | 连接测试、远端信息、上传/下载、设置/自动同步、冲突/失败状态与恢复投影可验；数据范围和凭据策略明确，仍以 ASB 文件为唯一所有者。 |
| O03 Codex OPENAI 环境冲突诊断、清理备份与恢复 | [env conflicts][cc-env]、[env commands][cc-env-command] | 邻近 `T/commands/status/overview.rs`、`src/pages/DiagnosticsPage.tsx` | **待核查**：已有诊断入口，未追齐环境来源扫描、显式清理及恢复链。 | 按平台识别实际覆盖来源而非仅提示变量名；确认后只改所选变量，备份可还原；读取不可触发写入；本次不操作用户环境。 |
| O04 Codex CLI 版本/安装来源检测及官方安装、升级、修复 | [tool lifecycle][cc-cli] | 邻近 `T/commands/status/overview.rs`、`src/components/RuntimeOverviewPanel.tsx` | **待核查**：CC 存在 Codex 工具分支；ASB 检测覆盖、生命周期动作与平台差异尚未逐项对齐。 | Windows/macOS/Linux及已支持的 WSL 路径分别验；选择实际安装来源，不盲改其他安装；失败有退出码/诊断；不把 CC 自身更新器或其他客户端安装补丁算进来。 |
| O05 托盘切换、用量与后台事件一致性 | [Codex tray + Auto][cc-tray] | `T/tray/snapshot.rs`、`T/commands/switching/`、`src/app/useConfigSnapshot.ts` | **部分**：现有托盘切换/缓存可见；账号级缓存、Auto/failover、项目 Profile 依赖未实现能力。 | 托盘与主窗口共用执行器；状态变更/失败/账号解绑后同步；不重复轮询；仍保留 ASB 托盘结构与样式，R05完成前不计全齐。 |
| O06 深链接导入 Codex provider / MCP / Skill / Prompt | [unified deeplink commands][cc-deeplink]、[Codex provider build][cc-deeplink-provider] | 邻近 `src/pages/ProviderImportPage.tsx`、`T/commands/extensions/portable.rs` | **待实现**：CC 数据扫描和 ASB 便携扩展包不是深链接解析/确认/导入入口。 | 编码/URL配置合并、多个端点、资源类型/客户端校验、重复与部分失败有结果；展示待执行脚本与启用意图；导入与立即应用分别确认。 |

## 2026-09-14 本轮实现说明

- 新功能使用独立的 Codex 账号、指令、通用配置、验证与设置入口文件；原 Codex 主页面/表单只收紧认证类型，没有布局或样式改版。
- 配对配置/auth 备份补齐了恢复与撤销恢复，陈旧或缺失认证预览拒绝写入；原生 token 刷新走专门执行器同步，账号库记录待重试意图，不把共享刷新令牌轮换后留成失效的 native 副本。
- 验证包括核心/执行器测试、Codex 后端切片、前端交互/契约/样式边界，以及仓库 Codex CLI 的隔离 loopback 无官方登录测试。最终命令结果记录在 progress.md；真实 OAuth/上游账号未用于写入或付费验证。
- **整个目标仍开放**：A07/R02/R08、P11、N01、G06/G09/G10、U04、S03、E04/E06、O01–O06 等仍有未实现或未完整验收内容；本轮没有以可用模块或 UI 入口替代全量验收。

## 排除与收口规则
- 按**调用客户端**判范围：[ClaudeAdapter 的 Codex OAuth FAST / priority][cc-claude-only]、Claude→Codex 反向转换、ClaudeDesktop 注入、Claude/Gemini 专属模型档/优化器，不因名字含 Codex 就纳入。Codex→Anthropic 的 G12 则确实在 Codex 请求路径，不能误排除；官方账号不参加 failover 是参考源码限制，不是删掉 failover 功能。
- ASB 可视化字段、通用配置文件、严格预览事务、WS/V2 compact、账号级重置观察等特色不得删除；已有增强不能抵扣其他行的缺口。若行为要替代 CC，必须记录等价用户结果及验收，不能擅自缩减范围。
- 每行关闭前须补齐实现提交/差异、生产者→命令→消费者链、针对性测试名称和**实际结果**；账号、配置、目录、持久数据变化同次更新直接契约/夹具/文档。当前无一行表示完整目标已验收。
- 后续验证必须使用隔离目录、假账号/假 HTTP/SSE/WS 服务；数据库相关变更先检查代表性历史 schema，再验新建及升级/回滚。前端/共享契约变化按范围执行 typecheck 与相关测试；真实账号请求另取授权，本次全部未运行。
- 收口按“所有账目闭环”而非 bug 数量：含直连/接管、官方/API-key/托管、三种上游、HTTP/SSE/WS、模型切换/工具多轮/compact、重启/外改/回滚矩阵。仍有待核查项就保留开放状态；新发现的 Codex 分支须入账，不以旧文档或当前短期修复范围排除。

<!-- 固定源码索引；本地路径与行号仅做静态检查，未请求 GitHub 或账号服务。 -->
[cc-provider]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/services/provider/mod.rs#L4541
[cc-provider-command]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/provider.rs
[cc-app]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src/App.tsx#L795
[cc-meta]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/provider.rs
[cc-presets]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src/config/codexProviderPresets.ts
[cc-template]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src/config/codexTemplates.ts
[cc-catalog]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/codex_config.rs#L1450
[cc-adapter]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/providers/codex.rs
[cc-model-fetch]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/model_fetch.rs#L95
[cc-check]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/services/stream_check.rs#L89
[cc-live]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/services/provider/live.rs
[cc-common]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/config.rs#L292
[cc-backfill]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/services/provider/live.rs#L943
[cc-config]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/codex_config.rs
[cc-settings]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/settings.rs#L975
[cc-auth]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/auth.rs#L111
[cc-oauth]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/providers/codex_oauth_auth.rs#L248
[cc-auth-live]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/services/provider/live.rs#L809
[cc-oauth-query]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/codex_oauth.rs#L28
[cc-proxy]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/services/proxy.rs
[cc-hot]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/services/proxy.rs#L2945
[cc-state-db]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/codex_state_db.rs#L28
[cc-proxy-command]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/proxy.rs
[cc-server]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/server.rs#L309
[cc-handlers]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/handlers.rs
[cc-forwarder]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/forwarder.rs
[cc-chat]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/providers/transform_codex_chat.rs
[cc-chat-stream]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/providers/streaming_codex_chat.rs
[cc-anthropic]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/providers/transform_codex_anthropic.rs
[cc-anthropic-stream]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/providers/streaming_codex_anthropic.rs
[cc-chat-history]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/providers/codex_chat_history.rs
[cc-global-proxy]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/global_proxy.rs
[cc-http-client]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/http_client.rs
[cc-proxy-types]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/types.rs#L162
[cc-failover]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/failover.rs
[cc-router]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/provider_router.rs
[cc-circuit]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/circuit_breaker.rs
[cc-encoding]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/content_encoding.rs
[cc-subscription]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/services/subscription.rs#L530
[cc-usage-log]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/usage/logger.rs
[cc-usage]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/usage.rs
[cc-session-usage]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/services/session_usage_codex.rs#L688
[cc-session]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/session_manager/providers/codex.rs
[cc-session-ui]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src/components/sessions/SessionManagerPage.tsx
[cc-session-command]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/session_manager.rs
[cc-history]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/codex_history_migration.rs
[cc-settings-command]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/settings.rs#L62
[cc-mcp]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/mcp/codex.rs
[cc-mcp-service]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/services/mcp.rs
[cc-skill]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/skill.rs
[cc-skill-service]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/services/skill.rs
[cc-prompt]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/services/prompt.rs#L117
[cc-prompt-path]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/prompt_files.rs#L12
[cc-profile]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/services/profile.rs
[cc-backups]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/import_export.rs
[cc-webdav]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/webdav_sync.rs
[cc-s3]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/s3_sync.rs
[cc-env]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/services/env_checker.rs#L20
[cc-env-command]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/env.rs
[cc-cli]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/misc.rs#L153
[cc-tray]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/tray.rs
[cc-deeplink]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/commands/deeplink.rs
[cc-deeplink-provider]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/deeplink/provider.rs
[cc-claude-only]: https://github.com/farion1231/cc-switch/blob/d695a2d77fd9081eafd3e9eedcbf2a97b3410928/src-tauri/src/proxy/providers/claude.rs#L404
