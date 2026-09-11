import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { providerParameters } from "./test/provider-parameters";
import { invokeMock, statuses, profiles, defaultSettings, openProviderImport, runtimeOverview, primeBackend, ccScan, codexSeed } from "./test/app-fixtures";

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
          importedCount: 1,
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
    expect(screen.getByText(/无法导入：客户端 gemini 超出本应用支持范围/)).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("scan_ccswitch");

    // Codex rows are completed in the editor, so only Claude rows are
    // batch-selected and importable.
    expect(screen.getByRole("checkbox", { name: "中继 A" })).toBeChecked();
    expect(screen.queryByRole("checkbox", { name: "Codex 中继" })).not.toBeInTheDocument();
    expect(screen.queryByRole("checkbox", { name: "Codex 官方登录" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "导入所选 1 项" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("import_ccswitch_claude_profiles", {
        keys: ["claude:id-1"],
      }),
    );
    expect(await screen.findByRole("region", { name: "供应商工作区" })).toBeInTheDocument();
  });

  it("seeds a scanned Codex row into the editor instead of batch-importing it", async () => {
    primeBackend();
    invokeMock.mockImplementation((command: string) => {
      if (command === "runtime_overview") return Promise.resolve(runtimeOverview);
      if (command === "get_app_settings") return Promise.resolve(defaultSettings);
      if (command === "discover_cached") return Promise.resolve(null);
      if (command === "scan_ccswitch") return Promise.resolve(ccScan);
      if (command === "prepare_ccswitch_codex_seed") return Promise.resolve(codexSeed);
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
    await user.click(screen.getByRole("button", { name: "补全导入" }));

    expect(await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("prepare_ccswitch_codex_seed", { key: "codex:id-4" }),
    )).toBeTruthy();
    // The editor opens on the Codex list page, prefilled with the seed.
    const heading = await screen.findByRole("heading", { name: "新建 Codex 供应商" });
    expect(heading).toBeInTheDocument();
    expect((screen.getByLabelText("名称") as HTMLInputElement).value).toBe("Codex 中继");
    expect((screen.getByLabelText("服务地址") as HTMLInputElement).value).toBe("https://relay.codex.example/v1");
    expect(screen.getByText(/来自 CC Switch 的未导入字段/)).toBeInTheDocument();
  });
});
