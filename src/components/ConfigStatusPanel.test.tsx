import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import type { ConfigFileStatus, MatchStatus, RouteState } from "../api/client";
import { ConfigStatusPanel } from "./ConfigStatusPanel";

const status: ConfigFileStatus = {
  app: "codex", path: "/isolated/codex/config.toml", exists: true, syntaxOk: true,
  route: null, readError: null, activeProfileId: "gateway", lastSwitch: null,
  matchStatus: { kind: "matchesProfile", profileId: "gateway", profileName: "网关" },
};

it.each<{ matchStatus: MatchStatus; label: string }>([
  { matchStatus: status.matchStatus, label: "与档案「网关」一致" },
  { matchStatus: { kind: "profileChanged", profileName: "网关" }, label: "档案「网关」或客户端设置已变更，尚未应用" },
  { matchStatus: { kind: "externallyModified", at: "2026-09-08T00:00:00Z" }, label: "配置可能被外部修改" },
  { matchStatus: { kind: "restoredBackup", at: "2026-09-08T00:00:00Z" }, label: "当前为已恢复备份" },
  { matchStatus: { kind: "unmanaged" }, label: "从未由本应用切换，也不匹配任何档案" },
])("preserves the $matchStatus.kind diagnosis", ({ matchStatus, label }) => {
  render(<ConfigStatusPanel statuses={[{ ...status, matchStatus }]} profiles={[]} locks={{ codex: { state: "free" } }}
    busy={false} onRefresh={vi.fn()} onRecoverLock={vi.fn()} />);

  expect(screen.getByRole("article", { name: "Codex 配置状态" })).toHaveTextContent(label);
  expect(screen.getByText(status.path)).toBeVisible();
});

it.each([
  { values: { readError: "文件访问被拒绝" }, label: "读取失败" },
  { values: { exists: false }, label: "未找到配置文件" },
  { values: { syntaxOk: false }, label: "语法错误" },
])("preserves $label without suggesting a successful profile match", ({ values, label }) => {
  render(<ConfigStatusPanel statuses={[{ ...status, ...values }]} profiles={[]} locks={{}}
    busy={false} onRefresh={vi.fn()} onRecoverLock={vi.fn()} />);

  expect(screen.getByText(label)).toBeVisible();
  if (values.readError) expect(screen.getByText(values.readError)).toBeVisible();
  expect(screen.queryByText("与档案「网关」一致")).not.toBeInTheDocument();
  expect(screen.getByText(status.path)).toBeVisible();
});

it("preserves scope warnings, switch history, and the recovery action on the affected client", async () => {
  const user = userEvent.setup();
  const onRecoverLock = vi.fn();
  const onRefresh = vi.fn();
  const route: RouteState = {
    app: "codex", routeMode: "custom", providerName: "网关", model: "gpt-fixture", baseUrl: "https://fixture.invalid",
    apiKey: "fixture-key", wireApi: "responses", codexModelOptions: null, haikuModel: null,
    sonnetModel: null, opusModel: null, availableModels: null, scopeWarnings: ["项目配置可能覆盖用户配置"],
  };
  const statuses: ConfigFileStatus[] = [{ ...status, route, lastSwitch: {
    app: "codex", profileId: "gateway", profileName: "网关", contentHash: "fixture-hash", backupId: "backup-one",
    at: "2026-09-08T00:00:00Z", operation: "projection",
  } }, { ...status, app: "claude" }];
  const props = { statuses, profiles: [], locks: { codex: { state: "stale" as const }, claude: { state: "held" as const, processName: "Claude Code" } },
    onRecoverLock, onRefresh };
  const { rerender } = render(<ConfigStatusPanel {...props} busy={false} />);

  const codex = screen.getByRole("article", { name: "Codex 配置状态" });
  expect(codex).toHaveTextContent("项目配置可能覆盖用户配置");
  expect(codex).toHaveTextContent("上次切换");
  expect(codex).not.toHaveTextContent("fixture-key");
  expect(screen.getByRole("article", { name: "Claude 配置状态" })).toHaveTextContent("写入锁由Claude Code持有");
  await user.click(within(codex).getByRole("button", { name: "清理遗留锁" }));
  expect(onRecoverLock).toHaveBeenCalledExactlyOnceWith("codex");
  await user.click(screen.getByRole("button", { name: "刷新状态" }));
  expect(onRefresh).toHaveBeenCalledOnce();
  rerender(<ConfigStatusPanel {...props} busy />);
  expect(screen.getByRole("button", { name: "清理遗留锁" })).toBeDisabled();
});
