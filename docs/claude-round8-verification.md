# Claude 对齐第八轮：待执行验证清单

本轮（2026-09-14 至 2026-09-15）实现期间，命令执行通道（Bash / PowerShell / node_repl / Agent）因模型侧安全分类器持续报错而不可用，**所有 Rust / 前端编译、测试与类型检查均未在会话中执行**。本文件列出需要在本机运行的命令、预期结果与失败时的定位入口。全部命令在仓库根目录 `F:\projects\agent-switchboard` 执行；Rust 构建使用独立目标目录，避免与正在运行的开发实例争用 `target/`。

跑完后请把结果（通过/失败计数、失败用例名与断言信息）贴回会话，我据此修错并把「验证」段落补进 `progress.md` 第八轮条目与 `docs/claude-ccswitch-parity.md`。

## 0. 本轮改动范围（用于失败定位）

新增文件：

| 文件 | 内容 |
| --- | --- |
| `src-tauri/src/claude_native_quota.rs` + `claude_native_quota/tests.rs` | 官方登录订阅额度（C15） |
| `src-tauri/src/claude_integration.rs` + `claude_integration/tests.rs` | 插件标记 / 引导跳过标记 + 切换后策略对齐（C16） |
| `src-tauri/src/claude_env_conflicts.rs` + `claude_env_conflicts/tests.rs` | `ANTHROPIC*` 环境变量冲突扫描/备份/删除/恢复（C16；Windows 依赖新增的 `winreg`） |
| `src-tauri/src/claude_session_usage.rs` + `claude_session_usage/tests.rs` | CLI 会话用量账本（C14） |
| `src-tauri/src/claude_mcp_source.rs`（含内联测试） | CC Switch `mcp_servers` 表导入扩展库（C16/C17） |
| `src-tauri/src/commands/claude_integration.rs`、`claude_env.rs`、`claude_session_usage.rs`、`claude_endpoints.rs`、`claude_mcp_import.rs` | 对应 Tauri 命令；`claude_endpoints` 是候选端点批量测速（C13） |
| `src-tauri/src/dev_api/claude_dispatch/integration_api.rs` | 开发 IPC 镜像 |
| `crates/asb-core/src/extensions/mcp/claude_launcher.rs` | Windows `cmd /c` 启动器包装/还原（C16 MCP） |
| `src/api/claude-integration.ts`、`src/api/claude-env.ts`、`src/api/claude-mcp-source.ts`、`src/api/claude-integration.test.ts` | 前端 API |
| `src/components/claude-management/IntegrationPane.tsx`、`EnvConflicts.tsx`、`SessionUsage.tsx`、`EndpointsPanel.tsx`、`McpSource.tsx` | 管理弹窗新页/新面板 |

改动文件：`src-tauri/src/lib.rs`、`commands/mod.rs`、`command_registry.rs`、`commands/claude_accounts.rs`（`get_claude_native_quota`）、`commands/switching/mod.rs`（切换后对齐插件标记）、`local_state/mod.rs`（两个新路径解析）、`dev_api/claude_dispatch.rs`、`dev_api/claude_dispatch/accounts_api.rs`、`providers_api.rs`、`extensions/checks/mod.rs`（Windows 包装）、`commands/extensions/planner/{deploy,rules/mod,rename/mod}.rs`（`render_claude` 加 host 参数）、`extensions/secrets.rs`（测试调用签名）、`src-tauri/Cargo.toml`（`[target.'cfg(windows)'.dependencies] winreg = "0.55"`）；`crates/asb-core/src/extensions/mcp/{mod,render,import,tests}.rs`、`crates/asb-core/src/claude_common/tests.rs`（快捷开关回归）；前端 `src/api/claude-accounts.ts`、`claude-ledger.ts`、`claude-providers.ts`、`src/components/claude-management/{ClaudeToolsLauncher,AccountsPane,MeteringPane,ProvidersPane}.tsx`、`ClaudeToolsLauncher.test.tsx`、`src/test/claude-management.ts`、`src/components/extensions/mcp-create/{presets.ts,presets.test.ts}`。

## 1. Rust：编译（含测试目标）

```powershell
Set-Location src-tauri
$env:CARGO_TARGET_DIR = "../target-check"
cargo check --tests 2>&1 | Select-String -Pattern "^error" -Context 0,10
```

预期：无 `error` 输出（既有的 `is_probe` 未使用与 `outbound_proxy` 警告属并行 Codex 会话域，可忽略）。

最可能的编译点（按风险排序）：

1. `claude_session_usage.rs`：`ClaudeRequestLedger::page`/`ClaudePriceBook::load`/`estimate` 的可见性均为 `pub(crate)`，应可访问；`sha2::Digest` 通过 `sha2` 依赖（已在 Cargo.toml）。若 `ClaudeRequestRecord` 字段集与 `request_ledger.rs` 有出入，按该文件当前定义补齐。
2. `claude_env_conflicts.rs` Windows 分支：`winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_SET_VALUE}`、`winreg::{RegKey, HKEY}`；`RegKey::enum_values()` 迭代 `io::Result<(String, RegValue)>`，`RegValue: Display`。
3. `claude_integration.rs`：`execute_rendered` 需要 `RenderedWriteRequest{target, app, backup_dir, expected_hash, expected_target_existed, rendered, reason}`；`KeyChange`/`ChangeKind` 来自 `asb_core::contracts`。
4. `commands/claude_endpoints.rs`：`std::thread::scope` 闭包捕获；`tauri::async_runtime::block_on` 仅测试用。
5. `crates/asb-core/src/extensions/mcp/render.rs`：`render_claude` 新签名 `(definition, resolve, host: ClaudeHost)`；所有调用点已改（`grep render_claude(` 应只剩带三个参数的）。

## 2. Rust：本轮新增/改动的切片

```powershell
Set-Location src-tauri
$env:CARGO_TARGET_DIR = "../target-check"
cargo test --lib -- claude_native_quota claude_integration claude_env_conflicts claude_session_usage claude_mcp_source commands::claude_endpoints
```

预期通过（共 19 项）：

| 模块 | 测试 | 覆盖 |
| --- | --- | --- |
| `claude_native_quota::tests` | `credential_parsing_accepts_both_key_spellings_and_expiry_forms` | `claudeAiOauth`/`claude.ai_oauth`；秒/毫秒/RFC3339 过期；Debug 脱敏 |
| 同上 | `missing_or_invalid_credentials_are_named_without_touching_the_file` | 缺文件/坏 JSON/无 OAuth/空令牌各自具名，原文件不动 |
| 同上 | `a_successful_query_sends_the_oauth_beta_and_reports_known_and_unknown_windows` | Bearer + `anthropic-beta` 头；已知/未知窗口、extra_usage、过期标记、序列化不含令牌（本机 tiny_http） |
| 同上 | `rejected_authorization_and_invalid_bodies_are_named_without_the_token` | 401 → 重新登录提示；非 JSON 体 |
| `claude_integration::tests` | `plugin_marker_is_created_previewed_and_cleared_with_sibling_keys_kept` | 缺文件创建、预览 diff、apply 幂等拒绝、无变化不写、清除保留邻键、备份计数 |
| 同上 | `onboarding_marker_keeps_the_rest_of_the_user_document` | `~/.claude.json` 其余键保留、锁释放 |
| 同上 | `non_object_documents_and_external_changes_never_get_overwritten` | 非对象拒绝；预览后外改拒绝且无备份 |
| 同上 | `switch_reconciliation_follows_the_policy_and_the_new_route` | 策略关/开；custom→写、official→清、幂等；未知策略键拒绝 |
| `claude_env_conflicts::tests` | `shell_exports_are_parsed_with_quotes_stripped_and_comments_ignored` | export 解析边界 |
| 同上 | `scanning_files_redacts_secrets_and_keeps_line_numbers` | 脱敏、行号、大小写前缀 |
| 同上 | `removal_requires_the_scan_revision_backs_up_full_values_and_restores_them` | 修订校验、完整备份、按高行号先删、恢复往返、路径穿越拒绝 |
| 同上 | `a_line_that_changed_after_the_scan_is_not_removed` | 扫描后行变化不删且提示备份已存 |
| `claude_session_usage::tests` | `assistant_messages_are_imported_once_priced_and_superseded_by_the_final_line` | 去重覆盖、含缓存输入、价格表计价（sonnet 0.000944）、未知模型不计价、幂等 |
| 同上 | `appends_are_incremental_and_rewrites_pin_without_replaying` | 增量追加、未换行尾行、外改钉住、重建备份 |
| 同上 | `a_gateway_request_with_the_same_usage_inside_the_window_is_marked_matched` | ±10 分钟 + 四类 token + 模型匹配 |
| `commands::claude_endpoints::tests` | `validation_rejects_empty_and_oversized_batches` / `a_blank_or_malformed_url_is_reported_in_place_without_a_network_call` / `results_keep_the_input_order_across_concurrent_chunks` | 批量上限、就地报错、并发保序（本机 tiny_http） |
| `claude_mcp_source::tests` | `rows_convert_through_the_claude_importer_with_ui_keys_stripped_and_problems_named` | UI 键剥离、`server` 嵌套、`cmd /c npx` 还原、env 引用、坏行具名跳过、元数据 |
| 同上 | `revision_and_existing_definitions_guard_the_import` | 修订校验、重复选择拒绝、同名同内容计已有、同名异内容拒绝 |

注：`claude_mcp_source` 测试里 `validate_definition` 要求 id 只含字母数字连字符（fixture 用 `ext-fixture`），且 `mcpMetadata.homepage` 走 `validate_mcp_metadata`——若 `https://example.test` 被拒，改 fixture 为 `https://example.com`。

注：`claude_session_usage` 的 sonnet 费用断言依据内置价格表 `claude-sonnet-5` = 输入 3 / 输出 15 / 缓存读 0.3 / 缓存写 3.75 USD/百万；若 `claude_pricing_seed.rs` 该行不同，按实际值修正断言而非改价格。

## 3. Rust：asb-core 切片

```powershell
Set-Location crates/asb-core
$env:CARGO_TARGET_DIR = "../../target-check"
cargo test claude_common extensions::mcp
```

预期新增通过：`claude_common::tests::upstream_quick_toggles_round_trip_through_the_profile_fragment`（五个快捷开关导入→投影→切回官方撤下）、`extensions::mcp::tests::windows_claude_renders_wrap_shell_launchers_and_imports_unwrap_them`（Windows 包装/Unix 不包/`.cmd` 大小写路径/`cmd` 不重复包/导入还原/普通 `cmd /c dir` 不动）。既有 `claude_http_bearer_renders_into_authorization` 已改为传 `ClaudeHost::Unix`。

## 4. Rust：全量回归

```powershell
Set-Location src-tauri
$env:CARGO_TARGET_DIR = "../target-check"
cargo test --lib 2>&1 | Select-String -Pattern "test result|FAILED|panicked"
```

预期：0 失败。基线为第七轮 lib 991/0/20 忽略（Codex 第九轮记 992/0/22）。本轮净增 22 项。**Windows 主机特别注意**：`render_claude` 在 Windows 上会把 `npx` 投影为 `cmd /c npx`，`dev_api/extension_sandbox_tests` 里的 `npx` 夹具目前只投到 Codex（不受影响）；若有其他用例在 Windows 上对 Claude 渲染文本断言 `"command":"npx"`，那是本轮预期变化，应改断言为 `cmd`。

## 5. Rust：格式

```powershell
rustfmt --check --edition 2021 src-tauri/src/claude_native_quota.rs src-tauri/src/claude_native_quota/tests.rs src-tauri/src/claude_integration.rs src-tauri/src/claude_integration/tests.rs src-tauri/src/claude_env_conflicts.rs src-tauri/src/claude_env_conflicts/tests.rs src-tauri/src/claude_session_usage.rs src-tauri/src/claude_session_usage/tests.rs src-tauri/src/claude_mcp_source.rs src-tauri/src/commands/claude_integration.rs src-tauri/src/commands/claude_env.rs src-tauri/src/commands/claude_session_usage.rs src-tauri/src/commands/claude_endpoints.rs src-tauri/src/commands/claude_mcp_import.rs src-tauri/src/commands/claude_accounts.rs src-tauri/src/commands/switching/mod.rs src-tauri/src/local_state/mod.rs src-tauri/src/dev_api/claude_dispatch.rs src-tauri/src/dev_api/claude_dispatch/integration_api.rs src-tauri/src/dev_api/claude_dispatch/accounts_api.rs src-tauri/src/dev_api/claude_dispatch/providers_api.rs src-tauri/src/extensions/checks/mod.rs src-tauri/src/commands/extensions/planner/deploy.rs src-tauri/src/commands/extensions/planner/rules/mod.rs src-tauri/src/commands/extensions/planner/rename/mod.rs crates/asb-core/src/extensions/mcp/claude_launcher.rs crates/asb-core/src/extensions/mcp/render.rs crates/asb-core/src/extensions/mcp/import.rs crates/asb-core/src/extensions/mcp/tests.rs crates/asb-core/src/claude_common/tests.rs
```

预期无 `Diff in` 输出。有差异时对同一文件列表执行 `rustfmt --edition 2021 <files>`（只格式化本轮触碰的文件）。

## 6. 前端：类型检查

```powershell
npx tsc --noEmit
```

预期 0 错误。最可能的类型点：`EndpointsPanel.tsx` 的 `ProbeResult` 走 `import("./providers").ProbeResult`；`SessionUsage.tsx`/`IntegrationPane.tsx`/`EnvConflicts.tsx` 只用既有 `Button/Checkbox/Table/Input/DiffView`。

## 7. 前端：本域切片

```powershell
npx vitest run src/components/claude-management src/api/claude-integration.test.ts src/api/claude-accounts.test.ts src/components/extensions/mcp-create/presets.test.ts
```

预期新增通过（launcher 文件 +6，api +1，presets 改 3）：

| 文件 | 用例 |
| --- | --- |
| `ClaudeToolsLauncher.test.tsx` | `previews then confirms a Claude client-integration marker and saves the switch policy separately` |
| 同上 | `reads the native Claude subscription without touching managed accounts` |
| 同上 | `scans environment conflicts read-only, removes only the selection with the scan revision, and restores a named backup` |
| 同上 | `keeps CLI session usage separate from the gateway ledger and confirms before a rebuild` |
| 同上 | `manages candidate endpoints with the profile revision and probes them without credentials` |
| 同上 | `imports only importable CC Switch MCP rows into the library after a read-only scan` |
| `claude-integration.test.ts` | `routes both Claude client markers through preview, explicit confirmation, and a Claude-only policy` |
| `presets.test.ts` | 三项改为不带平台参数（前端不再包装 `cmd /c`） |

若 launcher 用例因 `ProvidersPane` 在打开时额外请求 `list_provider_endpoints` 而失败，检查 `src/test/claude-management.ts` 的 `answer()` 已含 `list_provider_endpoints` / `test_claude_endpoints` / `get_claude_session_usage` / `get_claude_integration` / `scan_claude_env_conflicts` / `scan_claude_mcp_source` 等分支（本轮已加）。

## 8. 前端：全量与构建

```powershell
npx vitest run
npm run build
```

预期 0 失败、构建成功。并行 Codex 会话在途改动可能带来其域内的波动失败，按文件路径归属判断。

## 9. 手工核对（无自动测试覆盖的路径）

| 项 | 步骤 | 预期 |
| --- | --- | --- |
| Windows 注册表环境冲突 | 在「客户端集成」页扫描 | 若用户环境里有 `ANTHROPIC_*` 用户/系统变量应列出且值脱敏；不要在真实机器上点删除，除非确认需要 |
| macOS 钥匙串读取（仅 mac） | 「认证」页「查询官方登录订阅额度」 | 有 Claude CLI 登录时返回窗口；无登录时提示未找到凭据 |
| 官方订阅额度真实接口 | 同上（需真实 Claude 登录） | 5 小时 / 7 天窗口百分比与 CLI `/usage` 一致 |
| 切换后插件标记 | 开启策略后切到自定义供应商，再切回官方 | `~/.claude/config.json` 的 `primaryApiKey` 先出现后消失，切换结果无警告 |
| Windows MCP 包装 | 在 Windows 上把一个 `npx` MCP 投到 Claude 用户级 | `~/.claude.json` 中该条目为 `cmd /c npx …`；扩展库详情仍显示 `npx`；重新发现/接管不产生漂移 |

## 10. 完成后

把各步结果贴回，我会：补 `progress.md` 第八轮「验证」段落；把 `docs/claude-ccswitch-parity.md`「本轮验证」的待验证注记改为实测计数；修任何失败项。
