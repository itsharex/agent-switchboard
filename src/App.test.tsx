import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import * as client from "./api/client";
import { invokeMock, runtimeLogs, statuses, profiles, defaultSettings, deferred, openDiagnostics, openSettingsSection, openProviderImport, runtimeOverview, primeBackend } from "./test/app-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ onResized: () => Promise.resolve(() => {}) }) }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(() => Promise.resolve(null)) }));

describe("App", () => {
  it("marks the upper-left product brand as beta", async () => {
    primeBackend();
    render(<App />);

    const badge = await screen.findByText("Beta");
    expect(badge).toHaveAttribute("aria-label", "Beta 版本");
    expect(badge.closest(".asb-topbar-brand")).toBeInTheDocument();
  });

  it("opens suppliers from the tray and releases its event listener", async () => {
    primeBackend();
    let navigate: (() => void) | undefined;
    const stop = vi.fn();
    const subscribe = vi.spyOn(client, "onTrayNavigate").mockImplementation(async (handler) => {
      navigate = handler;
      return stop;
    });
    const view = render(<App />);
    await waitFor(() => expect(navigate).toBeDefined());
    act(() => navigate?.());
    expect(await screen.findByRole("region", { name: "Codex 供应商" })).toBeInTheDocument();
    view.unmount();
    expect(stop).toHaveBeenCalledOnce();
    subscribe.mockRestore();
  });

  it("shows the tray window failure in the main recovery surface", async () => {
    primeBackend();
    let report: ((message: string) => void) | undefined;
    const stop = vi.fn();
    const subscribe = vi.spyOn(client, "onTrayError").mockImplementation(async (handler) => {
      report = handler;
      return stop;
    });
    const view = render(<App />);
    await waitFor(() => expect(report).toBeDefined());
    act(() => report?.("托盘窗口未完成初始化，请重试"));
    expect(await screen.findByText("托盘窗口未完成初始化，请重试")).toBeInTheDocument();
    view.unmount();
    expect(stop).toHaveBeenCalledOnce();
    subscribe.mockRestore();
  });

  it("opens the logs tab through the typed application-log command", async () => {
    primeBackend(runtimeLogs);
    const user = userEvent.setup();
    render(<App />);

    await screen.findByRole("region", { name: "Codex 供应商" });
    await openDiagnostics(user, "运行日志");

    expect(await screen.findByText("已切换配置")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("list_runtime_logs");
  });

  it("applies Codex subagent runtime controls through a separate preview and confirmation", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    await openSettingsSection(user, "偏好设置");
    await screen.findByText("子 agent 运行");
    const enabled = screen.getByRole("radiogroup", { name: "启用子 agent" });
    await user.click(within(enabled).getByRole("radio", { name: "开启" }));
    await user.click(screen.getByRole("button", { name: "生成写入预览" }));
    await screen.findByRole("button", { name: "应用子 agent 设置" });
    await user.click(screen.getByRole("button", { name: "应用子 agent 设置" }));
    const dialog = screen.getByRole("dialog", { name: "确认应用子 agent 设置" });
    await user.click(within(dialog).getByRole("button", { name: "确认应用" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("apply_codex_subagent_settings", {
      plan: {
        settings: {
          enabled: { mode: "explicit", value: true },
          maxConcurrentThreadsPerSession: { mode: "automatic" },
          interruptMessage: { mode: "automatic" },
        },
        expectedHash: "codex-subagent-config-hash",
        expectedTargetExisted: true,
        renderedHash: "codex-subagent-rendered-hash",
      },
      confirmWrite: true,
    }));
  });

  it("reports match state, last switch time, and scope in configuration diagnostics", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);
    await openDiagnostics(user);

    expect(await screen.findByText(/与上次切换.*不符，配置可能被外部修改/)).toBeInTheDocument();
    const lastSwitchRow = screen.getByText("上次切换").closest(".asb-status-row");
    expect(lastSwitchRow).toHaveTextContent("2026年08月26日 16：00");
    expect(screen.getAllByText("本机网关 · gpt-5.3-codex").length).toBeGreaterThan(0);
    expect(screen.getAllByText("官方登录 · claude-sonnet-4").length).toBeGreaterThan(0);
  });

  it("undoes the last switch through an explicit confirmation", async () => {
    primeBackend();
    invokeMock.mockImplementation((command: string) => {
      if (command === "config_status") return Promise.resolve(statuses);
      if (command === "runtime_overview") return Promise.resolve(runtimeOverview);
      if (command === "list_profiles") return Promise.resolve(profiles);
      if (command === "list_backups") return Promise.resolve([]);
      if (command === "lock_status") return Promise.resolve({ state: "free" });
      if (command === "get_app_settings") return Promise.resolve(defaultSettings);
      if (command === "backup_diff") {
        return Promise.resolve([
          { key: "model", kind: "set", before: "gpt-5.3", after: "gpt-5.4" },
        ]);
      }
      if (command === "undo_last_switch") {
        return Promise.resolve({
          preRestoreBackup: {
            id: "restore-b1",
            app: "codex",
            targetPath: "C:/Users/test/.codex/config.toml",
            backupPath: "C:/backups/config.toml.restore.bak",
            createdAt: "2026-08-26T08:01:00Z",
            contentHash: "hash-after",
            targetExisted: true,
            linkedBackupId: null,
            reason: "restore-precheck",
          },
          restoredHash: "hash-before",
          warnings: [],
        });
      }
      return Promise.resolve([]);
    });
    const user = userEvent.setup();
    render(<App />);

    await openSettingsSection(user, "备份与恢复");
    await user.click(await screen.findByRole("button", { name: "撤回上一次切换" }));
    const dialog = await screen.findByRole("dialog", { name: "撤回上一次切换" });
    const diff = await screen.findByLabelText("撤回后写入的差异");
    expect(within(diff).getByText("gpt-5.4")).toHaveClass("asb-diff-old");
    expect(within(diff).getByText("gpt-5.3")).toHaveClass("asb-diff-new");
    expect(within(dialog).getByRole("button", { name: "确认撤回" })).toBeEnabled();
    await user.click(screen.getByRole("button", { name: "确认撤回" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("undo_last_switch", {
        target: "codex",
        confirmWrite: true,
      }),
    );
    expect(await screen.findByText("已撤回上一次切换")).toBeInTheDocument();
  });

  it("keeps undo confirmation unavailable until the exact difference is ready", async () => {
    primeBackend();
    const pendingDiff = deferred<Awaited<ReturnType<typeof client.backupDiff>>>();
    const backend = invokeMock.getMockImplementation();
    expect(backend).toBeDefined();
    invokeMock.mockImplementation((command: string, args?: unknown) => {
      if (command === "backup_diff") return pendingDiff.promise;
      return backend!(command, args as never);
    });
    const user = userEvent.setup();
    render(<App />);

    await openSettingsSection(user, "备份与恢复");
    await user.click(await screen.findByRole("button", { name: "撤回上一次切换" }));
    const dialog = await screen.findByRole("dialog", { name: "撤回上一次切换" });
    expect(within(dialog).getByRole("button", { name: "确认撤回" })).toBeDisabled();
    expect(within(dialog).getByText("正在生成撤回后会写入的差异。")).toBeInTheDocument();

    await act(async () => {
      pendingDiff.resolve([]);
    });

    await waitFor(() =>
      expect(within(dialog).getByRole("button", { name: "确认撤回" })).toBeEnabled(),
    );
    expect(within(dialog).getByText("当前受管配置已与将恢复的备份一致。")).toBeInTheDocument();
  });

  it("cancels an undo confirmation without starting the restore transaction", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    await openSettingsSection(user, "备份与恢复");
    invokeMock.mockClear();
    await user.click(await screen.findByRole("button", { name: "撤回上一次切换" }));
    const dialog = await screen.findByRole("dialog", { name: "撤回上一次切换" });

    await user.click(within(dialog).getByRole("button", { name: "取消" }));

    expect(screen.queryByRole("dialog", { name: "撤回上一次切换" })).not.toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("undo_last_switch", expect.anything());
  });

  it("opens the backup folder via the backend command", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    await openSettingsSection(user, "备份与恢复");
    await user.click(await screen.findByRole("button", { name: "打开备份文件夹" }));

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("open_backup_dir"));
  });

  it("marks keyboard focus and clears it on pointer interaction", async () => {
    primeBackend();
    render(<App />);
    await screen.findByRole("button", { name: "供应商" });
    const root = document.documentElement;
    expect(root.dataset.focusSource).toBeUndefined();

    // Pointer interaction clears the keyboard modality marker used by actions.
    fireEvent.pointerDown(window);
    expect(root.dataset.focusSource).toBeUndefined();

    // Keyboard navigation marks the modality used by keyboard-only actions.
    fireEvent.keyDown(window, { key: "Tab" });
    expect(root.dataset.focusSource).toBe("key");
    fireEvent.keyDown(window, { key: "ArrowRight" });
    expect(root.dataset.focusSource).toBe("key");

    // Any pointer down hands the modality back to the pointer.
    fireEvent.pointerDown(window);
    expect(root.dataset.focusSource).toBeUndefined();
  });

  it("shows a local read failure instead of treating it as a missing file", async () => {
    primeBackend();
    invokeMock.mockImplementation((command: string) => {
      if (command === "config_status") return Promise.resolve(statuses);
      if (command === "runtime_overview") return Promise.resolve(runtimeOverview);
      if (command === "list_profiles") return Promise.resolve(profiles);
      if (command === "list_backups") return Promise.resolve([]);
      if (command === "lock_status") return Promise.resolve({ state: "free" });
      if (command === "get_app_settings") return Promise.resolve(defaultSettings);
      if (command === "discover_cached") return Promise.resolve(null);
      if (command === "discover_local") {
        return Promise.resolve({
          codex: {
            app: "codex",
            path: "C:/Users/test/.codex/config.toml",
            exists: true,
            state: { kind: "readError", message: "无法读取配置文件" },
          },
          claude: {
            app: "claude",
            path: "C:/Users/test/.claude/settings.json",
            exists: true,
            state: { kind: "readError", message: "无法读取配置文件" },
          },
          claudeImportProposals: [],
        });
      }
      return Promise.resolve([]);
    });
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("radio", { name: "Claude" }));
    await openProviderImport(user);
    await user.click(screen.getByRole("button", { name: "扫描配置" }));
    expect(await screen.findByText("无法读取配置文件")).toBeInTheDocument();
  });

  it("surfaces a config-status failure as a typed error", async () => {
    primeBackend();
    invokeMock.mockImplementation((command: string) => {
      if (command === "config_status") {
        return Promise.reject({ code: "read-current", message: "无法读取当前配置" });
      }
      if (command === "runtime_overview") return Promise.resolve(runtimeOverview);
      if (command === "get_app_settings") return Promise.resolve(defaultSettings);
      return Promise.resolve([]);
    });
    render(<App />);
    expect(await screen.findByRole("alert")).toHaveTextContent("无法读取当前配置");
  });

  it("re-exports commands as typed functions only", () => {
    for (const exported of Object.keys(client)) {
      expect(typeof client[exported as keyof typeof client]).toBe("function");
    }
  });
});
