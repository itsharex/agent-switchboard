import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import * as api from "./codex-accounts";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const mocked = vi.mocked(invoke);
beforeEach(() => { mocked.mockReset(); mocked.mockResolvedValue({}); });
it("lists account metadata without credentials", async () => {
  await api.listCodexAccounts();
  expect(mocked).toHaveBeenCalledExactlyOnceWith("list_codex_accounts");
});
it("explicitly confirms native credential import and deletion", async () => {
  await api.importCodexNativeAccount("revision", false);
  expect(mocked).toHaveBeenLastCalledWith("import_codex_native_account", { expectedRevision: "revision", confirmWrite: false });
  await api.deleteCodexAccount("id", "next", true);
  expect(mocked).toHaveBeenLastCalledWith("delete_codex_account", { accountId: "id", expectedRevision: "next", confirmWrite: true });
});
it("keeps native, default and explicit account bindings distinct", async () => {
  for (const selection of [{ kind: "native" }, { kind: "default" }, { kind: "account", id: "chosen" }] as const) {
    await api.setCodexAccountBinding("profile", selection, "revision");
    expect(mocked).toHaveBeenLastCalledWith("set_codex_account_binding", { profileId: "profile", selection, expectedRevision: "revision" });
  }
});
it("targets account-specific models and quotas instead of a mutable global profile", async () => {
  await api.getCodexAccountModels("first");
  expect(mocked).toHaveBeenLastCalledWith("get_codex_account_models", { accountId: "first" });
  await api.getCodexAccountQuota("second");
  expect(mocked).toHaveBeenLastCalledWith("get_codex_account_quota", { accountId: "second" });
});
it("reauthenticates a stable account and forwards cancellation", async () => {
  await api.startCodexAccountLogin("existing");
  expect(mocked).toHaveBeenLastCalledWith("start_codex_account_login", { accountId: "existing" });
  await api.cancelCodexAccountLogin("session");
  expect(mocked).toHaveBeenLastCalledWith("cancel_codex_account_login", { sessionId: "session" });
});
it("keeps recoverable stale-account diagnostics", async () => {
  const error = { code: "codex-account-operation-failed", message: "账号库已变化" };
  mocked.mockRejectedValue(error);
  await expect(api.setCodexDefaultAccount(null, "stale")).rejects.toBe(error);
});
