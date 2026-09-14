# Claude 本地后端契约

状态：实施中。保持既有 Claude 页面、控件和可视化通用配置设计；新接口可调用不等于页面入口已经全部交付。完整差距见 [验收清单](claude-ccswitch-parity.md)。

## 所有权

| 内容 | 唯一所有者 / 存储位置 |
| --- | --- |
| 档案 | `configuration/providers/claude/`，现有配置存储 |
| 可视化客户端偏好 | `configuration/client-settings/claude.json`，现有可视化设置目录 |
| 托管账号 | `claude_auth/`，`state/claude-accounts.json` |
| Prompt 库 | `claude_prompts/`，`state/configuration/claude-prompts.json` |
| Prompt 中断事务 | `state/configuration/claude-prompts.pending.json` |
| 流量/接管策略 | `gateway/failover.rs`、`claude_settings.rs`，`state/claude-failover.json` |
| 健康与本地价格 | `state/claude-health.json`、`state/claude-pricing.json` |
| 真正的客户端写入 | `asb-switch` 执行器；Claude 配置和 CLAUDE.md 都由后端解析目标路径 |

账号文件包含敏感凭据，应按本地供应商密钥文件保护，不提交到版本库、不写入日志。这里只管理 **Claude 上游**的账号：`codex_oauth` 表示 Claude 使用 ChatGPT 上游，不表示读写 Codex 客户端的账号。不会读取或覆盖 `~/.codex/auth.json`、Claude 的 `.credentials.json` 来实现这些托管请求。Claude 原生官方登录仍由原来的独立登录入口负责。

## 供应商与认证

- `src/api/claude-providers.ts`：离线预设列表、准备、克隆与搜索。准备仅产生 `ProviderDraft` 和 warnings，不保存或激活；后续使用现有档案保存和预览切换路径。
- 预设数据固定在 CC Switch `d695a2d77fd9081eafd3e9eedcbf2a97b3410928`。`scripts/import-claude-presets.mjs` 只接受该版本的本地源码；不联网，不复制 UI。`NOTICE.md` 保留数据来源与 MIT 声明。
- 90 项中 87 项可准备；Gemini Native、Bedrock AKSK 与 Bedrock API Key 返回 `unavailableReason`，准备操作会拒绝，不用 Anthropic 档案冒充。来源中的 Bedrock、Vertex、Foundry 原生模式也明确拒绝。
- `connection.providerType` / `authBinding.authProvider` 只接受 `github_copilot`、`codex_oauth`、`xai_oauth`。显式 `accountId` 固定该账号；缺省跟随该服务的本地默认账号；账号缺失不会回退到其他账号或原生 CLI 缓存。
- 托管档案 `apiKey` 为空，由请求时解析的账号提供 Bearer；ChatGPT/xAI 必须使用 Responses。账号返回的服务端点具有权威性，因此托管模式拒绝 `isFullUrl`、`customEndpoints`、`claudeModelsUrl` 覆盖。Header/UA/body 的有效请求覆盖仍由既有机制执行，认证头不能被覆盖。
- 普通 Claude API 档案支持 `claudeModelsUrl` 完整模型查询 URL，导入上游 `modelsUrl` 后不只是保存元数据。Bearer 与 x-api-key 的选择独立于 body 协议。
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

## 当前未完成的范围

新增账号管理、预设、Prompt 库、完整请求账本等页面操作入口尚未全部接齐，没有为此增加、替换 Claude 可见控件。额外通用片段/环境变量、完整源队列与排序导入、Gemini Native、云原生传输及完整跨供应商/退出崩溃矩阵仍在验收清单中。真实外部授权和订阅尚未用用户账号验证；自动测试只使用隔离目录、假凭据和本机上游。
