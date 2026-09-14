import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "./App";
import * as client from "./api/client";
import { codexFilePreview, codexOfficialFilePreview, deferred, filePreview,
  invokeMock, primeBackend, statuses } from "./test/app-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ onResized: () => Promise.resolve(() => {}) }),
}));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(() => Promise.resolve(null)) }));

beforeEach(() => {
  vi.restoreAllMocks();
  invokeMock.mockReset();
  primeBackend();
});

const clients = [
  { label: "Codex", app: "codex", name: "备用网关", id: "codex-gateway", file: codexFilePreview },
  { label: "Claude", app: "claude", name: "备用网关", id: "claude-gateway", file: filePreview },
  { label: "Codex", app: "codex", name: "Codex 官方登录", id: "codex-official", file: codexOfficialFilePreview },
] as const;

function expectNoWrite() {
  expect(invokeMock.mock.calls.map(([command]) => command)).not.toContain("execute_switch");
}

for (const target of clients) describe(`${target.id} read-only preview`, () => {
  it("only toggles the what-if details, preserving the client confirmation contract", async () => {
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("radio", { name: target.label }));
    await user.click(await screen.findByRole("button", { name: `预览 ${target.name} 变更` }));
    const preview = await screen.findByRole("region", { name: "变更预览" });
    if (target.app === "codex") {
      expect(within(preview).getByRole("button", { name: "取消" })).toBeVisible();
      expect(within(preview).getByRole("button", { name: "确认切换" })).toBeVisible();
    } else {
      expect(within(preview).queryByRole("button")).not.toBeInTheDocument();
    }
    expect(within(preview).getByLabelText(`${target.file.preview.target} 配置预览`)).toBeVisible();
    expect(screen.queryByRole("dialog", { name: "确认切换" })).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("preview_switch", { profileId: target.id });
    expectNoWrite();

    await user.click(screen.getByRole("button", { name: `收起 ${target.name} 预览` }));
    expect(screen.queryByRole("region", { name: "变更预览" })).not.toBeInTheDocument();
    expectNoWrite();
  });

  it("refreshes the activation candidate without reviving a cancelled confirmation", async () => {
    let revision = 0;
    const previewMock = vi.spyOn(client, "previewSwitch").mockImplementation(async () => ({
      ...target.file, contentHash: `source-${++revision}`, renderedHash: `rendered-${revision}`,
    }));
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("radio", { name: target.label }));
    await user.click(await screen.findByRole("button", { name: `预览 ${target.name} 变更` }));
    await screen.findByRole("region", { name: "变更预览" });
    await user.click(screen.getByRole("button", { name: `启用 ${target.name}` }));
    expect(previewMock).toHaveBeenCalledTimes(2);
    expectNoWrite();
    if (target.app === "codex") {
      await user.click(within(await screen.findByRole("region", { name: "变更预览" }))
        .getByRole("button", { name: "取消" }));
      expect(screen.queryByRole("region", { name: "变更预览" })).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: `预览 ${target.name} 变更` }));
      await screen.findByRole("region", { name: "变更预览" });
      expect(screen.queryByRole("dialog", { name: "确认切换" })).not.toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: `启用 ${target.name}` }));
      expect(previewMock).toHaveBeenCalledTimes(4);
      expect(screen.queryByRole("dialog", { name: "确认切换" })).not.toBeInTheDocument();
      await user.click(within(await screen.findByRole("region", { name: "变更预览" }))
        .getByRole("button", { name: "确认切换" }));
    } else {
      const sheet = await screen.findByRole("dialog", { name: "确认切换" });
      await user.click(within(sheet).getByRole("button", { name: "取消" }));
      expect(screen.getByRole("region", { name: "变更预览" })).toBeVisible();
      await user.click(screen.getByRole("button", { name: `收起 ${target.name} 预览` }));
      await user.click(screen.getByRole("button", { name: `预览 ${target.name} 变更` }));
      await screen.findByRole("region", { name: "变更预览" });
      expect(screen.queryByRole("dialog", { name: "确认切换" })).not.toBeInTheDocument();
      expectNoWrite();

      await user.click(screen.getByRole("button", { name: `启用 ${target.name}` }));
      const nextSheet = await screen.findByRole("dialog", { name: "确认切换" });
      expect(previewMock).toHaveBeenCalledTimes(4);
      await user.click(within(nextSheet).getByRole("button", { name: "确认切换" }));
    }
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("execute_switch", {
      profileId: target.id, expectedHash: "source-4", expectedRenderedHash: "rendered-4", confirmWrite: true,
    }));
  });

  it("keeps the live-row action contract stable", async () => {
    vi.spyOn(client, "getConfigStatus").mockResolvedValue(statuses.map(status =>
      status.app === target.app ? { ...status, activeProfileId: target.id } : status));
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("radio", { name: target.label }));
    const row = await screen
      .findByText(target.name, { selector: ".asb-row-name" })
      .then((name) => name.closest("li"));
    expect(row).not.toBeNull();
    expect(row).toHaveClass("is-live");
    if (target.app === "codex") {
      expect(within(row!).queryByRole("button", { name: `启用 ${target.name}` })).not.toBeInTheDocument();
      expect(within(row!).getByRole("button", { name: `预览 ${target.name} 变更` })).toBeVisible();
    } else {
      const activate = within(row!).getByRole("button", { name: `启用 ${target.name}` });
      await user.click(activate);
      await screen.findByRole("dialog", { name: "确认切换" });
    }
    expectNoWrite();
  });

  it("does not open confirmation when activation finishes after leaving the workspace", async () => {
    const pending = deferred<client.FilePreview>();
    vi.spyOn(client, "previewSwitch").mockReturnValue(pending.promise);
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("radio", { name: target.label }));
    await user.click(await screen.findByRole("button", { name: `启用 ${target.name}` }));
    const navigation = within(screen.getByRole("navigation", { name: "主导航" }));
    await user.click(navigation.getByRole("button", { name: "设置" }));
    await act(async () => { pending.resolve(target.file); });
    await user.click(navigation.getByRole("button", { name: "供应商" }));
    expect(screen.queryByRole("dialog", { name: "确认切换" })).not.toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "变更预览" })).not.toBeInTheDocument();
    expectNoWrite();
  });
});
