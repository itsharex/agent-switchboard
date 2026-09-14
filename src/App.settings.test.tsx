import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import type { invoke } from "@tauri-apps/api/core";
import {
  invokeMock,
  statuses,
  profiles,
  defaultSettings,
  openDiagnostics,
  runtimeOverview,
  primeBackend,
} from "./test/app-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ onResized: () => Promise.resolve(() => {}) }),
}));
vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn(() => Promise.resolve(null)),
}));

describe("App.settings", () => {
  it("saves the selected runtime-log threshold through the one app-settings path", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    await openDiagnostics(user, "运行日志");
    await screen.findByText("暂无应用运行日志");
    const levelControl = screen.getByRole("combobox", { name: "记录级别" });
    await waitFor(() => expect(levelControl).not.toBeDisabled());
    await user.click(levelControl);
    await user.click(screen.getByRole("option", { name: "静默" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_app_settings", {
        settings: {
          ...defaultSettings,
          runtimeLogLevel: "silent",
        },
      }),
    );
  });

  it("saves client preferences without a client-file write and invalidates supplier previews", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("radio", { name: "Claude" }));
    await user.click(await screen.findByRole("button", { name: "供应商" }));
    // The preview button itself selects the row (2026-09-12: the identity
    // bar is inert; only buttons select).
    await user.click(
      await screen.findByRole("button", { name: "预览 备用网关 变更" }),
    );
    const previewPanel = await screen.findByRole("region", {
      name: "变更预览",
    });
    expect(
      within(previewPanel).getAllByText("claude-3-7-sonnet").length,
    ).toBeGreaterThan(0);
    expect(
      within(previewPanel).getAllByText("claude-sonnet-4").length,
    ).toBeGreaterThan(0);

    expect(
      screen.queryByRole("button", { name: "通用设置" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "设置" }));
    await user.click(screen.getByRole("button", { name: "偏好设置" }));
    await user.click(
      within(screen.getByRole("radiogroup", { name: "桌面通知" })).getByRole(
        "radio",
        { name: "开启" },
      ),
    );
    expect(screen.getByText("有未保存修改")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "保存客户端设置" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_client_settings", {
        target: "claude",
        settings: {
          settings: { "tui.notifications": { mode: "explicit", value: true } },
        },
        expectedSettingsHash: "claude-settings-hash",
      }),
    );
    expect(invokeMock.mock.calls.map(([command]) => command)).not.toContain(
      "execute_switch",
    );

    // Saving client preferences invalidated the supplier preview, so the
    // switch entry point is gone until a new preview is generated.
    await user.click(screen.getByRole("button", { name: "供应商" }));
    expect(
      screen.queryByRole("region", { name: "变更预览" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "确认切换" }),
    ).not.toBeInTheDocument();
  });

  it("loads, applies, and saves complete application settings", async () => {
    // The hardware-acceleration section renders only on Windows.
    vi.stubGlobal("navigator", {
      userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64)",
    });
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "设置" }));
    expect(
      await screen.findByRole("radio", { name: "最小化到托盘" }),
    ).toBeChecked();
    expect(document.documentElement.dataset.theme).toBeUndefined();
    expect(document.documentElement.dataset.motion).toBeUndefined();

    await user.click(screen.getByRole("radio", { name: "深色" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_app_settings", {
        settings: {
          closeBehavior: "hideToTray",
          theme: "dark",
          motion: "system",
          alwaysOnTop: false,
          launchAtLogin: false,
          hardwareAcceleration: true,
          interfaceFont: "Noto Sans SC",
          runtimeLogLevel: "info",
          collapsedUsageIds: [],
        },
      }),
    );
    expect(document.documentElement.dataset.theme).toBe("dark");

    await user.click(screen.getByRole("switch", { name: "窗口始终置顶" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_app_settings", {
        settings: {
          closeBehavior: "hideToTray",
          theme: "dark",
          motion: "system",
          alwaysOnTop: true,
          launchAtLogin: false,
          hardwareAcceleration: true,
          interfaceFont: "Noto Sans SC",
          runtimeLogLevel: "info",
          collapsedUsageIds: [],
        },
      }),
    );

    await user.click(screen.getByRole("switch", { name: "启用硬件加速" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_app_settings", {
        settings: {
          closeBehavior: "hideToTray",
          theme: "dark",
          motion: "system",
          alwaysOnTop: true,
          launchAtLogin: false,
          hardwareAcceleration: false,
          interfaceFont: "Noto Sans SC",
          runtimeLogLevel: "info",
          collapsedUsageIds: [],
        },
      }),
    );

    await user.click(screen.getByRole("button", { name: "重启应用" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("restart_application"),
    );
    vi.unstubAllGlobals();
  });

  it("顶栏置顶钮经同一保存路径提交完整设置对象", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    const pin = await screen.findByRole("button", { name: "置顶窗口" });
    expect(pin.getAttribute("aria-pressed")).toBe("false");
    await user.click(pin);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_app_settings", {
        settings: {
          closeBehavior: "hideToTray",
          theme: "system",
          motion: "system",
          alwaysOnTop: true,
          launchAtLogin: false,
          hardwareAcceleration: true,
          interfaceFont: "Noto Sans SC",
          runtimeLogLevel: "info",
          collapsedUsageIds: [],
        },
      }),
    );
    expect(
      await screen.findByRole("button", { name: "取消置顶" }),
    ).toBeDefined();
  });

  it("keeps the applied appearance when saving a replacement setting fails", async () => {
    primeBackend();
    invokeMock.mockImplementation((command: string) => {
      if (command === "runtime_overview")
        return Promise.resolve(runtimeOverview);
      if (command === "get_app_settings") {
        return Promise.resolve({
          closeBehavior: "hideToTray",
          theme: "dark",
          motion: "reduce",
          alwaysOnTop: false,
          launchAtLogin: false,
          hardwareAcceleration: true,
          interfaceFont: "Noto Sans SC",
          runtimeLogLevel: "info",
          collapsedUsageIds: [],
        });
      }
      if (command === "set_app_settings")
        return Promise.reject({ message: "保存失败" });
      if (command === "config_status") return Promise.resolve(statuses);
      if (command === "list_profiles") return Promise.resolve(profiles);
      if (command === "list_backups") return Promise.resolve([]);
      if (command === "lock_status") return Promise.resolve({ state: "free" });
      if (command === "get_client_settings_editor") {
        return Promise.resolve({
          app: "codex",
          settings: { settings: {} },
          settingsHash: "settings-hash",
          groups: [],
          specs: [],
          directory: [],
        });
      }
      return Promise.resolve([]);
    });
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "设置" }));
    await waitFor(() =>
      expect(document.documentElement.dataset.theme).toBe("dark"),
    );
    expect(document.documentElement.dataset.motion).toBe("reduce");
    await user.click(screen.getByRole("radio", { name: "浅色" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("保存失败");
    expect(document.documentElement.dataset.theme).toBe("dark");
    expect(document.documentElement.dataset.motion).toBe("reduce");
  });

  it("界面字体经完整设置对象保存并立即应用到界面", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "设置" }));
    const trigger = await screen.findByRole("button", { name: "选择界面字体" });
    expect(trigger.textContent).toContain("Noto Sans SC");
    expect(
      document.documentElement.style.getPropertyValue("--asb-font-user"),
    ).toBe('"Noto Sans SC"');

    await user.click(trigger);
    await user.click(screen.getByRole("option", { name: /Microsoft YaHei/ }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_app_settings", {
        settings: {
          closeBehavior: "hideToTray",
          theme: "system",
          motion: "system",
          alwaysOnTop: false,
          launchAtLogin: false,
          hardwareAcceleration: true,
          interfaceFont: "Microsoft YaHei",
          runtimeLogLevel: "info",
          collapsedUsageIds: [],
        },
      }),
    );
    await waitFor(() =>
      expect(
        document.documentElement.style.getPropertyValue("--asb-font-user"),
      ).toBe('"Microsoft YaHei"'),
    );
  });

  it("设置加载失败时固定显示原因并支持重试", async () => {
    primeBackend();
    let failSettings = true;
    const backend = invokeMock.getMockImplementation();
    invokeMock.mockImplementation(
      (command: string, args?: Parameters<typeof invoke>[1]) => {
        if (command === "get_app_settings" && failSettings) {
          return Promise.reject({
            code: "app-settings-unavailable",
            message: "应用设置格式无效",
          });
        }
        return backend?.(command, args) ?? Promise.resolve([]);
      },
    );
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "设置" }));
    expect(
      await screen.findByText("设置加载失败：应用设置格式无效"),
    ).toBeInTheDocument();
    expect(screen.queryByText("加载中")).toBeNull();
    expect(screen.queryByRole("radiogroup", { name: "界面主题" })).toBeNull();

    failSettings = false;
    await user.click(screen.getByRole("button", { name: "重试" }));
    expect(
      await screen.findByRole("radiogroup", { name: "界面主题" }),
    ).toBeInTheDocument();
  });

  it("设置加载失败时一键修复以默认值重建设置", async () => {
    primeBackend();
    const defaults = {
      closeBehavior: "hideToTray",
      theme: "system",
      motion: "system",
      alwaysOnTop: false,
      launchAtLogin: false,
      hardwareAcceleration: true,
      interfaceFont: "Noto Sans SC",
      runtimeLogLevel: "info",
      collapsedUsageIds: [],
    };
    const backend = invokeMock.getMockImplementation();
    invokeMock.mockImplementation(
      (command: string, args?: Parameters<typeof invoke>[1]) => {
        if (command === "get_app_settings") {
          return Promise.reject({
            code: "app-settings-unavailable",
            message: "应用设置格式无效",
          });
        }
        if (command === "repair_app_settings") return Promise.resolve(defaults);
        return backend?.(command, args) ?? Promise.resolve([]);
      },
    );
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "设置" }));
    expect(
      await screen.findByText("设置加载失败：应用设置格式无效"),
    ).toBeInTheDocument();

    // Both the settings-page failure row and the global banner offer the
    // same repair; this test exercises the settings-page view of it.
    const failureRow = screen
      .getByText("设置加载失败：应用设置格式无效")
      .closest('[role="alert"]') as HTMLElement;
    await user.click(
      within(failureRow).getByRole("button", { name: "一键修复" }),
    );

    expect(
      await screen.findByRole("radiogroup", { name: "界面主题" }),
    ).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("repair_app_settings");
    expect(
      document.documentElement.style.getPropertyValue("--asb-font-user"),
    ).toBe('"Noto Sans SC"');
  });

  it("在任意页面显示设置失败横幅并支持一键修复", async () => {
    primeBackend();
    const defaults = {
      closeBehavior: "hideToTray" as const,
      theme: "system" as const,
      motion: "system" as const,
      alwaysOnTop: false,
      launchAtLogin: false,
      hardwareAcceleration: true,
      interfaceFont: "Noto Sans SC",
      runtimeLogLevel: "info" as const,
      collapsedUsageIds: [] as string[],
    };
    let settingsBroken = true;
    const backend = invokeMock.getMockImplementation();
    invokeMock.mockImplementation(
      (command: string, args?: Parameters<typeof invoke>[1]) => {
        if (command === "get_app_settings" && settingsBroken) {
          return Promise.reject({
            code: "app-settings-unavailable",
            message: "应用设置格式无效",
          });
        }
        if (command === "repair_app_settings") {
          settingsBroken = false;
          return Promise.resolve(defaults);
        }
        return backend?.(command, args) ?? Promise.resolve([]);
      },
    );
    const user = userEvent.setup();
    render(<App />);

    // The banner appears without leaving the default providers page, where
    // settings-backed actions (usage collapse) silently wait.
    expect(
      await screen.findByRole("alert", { name: "应用设置不可用" }),
    ).toHaveTextContent("应用设置不可用：应用设置格式无效");

    await user.click(screen.getByRole("button", { name: "一键修复" }));

    expect(invokeMock).toHaveBeenCalledWith("repair_app_settings");
    await waitFor(() =>
      expect(
        screen.queryByRole("alert", { name: "应用设置不可用" }),
      ).not.toBeInTheDocument(),
    );
  });
});
