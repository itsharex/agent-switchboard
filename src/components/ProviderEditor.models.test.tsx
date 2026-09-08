import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { providerParameters } from "../test/provider-parameters";
import { ProviderEditor } from "../test/provider-editor";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("ProviderEditor.models", () => {
  it("maps the 1M context checkbox to the fixed context-window value", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <ProviderEditor
        profile={null}
        initialApp="codex"
        busy={false}
        officialTakenApps={[]}
        userConfigModel={null}
        onSave={onSave}
        onCancel={() => {}}
      />,
    );

    await user.click(screen.getByRole("button", { name: /配置运行参数/ }));
    const contextWindow = await screen.findByRole("checkbox", { name: "启用 1M 上下文窗口" });
    expect(contextWindow).not.toBeChecked();
    expect(screen.queryByRole("spinbutton")).toBeNull();

    await user.click(contextWindow);
    expect(contextWindow).toBeChecked();
    await user.click(screen.getByRole("button", { name: "返回供应商编辑" }));
    await user.type(screen.getByLabelText("名称"), "百万上下文网关");
    await user.type(screen.getByLabelText("服务地址"), "https://gateway.example/v1");
    await user.type(screen.getByLabelText("API 密钥"), "sk-test-codex");
    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        modelOptions: { kind: "codex", contextWindow: 1_000_000 },
      }),
    );

    onSave.mockClear();
    await user.click(screen.getByRole("button", { name: /配置运行参数/ }));
    await user.click(screen.getByRole("checkbox", { name: "启用 1M 上下文窗口" }));
    expect(screen.getByRole("checkbox", { name: "启用 1M 上下文窗口" })).not.toBeChecked();
    await user.click(screen.getByRole("button", { name: "返回供应商编辑" }));
    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ modelOptions: null }));
  });

  it("maps Claude Code 1M checkboxes to explicit semantic model state", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <ProviderEditor
        profile={null}
        initialApp="claude"
        busy={false}
        officialTakenApps={[]}
        userConfigModel={null}
        onSave={onSave}
        onCancel={() => {}}
      />,
    );

    await user.type(screen.getByLabelText("名称"), "百万上下文 Claude");
    await user.type(screen.getByLabelText("服务地址"), "https://relay.example");
    await user.type(screen.getByLabelText("API 密钥"), "sk-test-claude");
    await user.type(screen.getByLabelText("主模型"), "claude-opus-4-1");
    await user.type(screen.getByLabelText("Sonnet 档"), "claude-sonnet-4-6");
    await user.type(screen.getByLabelText("Opus 档"), "claude-opus-4-1");

    expect(screen.queryByRole("checkbox", { name: "Haiku 档启用 1M 上下文" })).toBeNull();
    const primaryOneM = screen.getByRole("checkbox", { name: "主模型启用 1M 上下文" });
    const sonnetOneM = screen.getByRole("checkbox", { name: "Sonnet 档启用 1M 上下文" });
    const opusOneM = screen.getByRole("checkbox", { name: "Opus 档启用 1M 上下文" });
    expect(primaryOneM).not.toBeChecked();
    expect(sonnetOneM).not.toBeChecked();
    expect(opusOneM).not.toBeChecked();

    await user.click(primaryOneM);
    await user.click(sonnetOneM);
    await user.click(opusOneM);

    expect(screen.getByLabelText("主模型")).toHaveValue("claude-opus-4-1");
    expect(screen.getByLabelText("Sonnet 档")).toHaveValue("claude-sonnet-4-6");
    expect(screen.getByLabelText("Opus 档")).toHaveValue("claude-opus-4-1");

    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        model: "claude-opus-4-1",
        modelOptions: {
          kind: "claude",
          primaryOneM: true,
          haikuModel: null,
          sonnetModel: "claude-sonnet-4-6",
          sonnetOneM: true,
          opusModel: "claude-opus-4-1",
          opusOneM: true,
          availableModels: null,
        },
      }),
    );
  });

  it("renders saved Claude 1M state through the matching checkboxes", () => {
    render(
      <ProviderEditor
        profile={{
          id: "claude-1m",
          app: "claude",
          routeMode: "custom",
          name: "已有 Claude 1M",
          model: "claude-opus-4-1",
          baseUrl: "https://relay.example",
          apiKey: "sk-test-claude",
          upstreamProtocol: "anthropicMessages",
          responsesOptions: null,
          maxOutputTokens: null,
          parameters: providerParameters("claude"),
          modelOptions: {
            kind: "claude",
            primaryOneM: true,
            haikuModel: "claude-haiku-4",
            sonnetModel: "claude-sonnet-4-6",
            sonnetOneM: true,
            opusModel: "claude-opus-4-1",
            opusOneM: false,
            availableModels: null,
          },
          websiteUrl: null,
        }}
        initialApp="claude"
        busy={false}
        officialTakenApps={[]}
        userConfigModel={null}
        onSave={vi.fn()}
        onCancel={() => {}}
      />,
    );

    expect(screen.getByLabelText("主模型")).toHaveValue("claude-opus-4-1");
    expect(screen.getByRole("checkbox", { name: "主模型启用 1M 上下文" })).toBeChecked();
    expect(screen.getByLabelText("Haiku 档")).toHaveValue("claude-haiku-4");
    expect(screen.getByLabelText("Sonnet 档")).toHaveValue("claude-sonnet-4-6");
    expect(screen.getByRole("checkbox", { name: "Sonnet 档启用 1M 上下文" })).toBeChecked();
  });

  it("drops an all-empty model-mapping block instead of storing it", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <ProviderEditor
        profile={null}
        initialApp="claude"
        busy={false}
        officialTakenApps={[]}
        userConfigModel={null}
        onSave={onSave}
        onCancel={() => {}}
      />,
    );

    await user.type(screen.getByLabelText("名称"), "中继 D");
    await user.type(screen.getByLabelText("服务地址"), "https://relay-d.example");
    await user.type(screen.getByLabelText("API 密钥"), "sk-test-key");
    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    expect(onSave).toHaveBeenCalledWith({
      app: "claude",
      routeMode: "custom",
      name: "中继 D",
      model: null,
      baseUrl: "https://relay-d.example",
      apiKey: "sk-test-key",
      upstreamProtocol: "anthropicMessages",
      responsesOptions: null,
      maxOutputTokens: null,
      notes: null,
      websiteUrl: null,
      usageQuery: null,
      officialQuotaRefreshIntervalMinutes: null,
      parameters: providerParameters("claude"),
      modelOptions: null,
    });
  });

  it("keeps an explicitly cleared Claude mapping so the next switch removes old tiers", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <ProviderEditor
        profile={null}
        initialApp="claude"
        busy={false}
        officialTakenApps={[]}
        userConfigModel={null}
        onSave={onSave}
        onCancel={() => {}}
      />,
    );

    await user.type(screen.getByLabelText("名称"), "清除旧映射");
    await user.type(screen.getByLabelText("服务地址"), "https://relay.example");
    await user.type(screen.getByLabelText("API 密钥"), "sk-test-key");
    await user.type(screen.getByLabelText("Haiku 档"), "claude-haiku-4");
    await user.clear(screen.getByLabelText("Haiku 档"));
    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    expect(onSave).toHaveBeenCalledWith({
      app: "claude",
      routeMode: "custom",
      name: "清除旧映射",
      model: null,
      baseUrl: "https://relay.example",
      apiKey: "sk-test-key",
      upstreamProtocol: "anthropicMessages",
      responsesOptions: null,
      maxOutputTokens: null,
      notes: null,
      websiteUrl: null,
      usageQuery: null,
      officialQuotaRefreshIntervalMinutes: null,
      parameters: providerParameters("claude"),
      modelOptions: {
        kind: "claude",
        primaryOneM: false,
        haikuModel: null,
        sonnetModel: null,
        sonnetOneM: false,
        opusModel: null,
        opusOneM: false,
        availableModels: null,
      },
    });
  });

  it("fetches the model list from the service address and fills the primary model", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockResolvedValue([
      { id: "gpt-5.2", ownedBy: "openai" },
      { id: "gpt-5.3-codex", ownedBy: null },
    ]);
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <ProviderEditor
        profile={null}
        initialApp="codex"
        busy={false}
        officialTakenApps={[]}
        userConfigModel={null}
        onSave={onSave}
        onCancel={() => {}}
      />,
    );

    await user.type(screen.getByLabelText("名称"), "本机网关");
    await user.type(screen.getByLabelText("服务地址"), "https://gateway.example/v1");
    await user.type(screen.getByLabelText("API 密钥"), "sk-test-key");
    await user.click(screen.getByRole("button", { name: "获取模型" }));

    const picker = await screen.findByRole("button", { name: "选择模型" });
    // The invoke keys must match the Rust command signature and the draft's
    // explicit protocol contract.
    expect(invokeMock).toHaveBeenCalledWith("fetch_provider_models", {
      request: {
        url: "https://gateway.example/v1",
        apiKey: "sk-test-key",
        upstreamProtocol: "responses",
      },
    });

    await user.click(picker);
    await user.click(await screen.findByRole("option", { name: "gpt-5.3-codex" }));
    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({ model: "gpt-5.3-codex", notes: null, websiteUrl: null, usageQuery: null }),
    );
    invokeMock.mockReset();
  });

  it("clears fetched models when the selected upstream protocol changes", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockResolvedValue([{ id: "gpt-5.3-codex", ownedBy: "openai" }]);
    const user = userEvent.setup();
    render(
      <ProviderEditor
        profile={null}
        initialApp="codex"
        busy={false}
        officialTakenApps={[]}
        userConfigModel={null}
        onSave={vi.fn()}
        onCancel={() => {}}
      />,
    );

    await user.type(screen.getByLabelText("服务地址"), "https://gateway.example/v1");
    await user.type(screen.getByLabelText("API 密钥"), "sk-test-key");
    await user.click(screen.getByRole("button", { name: "获取模型" }));
    expect(await screen.findByRole("button", { name: "选择模型" })).toBeInTheDocument();

    await user.click(screen.getByRole("combobox", { name: "API 格式" }));
    await user.click(screen.getByRole("option", { name: /Anthropic Messages/ }));
    expect(screen.queryByRole("button", { name: "选择模型" })).not.toBeInTheDocument();
    invokeMock.mockReset();
  });

  it("passes the entered API key to the model-list request", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockResolvedValue([{ id: "gpt-5.2", ownedBy: "openai" }]);
    const user = userEvent.setup();
    render(
      <ProviderEditor
        profile={null}
        initialApp="codex"
        busy={false}
        officialTakenApps={[]}
        userConfigModel={null}
        onSave={vi.fn()}
        onCancel={() => {}}
      />,
    );

    await user.type(screen.getByLabelText("名称"), "本机网关");
    await user.type(screen.getByLabelText("服务地址"), "https://gateway.example");
    await user.type(screen.getByLabelText("API 密钥"), "sk-entered-key");
    await user.click(screen.getByRole("button", { name: "获取模型" }));

    expect(await screen.findByRole("button", { name: "选择模型" })).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith(
      "fetch_provider_models",
      expect.objectContaining({
        request: {
          url: "https://gateway.example",
          apiKey: "sk-entered-key",
          upstreamProtocol: "responses",
        },
      }),
    );
    invokeMock.mockReset();
  });

  it("fills the Claude mapping tiers from the same fetched list", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockResolvedValue([
      { id: "claude-haiku-4-5", ownedBy: "anthropic" },
      { id: "deepseek-v4", ownedBy: "deepseek" },
    ]);
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <ProviderEditor
        profile={null}
        initialApp="claude"
        busy={false}
        officialTakenApps={[]}
        userConfigModel={null}
        onSave={onSave}
        onCancel={() => {}}
      />,
    );

    await user.type(screen.getByLabelText("名称"), "中继 B");
    await user.type(screen.getByLabelText("服务地址"), "https://relay.example");
    await user.type(screen.getByLabelText("API 密钥"), "sk-test-key");
    await user.click(screen.getByRole("button", { name: "获取模型" }));

    await user.click(await screen.findByRole("button", { name: "选择 Haiku 档模型" }));
    await user.click(await screen.findByRole("option", { name: "claude-haiku-4-5" }));
    await user.click(screen.getByRole("button", { name: "选择 Sonnet 档模型" }));
    await user.click(await screen.findByRole("option", { name: "deepseek-v4" }));

    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        modelOptions: expect.objectContaining({
          kind: "claude",
          haikuModel: "claude-haiku-4-5",
          sonnetModel: "deepseek-v4",
        }),
      }),
    );
    invokeMock.mockReset();
  });

  it("collects the Claude model tiers including the available list", async () => {    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <ProviderEditor
        profile={null}
        initialApp="claude"
        busy={false}
        officialTakenApps={[]}
        userConfigModel={null}
        onSave={onSave}
        onCancel={() => {}}
      />,
    );

    await user.type(screen.getByLabelText("名称"), "中继 C");
    await user.type(screen.getByLabelText("服务地址"), "https://relay-c.internal");
    await user.type(screen.getByLabelText("API 密钥"), "sk-test-key");
    await user.type(screen.getByLabelText("Haiku 档"), "claude-haiku-4");
    fireEvent.change(screen.getByLabelText("可选模型列表（每行一个）"), {
      target: { value: "claude-opus-4\nclaude-sonnet-4" },
    });
    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    expect(onSave).toHaveBeenCalledWith({
      app: "claude",
      routeMode: "custom",
      name: "中继 C",
      model: null,
      baseUrl: "https://relay-c.internal",
      apiKey: "sk-test-key",
      upstreamProtocol: "anthropicMessages",
      responsesOptions: null,
      maxOutputTokens: null,
      notes: null,
      websiteUrl: null,
      usageQuery: null,
      officialQuotaRefreshIntervalMinutes: null,
      parameters: providerParameters("claude"),
      modelOptions: {
        kind: "claude",
        primaryOneM: false,
        haikuModel: "claude-haiku-4",
        sonnetModel: null,
        sonnetOneM: false,
        opusModel: null,
        opusOneM: false,
        availableModels: ["claude-opus-4", "claude-sonnet-4"],
      },
    });
  });
});
