import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import * as client from "./api/client";
import {
  filePreview,
  invokeMock,
  openProviderImport,
  openSettingsSection,
  primeBackend,
  profiles,
  statuses,
} from "./test/app-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ onResized: () => Promise.resolve(() => {}) }),
}));
vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn(() => Promise.resolve(null)),
}));

beforeEach(() => {
  vi.restoreAllMocks();
  invokeMock.mockReset();
});

function primeActiveProvider() {
  primeBackend();
  vi.spyOn(client, "getConfigStatus").mockResolvedValue([
    { ...statuses[0], activeProfileId: profiles[0].profile.id },
    statuses[1],
  ]);
}

function navigation() {
  return within(screen.getByRole("navigation", { name: "主导航" }));
}

describe("workspace navigation", () => {
  it("opens the supplier workspace with exactly five named destinations", async () => {
    primeBackend();
    render(<App />);
    await screen.findByRole("region", { name: "Codex 当前连接" });
    expect(
      navigation()
        .getAllByRole("button")
        .map((button) => button.textContent),
    ).toEqual(["供应商", "扩展", "会话", "用量", "设置"]);
    expect(
      navigation().getByRole("button", { name: "供应商" }),
    ).toHaveAttribute("aria-current", "page");
    expect(
      navigation().queryByRole("button", {
        name: /概览|发现|备份|网关|日志|更多/,
      }),
    ).not.toBeInTheDocument();
    expect(
      invokeMock.mock.calls.some(
        ([command]) => command === "get_global_prompt_document",
      ),
    ).toBe(false);
  });

  it("opens the one client preference editor with the supplier client and a return path", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);
    await user.click(screen.getByRole("radio", { name: "Claude" }));
    await user.click(screen.getByRole("button", { name: "偏好设置" }));
    const picker = screen.getByRole("radiogroup", { name: "偏好设置客户端" });
    expect(within(picker).getByRole("radio", { name: "Claude" })).toBeChecked();
    await screen.findByText("桌面通知");
    expect(invokeMock).toHaveBeenCalledWith("get_client_settings_editor", {
      target: "claude",
    });
    await user.click(screen.getByRole("button", { name: "返回供应商" }));
    expect(
      within(
        screen.getByRole("radiogroup", { name: "供应商客户端" }),
      ).getByRole("radio", { name: "Claude" }),
    ).toBeChecked();
    await user.click(navigation().getByRole("button", { name: "设置" }));
    expect(screen.getByRole("region", { name: "偏好设置" })).toBeVisible();
  });

  it("saves preferences, requests a fresh complete preview, and still requires confirmation", async () => {
    primeActiveProvider();
    const previews = vi
      .spyOn(client, "previewSwitch")
      .mockResolvedValueOnce(filePreview)
      .mockResolvedValueOnce({
        ...filePreview,
        renderedHash: "fresh-rendered-hash",
      });
    const user = userEvent.setup();
    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: "预览 备用网关 变更" }),
    );
    await screen.findByRole("region", { name: "变更预览" });
    await user.click(screen.getByRole("button", { name: "偏好设置" }));
    await user.click(
      within(screen.getByRole("radiogroup", { name: "桌面通知" })).getByRole(
        "radio",
        { name: "开启" },
      ),
    );
    await user.click(screen.getByRole("button", { name: "保存并预览应用" }));
    await screen.findByRole("region", { name: "变更预览" });
    expect(previews).toHaveBeenCalledTimes(2);
    expect(
      invokeMock.mock.calls.some(
        ([command]) => command === "save_client_settings",
      ),
    ).toBe(true);
    expect(
      invokeMock.mock.calls.some(([command]) => command === "execute_switch"),
    ).toBe(false);
    await user.click(screen.getByRole("button", { name: "确认切换" }));
    const confirmation = await screen.findByRole("dialog", {
      name: "确认切换",
    });
    await user.click(
      within(confirmation).getByRole("button", { name: "确认切换" }),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("execute_switch", {
        profileId: profiles[0].profile.id,
        expectedHash: filePreview.contentHash,
        expectedRenderedHash: "fresh-rendered-hash",
        confirmWrite: true,
      }),
    );
  });

  it("keeps a failed preference draft in place and never advances to an apply preview", async () => {
    primeActiveProvider();
    vi.spyOn(client, "saveClientSettings").mockRejectedValue({
      code: "settings-conflict",
      message: "客户端偏好已在外部更改",
    });
    const preview = vi.spyOn(client, "previewSwitch");
    const user = userEvent.setup();
    render(<App />);
    await user.click(await screen.findByRole("button", { name: "偏好设置" }));
    await user.click(
      within(screen.getByRole("radiogroup", { name: "桌面通知" })).getByRole(
        "radio",
        { name: "开启" },
      ),
    );
    await user.click(screen.getByRole("button", { name: "保存并预览应用" }));
    await waitFor(() =>
      expect(
        screen.getAllByText(/客户端偏好已在外部更改/).length,
      ).toBeGreaterThan(0),
    );
    expect(navigation().getByRole("button", { name: "设置" })).toHaveAttribute(
      "aria-current",
      "page",
    );
    expect(
      within(screen.getByRole("radiogroup", { name: "桌面通知" })).getByRole(
        "radio",
        { name: "开启" },
      ),
    ).toBeChecked();
    expect(preview).not.toHaveBeenCalled();
  });

  it("leaves the import subview when client preferences request an application preview", async () => {
    primeActiveProvider();
    const user = userEvent.setup();
    render(<App />);
    await openProviderImport(user);
    await openSettingsSection(user, "偏好设置");
    await user.click(
      (await screen.findAllByRole("radio", { name: "开启" }))[0],
    );
    await user.click(screen.getByRole("button", { name: "保存并预览应用" }));
    expect(
      await screen.findByRole("region", { name: "变更预览" }),
    ).toBeVisible();
    expect(
      screen.queryByRole("region", { name: "导入供应商" }),
    ).not.toBeInTheDocument();
  });

  it("keeps a usage-query draft and blocks an application preview until that editor is closed", async () => {
    primeActiveProvider();
    vi.spyOn(client, "prepareProfileSave").mockResolvedValue({
      preparationId: "usage-save",
      kind: "saveOnly",
      preview: null,
    });
    const user = userEvent.setup();
    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: "配置 备用网关 用量" }),
    );
    fireEvent.change(screen.getByRole("textbox", { name: "用量查询地址" }), {
      target: { value: "{{baseUrl}}/unsaved" },
    });
    await openSettingsSection(user, "偏好设置");
    await screen.findByText("桌面通知");
    expect(
      screen.getByRole("button", { name: "保存并预览应用" }),
    ).toBeDisabled();
    expect(
      screen.getByText("请先保存或取消用量查询编辑，再预览应用。"),
    ).toBeVisible();
    await user.click(screen.getByRole("radio", { name: "Claude" }));
    await user.click(screen.getByRole("button", { name: "官方设置目录" }));
    await user.keyboard("{Escape}");
    await user.click(navigation().getByRole("button", { name: "供应商" }));
    expect(screen.getByRole("textbox", { name: "用量查询地址" })).toHaveValue(
      "{{baseUrl}}/unsaved",
    );
    fireEvent.change(screen.getByLabelText("余额提取路径"), {
      target: { value: "data/balance" },
    });
    await user.click(screen.getByRole("button", { name: "保存查询" }));
    const picker = await screen.findByRole("radiogroup", {
      name: "供应商客户端",
    });
    expect(within(picker).getByRole("radio", { name: "Codex" })).toBeChecked();
    expect(screen.getByRole("option", { name: /备用网关/ })).toHaveAttribute(
      "aria-selected",
      "true",
    );
  });

  it("retains the supplier edit target, revision and draft while another workspace changes clients", async () => {
    primeActiveProvider();
    const user = userEvent.setup();
    render(<App />);
    await user.click(
      await screen.findByRole("button", { name: "编辑 备用网关" }),
    );
    const name = screen.getByRole("textbox", { name: "名称" });
    await user.clear(name);
    await user.type(name, "未保存的供应商名称");
    await user.click(screen.getByRole("button", { name: /配置运行参数/ }));
    await openSettingsSection(user, "偏好设置");
    expect(
      screen.getByRole("button", { name: "保存并预览应用" }),
    ).toBeDisabled();
    await user.click(screen.getByRole("radio", { name: "Claude" }));
    await user.click(navigation().getByRole("button", { name: "供应商" }));
    expect(
      screen.getByRole("heading", { name: "运行参数" }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "返回供应商编辑" }));
    expect(screen.getByRole("textbox", { name: "名称" })).toHaveValue(
      "未保存的供应商名称",
    );
    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "prepare_profile_save",
        expect.objectContaining({
          profileId: profiles[0].profile.id,
          draft: expect.objectContaining({
            app: "codex",
            name: "未保存的供应商名称",
          }),
          expectedFileHash: profiles[0].fileHash,
        }),
      ),
    );
  });

  it("loads instruction documents only from the extension section and preserves its draft across navigation", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);
    await openSettingsSection(user, "偏好设置");
    await screen.findByText("桌面通知");
    expect(
      invokeMock.mock.calls.some(
        ([command]) => command === "get_global_prompt_document",
      ),
    ).toBe(false);
    await user.click(navigation().getByRole("button", { name: "扩展" }));
    await user.click(screen.getByRole("tab", { name: "全局指令" }));
    const editor = await screen.findByRole("textbox", {
      name: "AGENTS.md 内容",
    });
    await user.type(editor, "\n保留这份草稿");
    await user.click(navigation().getByRole("button", { name: "供应商" }));
    await user.click(navigation().getByRole("button", { name: "扩展" }));
    expect(screen.getByRole("tab", { name: "全局指令" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(
      screen.getByRole<HTMLTextAreaElement>("textbox", {
        name: "AGENTS.md 内容",
      }).value,
    ).toContain("保留这份草稿");
  });
});
