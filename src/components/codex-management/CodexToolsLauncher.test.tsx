import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { CodexToolsLauncher } from "./CodexToolsLauncher";
import { answer, filePreview } from "../../test/codex-management";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => {}) }));
const mocked = vi.mocked(invoke);
beforeEach(() => { mocked.mockReset(); mocked.mockImplementation(async (command, args) => answer(command, args as Record<string, unknown>)); });
async function open(pane?: string) {
  const user = userEvent.setup();
  render(<CodexToolsLauncher onChanged={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "管理 Codex 功能" }));
  await screen.findByRole("combobox", { name: "Codex 内置预设" });
  if (pane) await user.click(screen.getByRole("tab", { name: pane }));
  return user;
}
async function select(user: ReturnType<typeof userEvent.setup>, name: string, option: string) {
  await user.click(await screen.findByRole("combobox", { name }));
  await user.click(await screen.findByRole("option", { name: option }));
}
it("does not load extra Codex data until the settings action is opened", () => {
  render(<CodexToolsLauncher onChanged={vi.fn()} />);
  expect(mocked).not.toHaveBeenCalled();
  expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
});
it("creates an offline preset without activating it and clears the supplied key", async () => {
  const user = await open(); await select(user, "Codex 内置预设", "离线预设");
  await user.type(screen.getByLabelText("预设 API 密钥"), "isolated-input-key");
  await user.click(screen.getByRole("button", { name: "从预设创建档案" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("prepare_codex_preset", { presetId: "preset", apiKey: "isolated-input-key" }));
  await waitFor(() => expect(screen.getByLabelText("预设 API 密钥")).toHaveValue(""));
  expect(mocked.mock.calls.some(([name]) => name === "execute_switch")).toBe(false);
});
it("duplicates only the source revision without activation", async () => {
  const user = await open(); await user.click(await screen.findByRole("button", { name: "复制 隔离供应商" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("duplicate_codex_profile", { profileId: "provider-one", expectedFileHash: "provider-hash", name: null }));
  expect(mocked.mock.calls.some(([name]) => name === "execute_switch")).toBe(false);
});
it("saves a managed binding separately from the confirmed native-auth switch", async () => {
  const user = await open("认证"); await select(user, "Codex 账号绑定", "one@example.test");
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("set_codex_account_binding", { profileId: "official", selection: { kind: "account", id: "account-one" }, expectedRevision: "accounts-r1" }));
  expect(mocked.mock.calls.some(([name]) => name === "execute_switch")).toBe(false);
  await user.click(screen.getByRole("button", { name: "预览并应用账号绑定" }));
  const confirm = await screen.findByRole("button", { name: "确认应用 Codex 配置" });
  await waitFor(() => expect(confirm).toBeEnabled()); await user.click(confirm);
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("execute_switch", { profileId: "official", expectedHash: "before", expectedRenderedHash: "after", confirmWrite: true, authHash: "auth-before", authRenderedHash: "auth-after", authExisted: true }));
});
it("cancels only its own managed login when leaving the authentication pane", async () => {
  const user = await open("认证"); await user.click(await screen.findByRole("button", { name: "添加 Codex 账号" }));
  await screen.findByText("TEST-CODE"); await user.click(screen.getByRole("tab", { name: "供应商" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("cancel_codex_account_login", { sessionId: "login-session" }));
});
it("does not cancel a consumed gateway preparation while the commit is in flight", async () => {
  let finish!: (value: unknown) => void;
  mocked.mockImplementation(async (command, args) => command === "commit_codex_gateway_policy" ? new Promise((resolve) => { finish = resolve; }) : answer(command, args as Record<string, unknown>));
  const user = await open("网关与 Failover");
  await user.click(await screen.findByRole("button", { name: "预览 Codex 策略变更" }));
  await user.click(await screen.findByRole("button", { name: "确认应用 Codex 策略" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("commit_codex_gateway_policy", { preparationId: "policy-preparation", confirmWrite: true }));
  expect(mocked.mock.calls.filter(([name]) => name === "cancel_codex_gateway_policy")).toHaveLength(0);
  await act(async () => { finish({}); });
});
it("uses preview and a separate confirmation for prompt activation", async () => {
  const user = await open("指令预设"); await select(user, "Codex 指令预设", "工作指令");
  await user.click(screen.getByRole("button", { name: "预览启用预设" }));
  await screen.findByRole("region", { name: "Codex 指令变更预览" });
  expect(mocked.mock.calls.some(([name]) => name === "apply_codex_prompt")).toBe(false);
  await user.click(screen.getByRole("button", { name: "确认写入 AGENTS.md" }));
  await waitFor(() => expect(mocked).toHaveBeenCalledWith("apply_codex_prompt", { plan: { presetId: "prompt-one", revision: "prompts-r1", liveHash: "prompt-before", renderedHash: "prompt-after" }, confirmWrite: true }));
});
it("keeps missing prices explicitly unpriced and does not combine session and gateway ledgers", async () => {
  await open("计量"); await screen.findByText(/未计价 1/);
  expect(screen.getByText(/不与网关重复相加/)).toBeInTheDocument();
  expect(within(screen.getByRole("table", { name: "Codex 持久请求账本" })).getAllByRole("row")).toHaveLength(1);
});
it("shows stale-write diagnostics without pretending the switch succeeded", async () => {
  mocked.mockImplementation(async (command, args) => {
    if (command === "execute_switch") throw { code: "codex-account-preview-stale", message: "账号已变化，请重新预览" };
    return command === "preview_switch" ? filePreview : answer(command, args as Record<string, unknown>);
  });
  const user = await open("认证"); await user.click(await screen.findByRole("button", { name: "预览并应用账号绑定" }));
  const confirm = await screen.findByRole("button", { name: "确认应用 Codex 配置" }); await waitFor(() => expect(confirm).toBeEnabled()); await user.click(confirm);
  expect(await screen.findByRole("alert")).toHaveTextContent("账号已变化，请重新预览");
  expect(screen.queryByText(/Codex 配置已应用；/)).not.toBeInTheDocument();
});
