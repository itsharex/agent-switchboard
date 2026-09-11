import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import * as client from "../../api/client";
import type { CodexProviderRecord } from "../../api/client";
import { providerParameters, providerParametersCatalog } from "../../test/provider-parameters";
import { CodexProviderEditor } from "./CodexProviderEditor";
import { DEFAULT_CODEX_CAPABILITIES } from "./draft";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { invoke } from "@tauri-apps/api/core";
const invokeMock = vi.mocked(invoke);

function record(): CodexProviderRecord {
  return {
    profile: {
      id: "codex-profile-a",
      name: "中继 A",
      endpoint: "https://relay.example/v1",
      apiKey: "sk-test",
      upstream: "responses",
      requestMode: "standard",
      defaultModel: "relay-pro",
      catalog: [{
        id: "relay-pro",
        contextWindow: 128_000,
        maxOutputTokens: 8_192,
        functionTools: true,
        customTools: true,
        toolSearch: true,
        reasoning: true,
        defaultReasoningLevel: "high",
        supportedReasoningLevels: ["none", "high"],
        images: false,
        compact: true,
      }],
      modelRoutes: [{ clientModel: "relay-pro", upstreamModel: "vendor-pro" }],
      capabilities: { ...DEFAULT_CODEX_CAPABILITIES },
    },
    parameters: providerParameters("codex"),
    notes: "备注",
    websiteUrl: "https://example.com",
    usageQuery: null,
    fileHash: "hash-a",
  };
}

function mount(props: Partial<Parameters<typeof CodexProviderEditor>[0]> = {}) {
  return render(
    <CodexProviderEditor
      active
      source={null}
      busy={false}
      userConfigModel={null}
      userConfigWarnings={[]}
      onSave={vi.fn()}
      onSaveOfficial={vi.fn()}
      onSwitchAccessMode={vi.fn()}
      onCancel={vi.fn()}
      onSwitchClient={vi.fn()}
      {...props}
    />,
  );
}

beforeEach(() => {
  vi.spyOn(client, "getProviderParametersCatalog").mockImplementation(async (app) => providerParametersCatalog(app));
  vi.spyOn(client, "resolveProviderEndpoints").mockResolvedValue({
    requestUrl: "https://resolved-provider.example/v1/responses",
    modelsUrl: "https://resolved-provider.example/v1/models",
  });
  vi.spyOn(client, "getGatewayStatus").mockResolvedValue({
    configuredPort: 51234,
    listeningPort: 51234,
    baseUrl: "http://127.0.0.1:51234",
    status: "running",
    failure: null,
    repairReason: null,
    blockedRecovery: null,
    routes: [],
    metrics: { startedAtMs: 0, totalRequests: 0, failedRequests: 0, samples: [] },
  } as client.GatewayStatus);
});
afterEach(() => vi.restoreAllMocks());

async function fillIdentity(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByLabelText("名称"), "中继 B");
  await user.type(screen.getByLabelText("服务地址"), "https://relay.example/v1");
  await user.type(screen.getByLabelText("API 密钥"), "sk-new");
}

describe("CodexProviderEditor", () => {
  it("renders the same section layout as the Claude editor and no JSON textareas", async () => {
    mount();
    for (const name of ["基本资料", "连接配置", "模型", "连接测试"]) {
      expect(screen.getByRole("region", { name })).toBeInTheDocument();
    }
    // The notes field is the only textarea; the catalog, mapping, and
    // capability editors are structured controls.
    const textareas = document.querySelectorAll("textarea");
    expect(textareas).toHaveLength(1);
    expect(textareas[0]).toHaveAttribute("aria-label", "备注");
    expect(screen.queryByLabelText("模型目录 JSON")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("模型映射 JSON")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("能力声明 JSON")).not.toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("button", { name: "配置运行参数" })).toBeInTheDocument());
  });

  it("blocks saving an empty catalog with a readable problem", async () => {
    const onSave = vi.fn();
    mount({ onSave });
    await fillIdentity(userEvent.setup());
    expect(screen.getByText("模型目录不能为空；请获取模型或手动添加")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存供应商" })).toBeDisabled();
    expect(onSave).not.toHaveBeenCalled();
  });

  it("generates editable catalog rows from the fetched model list and saves the strict draft", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const fetchModels = vi.spyOn(client, "fetchProviderModels").mockResolvedValue([
      { id: "relay-pro", ownedBy: "relay" },
      { id: "relay-mini", ownedBy: null },
    ]);
    mount({ onSave });
    await fillIdentity(user);
    await user.click(screen.getByRole("button", { name: "获取模型" }));
    expect(fetchModels).toHaveBeenCalledWith("https://relay.example/v1", "sk-new", "responses");
    expect(screen.getByLabelText("模型标识 1")).toHaveValue("relay-pro");
    expect(screen.getByLabelText("模型标识 2")).toHaveValue("relay-mini");

    await user.click(screen.getByRole("combobox", { name: "默认模型" }));
    await user.click(await screen.findByRole("option", { name: "relay-pro" }));
    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const draft = onSave.mock.calls[0][0] as client.CodexProviderDraft;
    expect(draft.name).toBe("中继 B");
    expect(draft.endpoint).toBe("https://relay.example/v1");
    expect(draft.defaultModel).toBe("relay-pro");
    expect(draft.catalog.map((entry) => entry.id)).toEqual(["relay-pro", "relay-mini"]);
    expect(draft.catalog[0].contextWindow).toBeGreaterThan(0);
    expect(draft.capabilities).toEqual(DEFAULT_CODEX_CAPABILITIES);
    expect(draft.parameters).toEqual(providerParameters("codex"));
    expect(draft.notes).toBeNull();
  });

  it("keeps unknown-model limits empty and saves them with their defaults", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    vi.spyOn(client, "fetchProviderModels").mockResolvedValue([{ id: "relay-pro", ownedBy: null }]);
    mount({ onSave });
    await fillIdentity(user);
    await user.click(screen.getByRole("button", { name: "获取模型" }));

    const contextInput = screen.getByLabelText("relay-pro 上下文窗口");
    const outputInput = screen.getByLabelText("relay-pro 输出上限");
    expect((contextInput as HTMLInputElement).value).toBe("");
    expect(contextInput).toHaveAttribute("placeholder", "默认 128,000");
    expect(outputInput).toHaveAttribute("placeholder", "默认 8,192");

    await user.click(screen.getByRole("combobox", { name: "默认模型" }));
    await user.click(await screen.findByRole("option", { name: "relay-pro" }));
    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const draft = onSave.mock.calls[0][0] as client.CodexProviderDraft;
    expect(draft.catalog[0].contextWindow).toBe(128_000);
    expect(draft.catalog[0].maxOutputTokens).toBe(8_192);
  });

  it("narrows catalog rows and chat reasoning against the declared capabilities", async () => {
    const user = userEvent.setup();
    vi.spyOn(client, "fetchProviderModels").mockResolvedValue([{ id: "relay-pro", ownedBy: null }]);
    mount();
    await fillIdentity(user);
    await user.click(screen.getByRole("button", { name: "获取模型" }));

    await user.click(screen.getByText("供应商能力"));
    await user.click(screen.getByRole("checkbox", { name: "工具搜索" }));
    expect(screen.getByRole("checkbox", { name: "工具搜索" })).not.toBeChecked();
    // Entry flags follow the declaration instead of drifting out of range.
    const entryToolSearch = screen.getByRole("checkbox", { name: "relay-pro 工具搜索" });
    expect(entryToolSearch).not.toBeChecked();
    expect(entryToolSearch).toBeDisabled();
    expect(screen.getByText("Chat 推理参数仅 Chat Completions 上游且启用推理时可配置。")).toBeInTheDocument();
  });

  it("configures chat reasoning only on a Chat Completions upstream and resets it on protocol change", async () => {
    const user = userEvent.setup();
    mount();
    await user.click(screen.getByRole("combobox", { name: "API 格式" }));
    await user.click(await screen.findByRole("option", { name: "Chat Completions (/chat/completions)" }));
    await user.click(screen.getByText("供应商能力"));
    await user.click(screen.getByRole("radio", { name: "配置" }));
    expect(screen.getByRole("combobox", { name: "思考开关参数" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "推理力度参数" })).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: "力度映射" })).toBeInTheDocument();

    await user.click(screen.getByRole("combobox", { name: "API 格式" }));
    await user.click(await screen.findByRole("option", { name: "Responses (/responses)" }));
    expect(screen.queryByRole("combobox", { name: "思考开关参数" })).not.toBeInTheDocument();
    expect(screen.getByText("Chat 推理参数仅 Chat Completions 上游且启用推理时可配置。")).toBeInTheDocument();
  });

  it("shows the request-mode disclosure only for a Responses upstream", async () => {
    const user = userEvent.setup();
    mount();
    await user.click(screen.getByText("Responses 能力"));
    expect(screen.getByRole("combobox", { name: "请求模式" })).toBeInTheDocument();
    await user.click(screen.getByRole("combobox", { name: "API 格式" }));
    await user.click(await screen.findByRole("option", { name: "Anthropic Messages (/v1/messages)" }));
    expect(screen.queryByText("Responses 能力")).not.toBeInTheDocument();
    expect(screen.queryByRole("combobox", { name: "请求模式" })).not.toBeInTheDocument();
  });

  it("edits mapping rows and drops routes that referenced a deleted catalog model", async () => {
    const user = userEvent.setup();
    vi.spyOn(client, "fetchProviderModels").mockResolvedValue([{ id: "relay-pro", ownedBy: null }]);
    mount();
    await fillIdentity(user);
    await user.click(screen.getByRole("button", { name: "获取模型" }));
    await user.click(screen.getByText("模型映射"));
    await user.click(screen.getByRole("button", { name: "添加映射" }));
    await user.type(screen.getByLabelText("映射上游模型 1"), "vendor-pro");
    expect(screen.getByRole("combobox", { name: "映射客户端模型 1" })).toHaveTextContent("relay-pro");

    await user.click(screen.getByRole("button", { name: "删除模型 relay-pro" }));
    expect(screen.queryByLabelText("映射上游模型 1")).not.toBeInTheDocument();
    expect(screen.getByText("模型目录不能为空；请获取模型或手动添加")).toBeInTheDocument();
  });

  it("explains the always-on gateway route and the search loss on translated protocols", async () => {
    const user = userEvent.setup();
    mount();
    expect(await screen.findByText(/本机协议网关 http:\/\/127\.0\.0\.1:51234/)).toHaveTextContent("openai");
    expect(screen.queryByText(/网页搜索会关闭/)).not.toBeInTheDocument();

    await user.click(screen.getByRole("combobox", { name: "API 格式" }));
    await user.click(await screen.findByRole("option", { name: "Anthropic Messages (/v1/messages)" }));
    expect(screen.getByText(/网页搜索会关闭/)).toBeInTheDocument();
  });

  it("stores the selected subagent model and slider effort with its provider", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const fetchModels = vi.spyOn(client, "fetchProviderModels").mockResolvedValue([
      { id: "relay-coder", ownedBy: "Relay" },
    ]);
    mount({ source: { kind: "record", record: record() }, onSave });

    await user.click(screen.getByRole("button", { name: "配置运行参数" }));
    const modelMode = screen.getByRole("radiogroup", { name: "默认子 agent 模型配置方式" });
    await user.click(within(modelMode).getByRole("radio", { name: "指定" }));
    await user.click(screen.getByRole("button", { name: "获取模型" }));
    expect(fetchModels).toHaveBeenCalledWith("https://relay.example/v1", "sk-test", "responses");
    const picker = await screen.findByRole("button", { name: "选择默认子 agent 模型" });
    await user.click(picker);
    await user.click(screen.getByRole("option", { name: "relay-coder" }));
    fireEvent.change(screen.getByRole("slider", { name: "默认推理强度" }), { target: { value: "4" } });

    await user.click(screen.getByRole("button", { name: "返回供应商编辑" }));
    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    const draft = await waitFor(() => {
      expect(onSave).toHaveBeenCalled();
      return onSave.mock.calls[0][0] as client.CodexProviderDraft;
    });
    expect(draft.parameters.settings).toMatchObject({
      "agents.default_subagent_model": { mode: "explicit", value: "relay-coder" },
      "agents.default_subagent_reasoning_effort": { mode: "explicit", value: "high" },
    });
  });

  it("switching clients hands off instead of mutating the Codex draft", async () => {
    const user = userEvent.setup();
    const onSwitchClient = vi.fn();
    mount({ onSwitchClient });
    await user.click(screen.getByRole("combobox", { name: "客户端" }));
    await user.click(await screen.findByRole("option", { name: "Claude" }));
    expect(onSwitchClient).toHaveBeenCalledWith("claude");
  });

  it("fills from an existing record and saves the trimmed strict draft", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    mount({ source: { kind: "record", record: record() }, onSave });
    expect(screen.getByLabelText("名称")).toHaveValue("中继 A");
    expect(screen.getByLabelText("服务地址")).toHaveValue("https://relay.example/v1");
    expect(screen.getByLabelText("API 密钥")).toHaveValue("sk-test");
    expect(screen.getByLabelText("模型标识 1")).toHaveValue("relay-pro");
    const summary = screen.getByText("模型映射").closest("details");
    expect(summary).toHaveAttribute("open");
    expect(screen.getByLabelText("映射上游模型 1")).toHaveValue("vendor-pro");

    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const draft = onSave.mock.calls[0][0] as client.CodexProviderDraft;
    expect(draft.name).toBe("中继 A");
    expect(draft.notes).toBe("备注");
    expect(draft.websiteUrl).toBe("https://example.com");
    expect(draft.catalog).toHaveLength(1);
    expect(draft.modelRoutes).toEqual([{ clientModel: "relay-pro", upstreamModel: "vendor-pro" }]);
  });

  it("fills from a CC Switch seed, surfaces its warnings, and saves as a new profile", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    mount({
      source: {
        kind: "seed",
        seedKey: "codex:id-4",
        seed: {
          name: "Codex 中继",
          endpoint: "https://relay.codex.example/v1",
          apiKey: "sk-relay",
          upstream: "chatCompletions",
          requestMode: "standard",
          defaultModel: "gpt-5-codex",
          catalog: [{
            model: "gpt-5-codex",
            contextWindow: 272_000,
            images: true,
            defaultReasoningLevel: "high",
            reasoningLevels: ["low", "medium", "high"],
          }],
          parameters: providerParameters("codex"),
          notes: "从 CC Switch 导入",
          websiteUrl: null,
          usageQuery: null,
          warnings: ["未导入: meta.costMultiplier"],
        },
      },
      onSave,
    });
    expect(screen.getByRole("heading", { name: "新建 Codex 供应商" })).toBeInTheDocument();
    expect(screen.getByText(/来自 CC Switch 的未导入字段/)).toBeInTheDocument();
    expect(screen.getByText(/未导入: meta\.costMultiplier/)).toBeInTheDocument();
    expect(screen.getByLabelText("名称")).toHaveValue("Codex 中继");
    expect(screen.getByLabelText("服务地址")).toHaveValue("https://relay.codex.example/v1");
    expect(screen.getByLabelText("API 密钥")).toHaveValue("sk-relay");
    expect(screen.getByLabelText("模型标识 1")).toHaveValue("gpt-5-codex");
    expect(screen.getByLabelText("gpt-5-codex 上下文窗口")).toHaveValue(272_000);
    expect(screen.getByRole("combobox", { name: "gpt-5-codex 默认推理档位" })).toHaveTextContent("高");

    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    await waitFor(() => expect(onSave).toHaveBeenCalled());
    const draft = onSave.mock.calls[0][0] as client.CodexProviderDraft;
    expect(draft.name).toBe("Codex 中继");
    expect(draft.notes).toBe("从 CC Switch 导入");
    expect(draft.upstream).toBe("chatCompletions");
    expect(draft.defaultModel).toBe("gpt-5-codex");
    expect(draft.catalog[0]).toMatchObject({
      id: "gpt-5-codex",
      contextWindow: 272_000,
      images: true,
      defaultReasoningLevel: "high",
      supportedReasoningLevels: ["low", "medium", "high"],
    });
    expect(draft.modelRoutes).toEqual([]);
  });

  it("prepares a real request from the draft through the shared protocol contract", async () => {
    const user = userEvent.setup();
    invokeMock.mockReset();
    invokeMock.mockImplementation(async (command, args) => {
      if (command === "prepare_provider_request") {
        const connection = (args as { target: { connection: { baseUrl: string; defaultModel: string } } }).target.connection;
        return { requestId: "token-1", endpoint: `${connection.baseUrl}/responses`,
          upstreamProtocol: "responses", defaultModel: connection.defaultModel, prompt: "连接测试" };
      }
      if (command === "execute_provider_request") return { outcome: "success", reply: "草稿连接成功",
        status: 200, latencyMs: 10, model: "relay-pro", diagnostic: null, error: null, at: "2026-09-11T05:00:00Z" };
      if (command === "fetch_provider_request_models") return [{ id: "relay-pro", ownedBy: null }];
      if (command === "cancel_provider_request") return true;
      throw new Error(`Unexpected command ${command}`);
    });
    mount({ source: { kind: "record", record: record() } });
    await user.click(screen.getByRole("button", { name: "测试供应商" }));
    await user.click(screen.getByRole("radio", { name: "真实请求" }));
    await waitFor(() => expect(invokeMock.mock.calls.find(([command]) => command === "prepare_provider_request")).toEqual([
      "prepare_provider_request",
      { target: {
        kind: "draft",
        connection: {
          baseUrl: "https://relay.example/v1",
          apiKey: "sk-test",
          upstreamProtocol: "responses",
          responsesOptions: { requestMode: "standard" },
          defaultModel: "relay-pro",
        },
      } },
    ]));
    const panel = within(screen.getByRole("region", { name: "当前草稿 供应商测试" }));
    expect(panel.getByRole("radio", { name: "连通性测试" })).toBeInTheDocument();
  });
});
