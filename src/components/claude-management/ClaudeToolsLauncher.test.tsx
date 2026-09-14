import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { ClaudeToolsLauncher } from "./ClaudeToolsLauncher";
import { answer, accounts, record, promptPlan, stopPlan, policy } from "../../test/claude-management";
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
