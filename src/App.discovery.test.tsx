import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { providerParameters } from "./test/provider-parameters";
import { invokeMock, statuses, profiles, defaultSettings, openProviderImport, runtimeOverview, primeBackend, ccScan } from "./test/app-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ onResized: () => Promise.resolve(() => {}) }) }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(() => Promise.resolve(null)) }));

describe("App.discovery", () => {
  it("shows scan results as per-client status cards with route facts and in-card import", async () => {
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
            state: {
              kind: "ok",
              route: statuses[0].route,
              managed: true,
              warnings: ["存在托管键但未识别到供应商名称"],
              importable: false,
            },
          },
          claude: {
            app: "claude",
            path: "C:/Users/test/.claude/settings.json",
            exists: true,
            state: {
              kind: "ok",
              route: {
                ...statuses[1].route,
                routeMode: "custom",
                baseUrl: "https://relay.internal",
              },
              managed: false,
              warnings: ["settings.json 的 env 中存在明文 ANTHROPIC_AUTH_TOKEN"],
              importable: true,
            },
          },
          claudeImportProposals: [
            {
              draft: {
                app: "claude",
                name: "当前 Claude 配置",
                model: "claude-sonnet-4",
                baseUrl: "https://relay.internal",
                apiKey: "test-api-key",
                parameters: providerParameters("claude"),
                modelOptions: null,
              },
              basis: "由当前 Claude 配置的模型与服务地址生成",
            },
          ],
        });
      }
      if (command === "import_discovered_claude_profile") {
        return Promise.resolve(profiles[0]);
      }
      return Promise.resolve([]);
    });
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("radio", { name: "Claude" }));
    await openProviderImport(user);
    await user.click(screen.getByRole("button", { name: "扫描配置" }));

    const claudeCard = await screen.findByLabelText("Claude 扫描结果");
    expect(within(claudeCard).getByText("自定义服务 · claude-sonnet-4")).toBeInTheDocument();
    expect(within(claudeCard).getByText("未由本应用管理")).toBeInTheDocument();
    expect(within(claudeCard).getByText(/ANTHROPIC_AUTH_TOKEN/)).toBeInTheDocument();
    expect(within(claudeCard).getByText("由当前 Claude 配置的模型与服务地址生成")).toBeInTheDocument();

    await user.click(within(claudeCard).getByRole("button", { name: "导入供应商" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("import_discovered_claude_profile"),
    );
  });

  it("shows the previous scan from cache and relabels the action to refresh", async () => {
    primeBackend();
    invokeMock.mockImplementation((command: string) => {
      if (command === "config_status") return Promise.resolve(statuses);
      if (command === "runtime_overview") return Promise.resolve(runtimeOverview);
      if (command === "list_profiles") return Promise.resolve(profiles);
      if (command === "list_backups") return Promise.resolve([]);
      if (command === "lock_status") return Promise.resolve({ state: "free" });
      if (command === "get_app_settings") return Promise.resolve(defaultSettings);
      if (command === "discover_cached") {
        return Promise.resolve({
          codex: {
            app: "codex",
            path: "C:/Users/test/.codex/config.toml",
            exists: true,
            state: {
              kind: "ok",
              route: statuses[0].route,
              managed: true,
              warnings: [],
              importable: false,
            },
          },
          claude: {
            app: "claude",
            path: "C:/Users/test/.claude/settings.json",
            exists: false,
            state: { kind: "missing" },
          },
          claudeImportProposals: [],
        });
      }
      if (command === "discover_local") {
        return Promise.resolve({
          codex: {
            app: "codex",
            path: "C:/Users/test/.codex/config.toml",
            exists: true,
            state: {
              kind: "ok",
              route: statuses[0].route,
              managed: true,
              warnings: [],
              importable: false,
            },
          },
          claude: {
            app: "claude",
            path: "C:/Users/test/.claude/settings.json",
            exists: false,
            state: { kind: "missing" },
          },
          claudeImportProposals: [],
        });
      }
      return Promise.resolve([]);
    });
    const user = userEvent.setup();
    render(<App />);
    expect(invokeMock).toHaveBeenCalledWith("discover_cached");

    await user.click(screen.getByRole("radio", { name: "Claude" }));
    await openProviderImport(user);
    // The cached scan renders without any user scan in this session.
    const claudeCard = await screen.findByLabelText("Claude 扫描结果");
    expect(within(claudeCard).getByText("未找到配置文件")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "刷新配置" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "刷新配置" }));
    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("discover_local"));
  });

  it("scans CC Switch read-only, previews providers, and imports the Claude selection", async () => {
    primeBackend();
    invokeMock.mockImplementation((command: string) => {
      if (command === "runtime_overview") return Promise.resolve(runtimeOverview);
      if (command === "get_app_settings") return Promise.resolve(defaultSettings);
      if (command === "discover_cached") return Promise.resolve(null);
      if (command === "scan_ccswitch") return Promise.resolve(ccScan);
      if (command === "import_ccswitch_claude_profiles") {
        return Promise.resolve({
          importedCount: 3,
          usageScriptImportedCount: 1,
          skippedExisting: [],
          notImported: [],
        });
      }
      return Promise.resolve([]);
    });
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("radio", { name: "Claude" }));
    await openProviderImport(user);
    await user.click(screen.getByRole("radio", { name: "CC Switch" }));
    await user.click(screen.getByRole("button", { name: "扫描 CC Switch（只读）" }));

    expect(await screen.findByText("中继 A")).toBeInTheDocument();
    expect(screen.getByText(/将导入用量查询脚本/)).toBeInTheDocument();
    // Foreign-client rows are invisible and the official Codex row is an
    // importable checkbox, not a skip wall.
    expect(screen.queryByText(/无法导入/)).not.toBeInTheDocument();
    expect(screen.queryByText("双子")).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("scan_ccswitch");

    // Every importable row is batch-selected, across both stores.
    expect(screen.getByRole("checkbox", { name: "中继 A" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Codex 中继" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "Codex 官方登录" })).toBeChecked();

    await user.click(screen.getByRole("button", { name: "导入所选 3 项" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("import_ccswitch_claude_profiles", {
        keys: ["claude:id-1", "codex:id-4", "codex:id-2"],
      }),
    );
    expect(await screen.findByRole("region", { name: "供应商工作区" })).toBeInTheDocument();
  });

  it("imports scanned Codex rows in one click, official login included", async () => {
    primeBackend();
    invokeMock.mockImplementation((command: string) => {
      if (command === "runtime_overview") return Promise.resolve(runtimeOverview);
      if (command === "get_app_settings") return Promise.resolve(defaultSettings);
      if (command === "discover_cached") return Promise.resolve(null);
      if (command === "scan_ccswitch") return Promise.resolve(ccScan);
      if (command === "import_ccswitch_claude_profiles") {
        return Promise.resolve({
          importedCount: 2,
          usageScriptImportedCount: 0,
          skippedExisting: [],
          notImported: [],
        });
      }
      return Promise.resolve([]);
    });
    const user = userEvent.setup();
    render(<App />);

    await user.click(screen.getByRole("radio", { name: "Claude" }));
    await openProviderImport(user);
    await user.click(screen.getByRole("radio", { name: "CC Switch" }));
    await user.click(screen.getByRole("button", { name: "扫描 CC Switch（只读）" }));

    expect(await screen.findByText("Codex 中继")).toBeInTheDocument();
    expect(screen.getByText(/未导入: meta\.costMultiplier/)).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Codex 中继" })).toBeChecked();

    await user.click(screen.getByRole("button", { name: "导入所选 3 项" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("import_ccswitch_claude_profiles", {
        keys: ["claude:id-1", "codex:id-4", "codex:id-2"],
      }),
    );
    expect(await screen.findByRole("region", { name: "供应商工作区" })).toBeInTheDocument();
  });
});
