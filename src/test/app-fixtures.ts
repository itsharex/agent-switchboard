import { expect, vi } from "vitest";
import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import type * as client from "../api/client";
import type { ConfigWriteRecord, ConfigFileStatus, FilePreview, ProviderRecord, RuntimeLogEntry } from "../api/client";
import { providerParameters, providerParametersCatalog } from "./provider-parameters";

export const invokeMock = vi.mocked(invoke);

export const codexSwitch: ConfigWriteRecord = {
  app: "codex",
  profileId: "codex-gateway",
  profileName: "备用网关",
  contentHash: "hash-after",
  backupId: "b1",
  at: "2026-08-26T08:00:00Z",
  operation: "projection",
};

export const runtimeLogs: RuntimeLogEntry[] = [
  {
    at: "2026-08-26T08:00:00Z",
    level: "info",
    action: "configurationSwitched",
  },
];

export const statuses: ConfigFileStatus[] = [
  {
    app: "codex",
    path: "C:/Users/test/.codex/config.toml",
    exists: true,
    syntaxOk: true,
    route: {
      app: "codex",
      routeMode: "custom",
      providerName: "本机网关",
      model: "gpt-5.3-codex",
      baseUrl: "https://gateway.internal/v1",
      apiKey: "OPENAI_API_KEY",
      wireApi: "responses",
      codexModelOptions: null,
      haikuModel: null,
      sonnetModel: null,
      opusModel: null,
      availableModels: null,
      scopeWarnings: [],
    },
    readError: null,
    activeProfileId: null,
    matchStatus: { kind: "externallyModified", at: "2026-08-26T08:00:00Z" },
    lastSwitch: codexSwitch,
  },
  {
    app: "claude",
    path: "C:/Users/test/.claude/settings.json",
    exists: true,
    syntaxOk: true,
    route: {
      app: "claude",
      routeMode: "official",
      providerName: null,
      model: "claude-sonnet-4",
      baseUrl: null,
      apiKey: "test-api-key",
      wireApi: null,
      codexModelOptions: null,
      haikuModel: null,
      sonnetModel: null,
      opusModel: null,
      availableModels: null,
      scopeWarnings: [],
    },
    readError: null,
    activeProfileId: null,
    matchStatus: { kind: "unmanaged" },
    lastSwitch: null,
  },
];

export const profiles: ProviderRecord[] = [
  {
    profile: {
      id: "codex-gateway",
      app: "codex",
      routeMode: "custom",
      name: "备用网关",
      model: "gpt-5.4",
      baseUrl: "https://backup.internal/v1",
      apiKey: "OPENAI_API_KEY",
      upstreamProtocol: "responses",
      responsesOptions: { requestMode: "standard" as const },
      maxOutputTokens: null,
      parameters: providerParameters("codex"),
      modelOptions: null,
      websiteUrl: null,
    },
    fileHash: "provider-file-hash",
  },
];

export const filePreview: FilePreview = {
  contentHash: "hash1",
  renderedHash: "rendered-hash1",
  content: 'model = "gpt-5.4"\nthreads = 8\n',
  preview: {
    app: "codex",
    target: "C:/Users/test/.codex/config.toml",
    changes: [{ key: "model", kind: "set", before: "gpt-5.3-codex", after: "gpt-5.4" }],
    warnings: [],
    backupDir: "C:/Users/test/AppData/Roaming/Agent Switchboard/state/backups",
  },
};

export const defaultSettings = {
  closeBehavior: "hideToTray",
  theme: "system",
  motion: "system",
  alwaysOnTop: false,
  launchAtLogin: false,
  hardwareAcceleration: true,
  interfaceFont: "Noto Sans SC",
  runtimeLogLevel: "info",
  collapsedUsageIds: [] as string[],
};

export function targetFrom(args: unknown): "codex" | "claude" {
  if (
    typeof args === "object" &&
    args !== null &&
    "target" in args &&
    (args as { target?: unknown }).target === "claude"
  ) {
    return "claude";
  }
  return "codex";
}

export function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

export async function openSettingsSection(
  user: ReturnType<typeof userEvent.setup>,
  name: string,
) {
  await user.click(within(screen.getByRole("navigation", { name: "主导航" })).getByRole("button", { name: "设置" }));
  await user.click(within(screen.getByRole("navigation", { name: "设置分类" })).getByRole("button", { name }));
}

export async function openDiagnostics(user: ReturnType<typeof userEvent.setup>, name = "配置与环境") {
  await openSettingsSection(user, "诊断");
  await user.click(within(screen.getByRole("tablist", { name: "诊断内容" })).getByRole("tab", { name }));
}

export async function openProviderImport(user: ReturnType<typeof userEvent.setup>) {
  await user.click(within(screen.getByRole("navigation", { name: "主导航" })).getByRole("button", { name: "供应商" }));
  await user.click(screen.getByRole("button", { name: "导入" }));
}

export const runtimeOverview = {
  appVersion: "0.1.5",
  buildMode: "debug",
  platform: "windows",
  architecture: "x86_64",
  transport: { kind: "desktopProtocol" },
  appDataPath: "C:/Users/test/AppData/Roaming/Agent Switchboard",
} satisfies client.RuntimeOverview;

export function primeBackend(logEntries: RuntimeLogEntry[] = []) {
  invokeMock.mockImplementation((command: string, args?: unknown) => {
    switch (command) {
      case "config_status":
        return Promise.resolve(statuses);
      case "runtime_overview":
        return Promise.resolve(runtimeOverview);
      case "list_profiles":
        return Promise.resolve(profiles);
      case "list_backups":
        return Promise.resolve([]);
      case "list_runtime_logs":
        return Promise.resolve(logEntries);
      case "lock_status":
        return Promise.resolve({ state: "free" });
      case "get_app_settings":
        return Promise.resolve(defaultSettings);
      case "set_app_settings":
        return Promise.resolve((args as { settings: unknown }).settings);
      case "list_system_fonts":
        return Promise.resolve(["Microsoft YaHei", "Noto Sans SC"]);
      case "get_provider_parameters_catalog":
        return Promise.resolve(providerParametersCatalog(targetFrom(args)));
      case "get_client_settings_editor":
        return Promise.resolve({
          app: targetFrom(args),
          settings: { settings: { "tui.notifications": { mode: "automatic" } } },
          settingsHash: `${targetFrom(args)}-settings-hash`,
          groups: ["终端界面", "安全与审批", "隐私与数据"],
          specs: [
            {
              key: "tui.notifications",
              label: "桌面通知",
              group: "终端界面",
              control: "toggle",
              options: [],
            },
          ],
          directory: [],
        });
      case "save_client_settings":
        return Promise.resolve({
          settings: (args as { settings: unknown }).settings,
          settingsHash: "saved-settings-hash",
        });
      case "get_codex_subagent_settings":
        return Promise.resolve({
          app: "codex",
          settings: {
            enabled: { mode: "automatic" },
            maxConcurrentThreadsPerSession: { mode: "automatic" },
            interruptMessage: { mode: "automatic" },
          },
          configHash: "codex-subagent-config-hash",
          fileExists: true,
          deprecatedKeys: [],
        });
      case "preview_codex_subagent_settings_command":
        return Promise.resolve({
          app: "codex",
          target: "Codex 用户级配置",
          content: "[agents]\nenabled = true\n",
          configHash: "codex-subagent-config-hash",
          renderedHash: "codex-subagent-rendered-hash",
        });
      case "apply_codex_subagent_settings": {
        const plan = (args as { plan: { settings: unknown } }).plan;
        return Promise.resolve({
          app: "codex",
          settings: plan.settings,
          configHash: "codex-subagent-after-hash",
          fileExists: true,
          deprecatedKeys: [],
        });
      }
      case "get_global_prompt_document": {
        const target = targetFrom(args);
        return Promise.resolve({
          app: target,
          fileName: target === "codex" ? "AGENTS.md" : "CLAUDE.md",
          content: target === "codex" ? "# Codex global instructions\n" : "# Claude global instructions\n",
          contentHash: `${target}-prompt-hash`,
          exists: true,
        });
      }
      case "save_global_prompt_document": {
        const target = targetFrom(args);
        const payload = args as { content: string };
        return Promise.resolve({
          app: target,
          fileName: target === "codex" ? "AGENTS.md" : "CLAUDE.md",
          content: payload.content,
          contentHash: `${target}-saved-prompt-hash`,
          exists: true,
        });
      }
      case "preview_switch":
        return Promise.resolve(filePreview);
      case "execute_switch":
        return Promise.resolve({
          lock: { state: "free" },
          acquiredAt: "2026-08-26T08:00:00Z",
          changed: ["C:/Users/test/.codex/config.toml"],
          warnings: [],
          backup: {
            id: "b1",
            app: "codex",
            targetPath: "C:/Users/test/.codex/config.toml",
            backupPath: "C:/backups/config.toml.bak",
            createdAt: "2026-08-26T08:00:00Z",
            contentHash: "h",
            targetExisted: true,
            linkedBackupId: null,
            reason: "switch",
          },
          preview: filePreview.preview,
          recovery: { outcome: "not_needed" },
          finalHash: "hash-after",
        });
      case "prepare_profile_save":
        return Promise.resolve({ preparationId: "prepared-save", kind: "saveAndApply", preview: filePreview });
      case "commit_profile_save":
        return Promise.resolve(profiles[0]);
      case "discover_local":
        return Promise.resolve({ codex: {}, claude: {}, importProposals: [] });
      case "discover_cached":
        return Promise.resolve(null);
      case "window_is_maximized":
        return Promise.resolve(false);
      case "update_channel":
        return Promise.resolve("github");
      case "get_cached_codex_official_reset":
        return Promise.resolve(null);
      case "list_extensions":
        return Promise.resolve({
          generation: 1,
          items: [],
          projects: [],
          history: [],
          capabilities: [],
          recoveryRequired: [],
        });
      default:
        return Promise.resolve([]);
    }
  });
}

export const ccScan = {
  dbPath: "C:/Users/test/.cc-switch/cc-switch.db",
  providers: [
    {
      key: "claude:id-1",
      app: "claude",
      routeMode: "custom",
      name: "中继 A",
      model: "claude-x",
      baseUrl: "https://relay.internal",
      usageScriptImportable: true,
      usageScriptUpdatesExisting: false,
      warnings: [],
      existing: false,
    },
    {
      key: "codex:id-2",
      app: "codex",
      routeMode: "official",
      name: "Codex 官方登录",
      model: null,
      baseUrl: null,
      usageScriptImportable: false,
      usageScriptUpdatesExisting: false,
      warnings: [],
      existing: false,
    },
  ],
  skipped: [
    { key: "gemini:id-3", appType: "gemini", name: "双子", reason: "客户端 gemini 超出本应用支持范围" },
  ],
};

export function primeUsageCollapseBackend(settings: typeof defaultSettings) {
  const record: ProviderRecord = {
    profile: { ...profiles[0].profile, usageQuery: {
      kind: "declarative", url: "{{baseUrl}}/balance", remainingPath: "balance",
      usedPath: null, totalPath: null, refreshIntervalMinutes: 0, unit: "USD",
    } },
    fileHash: "provider-file-hash",
  };
  primeBackend();
  const backend = invokeMock.getMockImplementation();
  expect(backend).toBeDefined();
  invokeMock.mockImplementation((command: string, args?: unknown) => {
    if (command === "list_profiles") return Promise.resolve([record]);
    if (command === "get_app_settings") return Promise.resolve(settings);
    if (command === "query_profile_usage") return Promise.resolve({
      readings: [{ remaining: 18.5, used: 7, total: 25.5, unit: "USD" }], at: "2026-09-01T08:00:00Z",
    });
    return backend!(command, args as never);
  });
  invokeMock.mockClear();
}
