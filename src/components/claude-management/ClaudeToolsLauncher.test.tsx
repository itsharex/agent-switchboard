import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ClaudeToolsLauncher } from "./ClaudeToolsLauncher";
import { answer, accounts, record, promptPlan, stopPlan, policy, integrationPreview } from "../../test/claude-management";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => {}) }));
const mocked = vi.mocked(invoke);
beforeEach(() => { mocked.mockReset(); mocked.mockImplementation(async (command, args) => answer(command, args as Record<string, unknown>)); });
async function open(pane?: string) {
  const user = userEvent.setup(); render(<ClaudeToolsLauncher onChanged={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "管理 Claude 功能" }));
  await screen.findByRole("combobox", { name: "Claude 离线预设" });
  if (pane) await user.click(screen.getByRole("tab", { name: pane }));
  return user;
}
async function select(user: ReturnType<typeof userEvent.setup>, name: string, option: string) {
  await user.click(await screen.findByRole("combobox", { name })); await user.click(await screen.findByRole("option", { name: option }));
}
function calls(command: string) { return mocked.mock.calls.filter(([name]) => name === command); }
it("loads nothing until opened and does not mount another provider workspace", () => {
  render(<ClaudeToolsLauncher onChanged={vi.fn()} />); expect(mocked).not.toHaveBeenCalled();
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument(); expect(screen.queryByLabelText("API 密钥")).not.toBeInTheDocument();
});
it("prepares then explicitly saves a preset without writing or activating the client", async () => {
  const user = await open(); await select(user, "Claude 离线预设", "离线 Claude 预设");
  await user.type(screen.getByLabelText("预设 API 密钥"), "fake-input-key");
  await user.click(screen.getByRole("button", { name: "准备 Claude 预设" }));
  await screen.findByLabelText("档案名称"); expect(calls("commit_profile_save")).toHaveLength(0);
  await user.click(screen.getByRole("button", { name: "预览保存预设档案" }));
  const confirm = await screen.findByRole("button", { name: "确认保存 Claude 档案" });
  expect(calls("commit_profile_save")).toHaveLength(0); await user.click(confirm);
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("commit_profile_save", { preparationId: "claude-save", confirmWrite: true }));
  await waitFor(() => expect(screen.getByLabelText("预设 API 密钥")).toHaveValue(""));
  expect(calls("execute_switch")).toHaveLength(0); expect(calls("prepare_codex_preset")).toHaveLength(0);
});
it("uses profile revisions and a live preview for connection edits while preserving model metadata", async () => {
  const user = await open(); await select(user, "Claude 供应商管理操作", "档案连接、绑定与复制");
  await select(user, "Claude 本地档案", "隔离 Claude 供应商");
  await user.type(screen.getByLabelText("自定义 User-Agent"), "asb-fixture");
  await user.click(screen.getByRole("button", { name: "预览保存 Claude 连接设置" }));
  const confirm = await screen.findByRole("button", { name: "确认保存并应用 Claude 配置" });
  expect(mocked).toHaveBeenCalledWith("prepare_profile_save", expect.objectContaining({ profileId: "provider-one", expectedFileHash: "provider-r1", draft: expect.objectContaining({ app: "claude", modelOptions: record.profile.modelOptions, connection: expect.objectContaining({ customUserAgent: "asb-fixture" }) }) }));
  expect(calls("commit_profile_save")).toHaveLength(0); await user.click(confirm);
  await waitFor(() => expect(calls("commit_profile_save")).toHaveLength(1));
});
it("duplicates only a selected Claude revision", async () => {
  const user = await open(); await select(user, "Claude 供应商管理操作", "档案连接、绑定与复制"); await select(user, "Claude 本地档案", "隔离 Claude 供应商");
  await user.click(screen.getByRole("button", { name: "确认创建副本" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("duplicate_claude_profile", { profileId: "provider-one", expectedFileHash: "provider-r1", name: "隔离 Claude 供应商 副本", confirmWrite: true }));
  expect(calls("execute_switch")).toHaveLength(0);
});
it("edits the profile-owned settings fragment through the preview-save flow", async () => {
  const user = await open(); await select(user, "Claude 供应商管理操作", "档案连接、绑定与复制");
  await select(user, "Claude 本地档案", "隔离 Claude 供应商");
  const field = screen.getByLabelText("Claude 设置附加片段（JSON）");
  expect(field).toHaveValue(JSON.stringify(record.profile.claudeFragment, null, 2));
  fireEvent.change(field, { target: { value: '{"env":{"HTTP_PROXY":"http://127.0.0.1:1"}}' } });
  await user.click(screen.getByRole("button", { name: "预览保存 Claude 连接设置" }));
  await screen.findByRole("button", { name: "确认保存并应用 Claude 配置" });
  expect(mocked).toHaveBeenCalledWith("prepare_profile_save", expect.objectContaining({
    profileId: "provider-one",
    expectedFileHash: "provider-r1",
    draft: expect.objectContaining({ claudeFragment: { env: { HTTP_PROXY: "http://127.0.0.1:1" } } }),
  }));
});
it("imports the shared snippet after a read-only scan without writing client files", async () => {
  const user = await open(); await select(user, "Claude 供应商管理操作", "通用配置片段导入");
  await user.type(screen.getByLabelText("CC Switch 数据库路径"), "isolated/source.db");
  await user.click(screen.getByRole("button", { name: "只读扫描 Claude 通用配置片段" }));
  const section = await screen.findByRole("region", { name: "Claude 通用配置片段预览" });
  expect(within(section).getByText(/可视化偏好 1 项/)).toBeInTheDocument();
  expect(within(section).getByText(/未导入: env\.ANTHROPIC_AUTH_TOKEN/)).toBeInTheDocument();
  expect(calls("import_claude_snippet_source")).toHaveLength(0);
  await user.click(within(section).getByRole("button", { name: "确认导入到客户端偏好与通用片段" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("import_claude_snippet_source", {
    sourcePath: "isolated/source.db", sourceRevision: "snippet-source-r1", expectedSettingsHash: "client-settings-r1", confirmWrite: true,
  }));
  expect(calls("execute_switch")).toHaveLength(0);
});
it("cancels only the Claude device session on leaving the pane", async () => {
  const user = await open("认证"); await user.type(screen.getByLabelText("新账号名称"), "isolated-device");
  await user.click(screen.getByRole("button", { name: "开始 Claude 托管账号登录" })); await screen.findByText("CLAUDE-CODE");
  await user.click(screen.getByRole("tab", { name: "供应商" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("cancel_claude_account_login", { sessionId: "claude-login" }));
  expect(calls("cancel_codex_account_login")).toHaveLength(0);
});
it("refreshes account revisions after a model query that may rotate tokens", async () => {
  let reads = 0;
  mocked.mockImplementation(async (command, args) => command === "get_claude_accounts" ? { ...accounts, fileHash: ++reads > 2 ? "accounts-refreshed" : "accounts-r1" } : answer(command, args as Record<string, unknown>));
  const user = await open("认证"); const table = await screen.findByRole("table", { name: "Claude 托管账号" });
  await user.click(within(table).getByRole("button", { name: "模型" })); await screen.findByRole("table", { name: "Claude 账号模型" });
  await waitFor(() => expect(within(table).getByRole("button", { name: "设为默认" })).toBeEnabled());
  await user.click(within(table).getByRole("button", { name: "设为默认" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("set_claude_default_account", { provider: "github_copilot", accountId: "account-one", expectedFileHash: "accounts-refreshed", confirmWrite: true }));
});
it("saves an active Prompt to the library without silently rewriting CLAUDE.md", async () => {
  const user = await open("Prompt 预设"); await select(user, "Claude Prompt 预设", "本地 Prompt · 使用中");
  await user.clear(screen.getByLabelText("Claude Prompt 内容")); await user.type(screen.getByLabelText("Claude Prompt 内容"), "New instructions");
  await user.click(screen.getByRole("button", { name: "保存 Claude Prompt 到库" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("save_claude_prompt", { promptId: "prompt-one", draft: { name: "本地 Prompt", description: null, content: "New instructions" }, expectedFileHash: "prompts-r1", confirmWrite: true }));
  expect(calls("activate_claude_prompt")).toHaveLength(0);
  await user.click(screen.getByRole("button", { name: "预览启用 Claude Prompt" })); await screen.findByRole("button", { name: "确认写入 CLAUDE.md" });
  expect(calls("activate_claude_prompt")).toHaveLength(0); await user.click(screen.getByRole("button", { name: "确认写入 CLAUDE.md" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("activate_claude_prompt", { plan: promptPlan, confirmWrite: true }));
});
it("imports selected source Prompts with both revisions and never follows source enabled automatically", async () => {
  const user = await open("Prompt 预设"); await user.type(screen.getByLabelText("CC Switch 数据库路径"), "isolated/source.db");
  await user.click(screen.getByRole("button", { name: "只读扫描 Claude Prompt" }));
  await user.click(await screen.findByRole("checkbox", { name: "导入 来源 Prompt" }));
  await user.click(screen.getByRole("button", { name: "确认导入所选 Claude Prompt" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("import_claude_prompt_source", { sourcePath: "isolated/source.db", sourceIds: ["source-one"], sourceRevision: "source-r1", expectedFileHash: "prompts-r1", confirmWrite: true }));
  expect(calls("activate_claude_prompt")).toHaveLength(0);
});
it("imports the source failover queue after a read-only scan", async () => {
  const user = await open("网关与 Failover");
  await user.type(screen.getByLabelText("CC Switch 数据库路径"), "isolated/source.db");
  await user.click(screen.getByRole("button", { name: "只读扫描 Claude 故障转移队列" }));
  const section = await screen.findByRole("region", { name: "Claude 故障转移队列预览" });
  expect(within(section).getByText("中继 A")).toBeInTheDocument();
  expect(within(section).getByText(/未匹配/)).toBeInTheDocument();
  expect(calls("import_claude_failover_source")).toHaveLength(0);
  await user.click(within(section).getByRole("button", { name: "确认导入队列与策略" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("import_claude_failover_source", {
    sourcePath: "isolated/source.db", sourceRevision: "failover-source-r1", expectedPolicyHash: "policy-r1", confirmWrite: true,
  }));
  expect(calls("commit_codex_gateway_policy")).toHaveLength(0);
});
it("saves Claude failover settings without changing Codex policy", async () => {
  const user = await open("网关与 Failover"); await user.click(await screen.findByRole("checkbox", { name: "启用 Claude Failover" }));
  await user.click(screen.getByRole("checkbox", { name: "将 隔离 Claude 供应商 纳入 Claude 队列" }));
  await user.clear(screen.getByLabelText("最多重试次数（首次请求之外）")); await user.type(screen.getByLabelText("最多重试次数（首次请求之外）"), "2");
  await user.click(screen.getByRole("button", { name: "确认保存 Claude 请求策略" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("set_claude_failover_policy", { policy: { ...policy.policy, enabled: true, providerIds: ["provider-one"], maxRetries: 2 }, confirmWrite: true }));
  expect(calls("commit_codex_gateway_policy")).toHaveLength(0);
});
it("requires the gateway restore preview before the real stop command", async () => {
  const user = await open("网关与 Failover"); await user.click(await screen.findByRole("button", { name: "预览停止 Claude 接管" }));
  const confirm = await screen.findByRole("button", { name: "确认停止并恢复 Claude 配置" }); expect(calls("stop_claude_gateway")).toHaveLength(0); await user.click(confirm);
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("stop_claude_gateway", { preview: stopPlan, confirmWrite: true }));
});
it("distinguishes unpriced usage and preserves decimal strings in the price-book write", async () => {
  const user = await open("请求计量"); await screen.findByText(/未计价 2/); expect(screen.getByText(/输入 未知/)).toBeInTheDocument();
  await user.type(screen.getByLabelText("计价模型 ID"), "gemini-fixture");
  for (const label of ["输入", "输出（含思考）", "缓存读取", "缓存写入"]) await user.type(screen.getByLabelText(label), "0.123456789");
  await user.click(screen.getByRole("button", { name: "确认保存 Claude 模型价格" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("set_claude_price_book", { expectedFileHash: "prices-r1", confirmWrite: true, book: { version: 1, models: { "gemini-fixture": { inputUsdPerMillion: "0.123456789", outputUsdPerMillion: "0.123456789", cacheReadUsdPerMillion: "0.123456789", cacheCreationUsdPerMillion: "0.123456789", source: "user" } } } }));
});
it("previews then confirms a Claude client-integration marker and saves the switch policy separately", async () => {
  const user = await open("客户端集成");
  const plugin = await screen.findByRole("region", { name: "插件 API Key 标记" });
  expect(within(plugin).getByText(/文件不存在/)).toBeInTheDocument();
  await user.click(within(plugin).getByRole("button", { name: "预览写入插件 API Key 标记" }));
  const section = await screen.findByRole("region", { name: "Claude 客户端集成预览" });
  expect(mocked).toHaveBeenCalledWith("preview_claude_integration", { flag: "plugin", enable: true });
  expect(calls("apply_claude_integration")).toHaveLength(0);
  await user.click(within(section).getByRole("button", { name: "确认写入 Claude 客户端标记" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("apply_claude_integration", { preview: integrationPreview, confirmWrite: true }));
  await waitFor(() => expect(within(screen.getByRole("region", { name: "插件 API Key 标记" })).getByText(/已写入/)).toBeInTheDocument());
  await user.click(screen.getByRole("checkbox", { name: "切换 Claude 供应商后自动同步插件 API Key 标记" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("set_claude_integration_policy", { policy: { pluginIntegration: true }, confirmWrite: true }));
  expect(calls("execute_switch")).toHaveLength(0);
});
it("reads the native Claude subscription without touching managed accounts", async () => {
  const user = await open("认证");
  await user.click(await screen.findByRole("button", { name: "查询官方登录订阅额度" }));
  const section = await screen.findByRole("region", { name: "Claude 官方登录订阅额度" });
  expect(within(section).getByText("5 小时")).toBeInTheDocument();
  expect(mocked).toHaveBeenCalledWith("get_claude_native_quota");
  expect(calls("get_claude_account_quota")).toHaveLength(0);
});
it("scans environment conflicts read-only, removes only the selection with the scan revision, and restores a named backup", async () => {
  const user = await open("客户端集成");
  const section = await screen.findByRole("region", { name: "Claude 环境变量冲突" });
  await user.click(within(section).getByRole("button", { name: "扫描 ANTHROPIC 环境变量" }));
  await within(section).findByText("••••••••");
  expect(within(section).getByRole("button", { name: "备份并删除所选环境变量" })).toBeDisabled();
  await user.click(within(section).getByRole("checkbox", { name: "删除 ANTHROPIC_API_KEY（isolated/.zshrc:3）" }));
  await user.click(within(section).getByRole("button", { name: "备份并删除所选环境变量" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("remove_claude_env_conflicts", {
    selections: [{ varName: "ANTHROPIC_API_KEY", source: { kind: "file", path: "isolated/.zshrc", line: 3 } }], expectedRevision: "env-r1", confirmWrite: true,
  }));
  await user.click(await within(section).findByRole("button", { name: "恢复 env-20260914T000000.000Z.json" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("restore_claude_env_backup", { fileName: "env-20260914T000000.000Z.json", confirmWrite: true }));
});
it("keeps CLI session usage separate from the gateway ledger and confirms before a rebuild", async () => {
  const user = await open("请求计量");
  const section = await screen.findByRole("region", { name: "Claude 会话用量" });
  expect(calls("get_claude_session_usage")).toHaveLength(0);
  await user.click(within(section).getByRole("button", { name: "同步并读取 Claude 会话用量" }));
  await within(section).findByText(/已匹配网关 1/);
  expect(within(section).getByText("custom-model")).toBeInTheDocument();
  expect(within(section).getByText("未计价")).toBeInTheDocument();
  await user.click(within(section).getByRole("button", { name: "重建会话用量账本" }));
  expect(calls("rebuild_claude_session_usage")).toHaveLength(0);
  await user.click(within(section).getByRole("button", { name: "确认重建 Claude 会话用量" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("rebuild_claude_session_usage", { confirmWrite: true }));
  expect(calls("sync_codex_session_usage")).toHaveLength(0);
});
it("manages candidate endpoints with the profile revision and probes them without credentials", async () => {
  const user = await open(); await select(user, "Claude 供应商管理操作", "档案连接、绑定与复制");
  await select(user, "Claude 本地档案", "隔离 Claude 供应商");
  const section = await screen.findByRole("region", { name: "Claude 候选端点" });
  expect(within(section).getByText(/https:\/\/fixture\.invalid\/v1（主地址）/)).toBeInTheDocument();
  await user.click(within(section).getByRole("button", { name: "测试全部端点延迟" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("test_claude_endpoints", { urls: ["https://fixture.invalid/v1", "https://backup.fixture.invalid/v1"] }));
  await within(section).findByText(/正常 · 42 ms · HTTP 200/); expect(within(section).getByText(/不可达/)).toBeInTheDocument();
  await user.type(within(section).getByLabelText("新增候选端点地址"), "https://third.fixture.invalid/v1");
  await user.click(within(section).getByRole("button", { name: "添加候选端点" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("add_provider_endpoint", { providerId: "provider-one", url: "https://third.fixture.invalid/v1", expectedFileHash: "provider-r1", confirmWrite: true }));
  await user.click((await within(section).findAllByRole("button", { name: "删除端点" }))[0]);
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("remove_provider_endpoint", expect.objectContaining({ providerId: "provider-one", url: "https://backup.fixture.invalid/v1", expectedFileHash: "provider-r2", confirmWrite: true })));
  expect(calls("execute_switch")).toHaveLength(0);
});
it("imports only importable CC Switch MCP rows into the library after a read-only scan", async () => {
  const user = await open("客户端集成");
  const section = await screen.findByRole("region", { name: "导入 CC Switch MCP 服务" });
  await user.type(within(section).getByLabelText("CC Switch MCP 数据库路径"), "isolated/source.db");
  await user.click(within(section).getByRole("button", { name: "只读扫描 CC Switch MCP 服务" }));
  await within(section).findByText(/来源已为 Claude 启用/);
  expect(within(section).getByRole("checkbox", { name: "导入 events" })).toBeDisabled();
  expect(within(section).getByRole("checkbox", { name: "导入 broken" })).toBeDisabled();
  await user.click(within(section).getByRole("checkbox", { name: "导入 docs" }));
  expect(calls("import_claude_mcp_source")).toHaveLength(0);
  await user.click(within(section).getByRole("button", { name: "确认导入所选 MCP 服务到扩展库" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("import_claude_mcp_source", { sourcePath: "isolated/source.db", sourceIds: ["m1"], sourceRevision: "mcp-source-r1", confirmWrite: true }));
  expect(calls("apply_extension_plan")).toHaveLength(0);
});
