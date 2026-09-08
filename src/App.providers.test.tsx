import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { primeUsageCollapseBackend } from "./test/app-fixtures";
import type { FilePreview, ProviderRecord } from "./api/client";
import { providerParameters } from "./test/provider-parameters";
import { invokeMock, defaultSettings, deferred, runtimeOverview, primeBackend } from "./test/app-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => ({ onResized: () => Promise.resolve(() => {}) }) }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: vi.fn(() => Promise.resolve(null)) }));

describe("App.providers", () => {
  it("loads actual status, renders lanes, and completes a confirmed switch", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    await waitFor(() => expect(screen.getByText("本机网关")).toBeInTheDocument());
    expect(screen.getByText("gpt-5.3-codex")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "供应商" }));
    await user.click(await screen.findByRole("option", { name: /备用网关/ }));
    // Selection alone shows no diff; the diff appears on explicit request.
    expect(screen.queryByRole("region", { name: "变更预览" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "预览 备用网关 变更" }));
    const previewPanel = await screen.findByRole("region", { name: "变更预览" });
    // The user-level configuration model shows twice here: the summary and
    // the diff's before-value.
    expect(within(previewPanel).getByText("当前用户级配置模型")).toBeInTheDocument();
    expect(within(previewPanel).getAllByText("gpt-5.3-codex").length).toBeGreaterThan(0);
    expect(within(previewPanel).getByText("gpt-5.4")).toBeInTheDocument();

    // The preview unfolds under the provider list, inside the same panel,
    // and the eye button retracts it (user decision 2026-08-28).
    expect(screen.getByRole("region", { name: "供应商工作区" })).toContainElement(previewPanel);
    await user.click(screen.getByRole("button", { name: "收起 备用网关 预览" }));
    expect(screen.queryByRole("region", { name: "变更预览" })).not.toBeInTheDocument();

    // The preview header's cancel button retracts it without switching.
    await user.click(screen.getByRole("button", { name: "预览 备用网关 变更" }));
    await screen.findByRole("region", { name: "变更预览" });
    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(screen.queryByRole("region", { name: "变更预览" })).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.map(([command]) => command)).not.toContain("execute_switch");

    await user.click(screen.getByRole("button", { name: "预览 备用网关 变更" }));
    await screen.findByRole("region", { name: "变更预览" });

    // The switch confirms from the provider page's inline preview.
    await user.click(screen.getByRole("button", { name: "确认切换" }));
    const sheet = await screen.findByRole("dialog", { name: "确认切换" });
    await user.click(within(sheet).getByRole("button", { name: "确认切换" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("execute_switch", {
        profileId: "codex-gateway",
        expectedHash: "hash1",
        expectedRenderedHash: "rendered-hash1",
        confirmWrite: true,
      }),
    );
    expect(await screen.findByText(/已切换到「备用网关」/)).toBeInTheDocument();
  });

  it("供应商用量面板的收起选择经应用设置持久化", async () => {
    primeUsageCollapseBackend(defaultSettings);
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "供应商" }));
    expect(await screen.findByRole("region", { name: "备用网关 用量" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "收起 备用网关 用量" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("set_app_settings", {
        settings: { ...defaultSettings, collapsedUsageIds: ["codex-gateway"] },
      }),
    );
    expect(await screen.findByRole("button", { name: "查看 备用网关 用量" })).toBeInTheDocument();
    expect(screen.queryByRole("region", { name: "备用网关 用量" })).not.toBeInTheDocument();
  });

  it("重启后保持用量收起状态并查询显示摘要", async () => {
    primeUsageCollapseBackend({ ...defaultSettings, collapsedUsageIds: ["codex-gateway"] });
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "供应商" }));
    const toggle = await screen.findByRole("button", { name: "查看 备用网关 用量" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("region", { name: "备用网关 用量" })).not.toBeInTheDocument();
    const usageQueries = invokeMock.mock.calls.filter(
      ([command]) => command === "query_profile_usage",
    );
    expect(usageQueries).toHaveLength(1);
    expect(await screen.findByLabelText("备用网关 用量摘要")).toHaveTextContent("余额 18.5 USD");
  });

  it("opens provider editing in a dedicated view and returns via the back affordance", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "供应商" }));
    await user.click(await screen.findByRole("option", { name: /备用网关/ }));
    await user.click(screen.getByRole("button", { name: "编辑 备用网关" }));

    // Dedicated view: focused title, back affordance, list hidden.
    expect(screen.getByRole("heading", { name: "编辑供应商" })).toBeInTheDocument();
    expect(screen.queryByRole("radiogroup", { name: "供应商客户端" })).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "返回供应商列表" }));
    expect(await screen.findByRole("radiogroup", { name: "供应商客户端" })).toBeInTheDocument();
  });

  it("confirms an active provider edit in place before applying it", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "供应商" }));
    await user.click(await screen.getByRole("option", { name: /备用网关/ }));
    await user.click(screen.getByRole("button", { name: "编辑 备用网关" }));
    fireEvent.change(screen.getByLabelText("服务地址"), {
      target: { value: "https://updated.internal/v1" },
    });
    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    expect(await screen.findByRole("dialog", { name: "确认保存并应用" })).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("prepare_profile_save", expect.objectContaining({
      profileId: "codex-gateway",
      expectedFileHash: "provider-file-hash",
    }));
    expect(invokeMock).not.toHaveBeenCalledWith("commit_profile_save", expect.anything());

    await user.click(screen.getByRole("button", { name: "确认保存并应用" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("commit_profile_save", expect.objectContaining({
        preparationId: "prepared-save",
        confirmWrite: true,
      })),
    );
  });

  it("offers a confirmed reset only for an unsupported profile store", async () => {
    primeBackend();
    let unsupported = true;
    const backend = invokeMock.getMockImplementation();
    expect(backend).toBeDefined();
    invokeMock.mockImplementation((command: string, args?: unknown) => {
      if (command === "list_profiles" && unsupported) {
        return Promise.reject({
          code: "profile-store-unsupported",
          message: "供应商存储格式无效或来自已不受支持的旧版本；请重新创建供应商档案",
        });
      }
      if (command === "reset_profile_store") {
        unsupported = false;
        return Promise.resolve(undefined);
      }
      return backend!(command, args as never);
    });
    const user = userEvent.setup();
    render(<App />);

    const alert = await screen.findByRole("alert", { name: "操作错误" });
    expect(alert).toHaveTextContent("供应商存储格式无效");
    const resetTrigger = screen.getByRole("button", { name: "清空旧档案并重新开始" });
    expect(invokeMock).not.toHaveBeenCalledWith("reset_profile_store", expect.anything());

    await user.click(resetTrigger);
    expect(screen.getByRole("dialog", { name: "清空旧供应商档案" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(invokeMock).not.toHaveBeenCalledWith("reset_profile_store", expect.anything());

    await user.click(screen.getByRole("button", { name: "清空旧档案并重新开始" }));
    await user.click(screen.getByRole("button", { name: "清空并重新开始" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("reset_profile_store", { confirmWrite: true }),
    );
    await waitFor(() => expect(screen.queryByRole("button", { name: "清空旧档案并重新开始" })).not.toBeInTheDocument());
  });

  it("does not offer a reset for an unreadable profile store", async () => {
    primeBackend();
    invokeMock.mockImplementation((command: string) => {
      if (command === "runtime_overview") return Promise.resolve(runtimeOverview);
      if (command === "list_profiles") {
        return Promise.reject({ code: "store-unreadable", message: "供应商存储不可读" });
      }
      if (command === "get_app_settings") return Promise.resolve(defaultSettings);
      return Promise.resolve([]);
    });
    render(<App />);

    await screen.findByRole("alert");
    expect(screen.queryByRole("button", { name: "清空旧档案并重新开始" })).not.toBeInTheDocument();
  });

  it("keeps the latest provider preview when an older in-flight request lands late", async () => {
    primeBackend();
    const twoProfiles: ProviderRecord[] = [
      {
        profile: {
          id: "codex-a",
          app: "codex",
          routeMode: "custom",
          name: "网关甲",
          model: "model-a",
          baseUrl: "https://a.internal/v1",
          apiKey: "KEY_A",
          upstreamProtocol: "responses",
          responsesOptions: { requestMode: "standard" as const },
          maxOutputTokens: null,
          parameters: providerParameters("codex"),
          modelOptions: null,
          websiteUrl: null,
        },
        fileHash: "a-hash",
      },
      {
        profile: {
          id: "codex-b",
          app: "codex",
          routeMode: "custom",
          name: "网关乙",
          model: "model-b",
          baseUrl: "https://b.internal/v1",
          apiKey: "KEY_B",
          upstreamProtocol: "responses",
          responsesOptions: { requestMode: "standard" as const },
          maxOutputTokens: null,
          parameters: providerParameters("codex"),
          modelOptions: null,
          websiteUrl: null,
        },
        fileHash: "b-hash",
      },
    ];
    const previewFor = (id: string, model: string): FilePreview => ({
      contentHash: `hash-${id}`,
      renderedHash: `rendered-${id}`,
      content: `model = "${model}"\n`,
      preview: {
        app: "codex",
        target: "C:/Users/test/.codex/config.toml",
        changes: [{ key: "model", kind: "set", before: "gpt-5.3-codex", after: model }],
        warnings: [],
        backupDir: "C:/backups",
      },
    });
    const pending = new Map<string, ReturnType<typeof deferred<FilePreview>>>();
    const backend = invokeMock.getMockImplementation();
    expect(backend).toBeDefined();
    invokeMock.mockImplementation((command: string, args?: unknown) => {
      if (command === "list_profiles") return Promise.resolve(twoProfiles);
      if (command === "preview_switch") {
        const profileId = (args as { profileId: string }).profileId;
        let entry = pending.get(profileId);
        if (!entry) {
          entry = deferred<FilePreview>();
          pending.set(profileId, entry);
        }
        return entry.promise;
      }
      return backend!(command, args as never);
    });
    invokeMock.mockClear();
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "供应商" }));
    await user.click(await screen.findByRole("button", { name: "预览 网关甲 变更" }));
    await user.click(screen.getByRole("button", { name: "预览 网关乙 变更" }));
    expect(pending.get("codex-a")).toBeDefined();
    expect(pending.get("codex-b")).toBeDefined();
    const switchCalls = invokeMock.mock.calls.filter(([command]) => command === "preview_switch");
    expect(switchCalls).toHaveLength(2);

    await act(async () => {
      pending.get("codex-b")!.resolve(previewFor("codex-b", "model-b"));
    });
    const panel = await screen.findByRole("region", { name: "变更预览" });
    expect(within(panel).getByText("model-b")).toBeInTheDocument();

    // The older request lands last; it belongs to a superseded selection.
    await act(async () => {
      pending.get("codex-a")!.resolve(previewFor("codex-a", "model-a"));
    });
    expect(within(panel).getByText("model-b")).toBeInTheDocument();
    expect(within(panel).queryByText("model-a")).not.toBeInTheDocument();
  });
});
