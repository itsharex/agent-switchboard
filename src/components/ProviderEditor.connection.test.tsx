import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { providerParameters } from "../test/provider-parameters";
import { ProviderEditor, gatewayStatus } from "../test/provider-editor";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("ProviderEditor.connection", () => {
  it("derives credential delivery from the selected upstream protocol", async () => {
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

    expect(screen.getByText(/Bearer Token：以 Authorization: Bearer <API 密钥> 请求头发送密钥/)).toBeInTheDocument();

    await user.type(screen.getByLabelText("名称"), "跨协议供应商");
    await user.type(screen.getByLabelText("服务地址"), "https://relay.example");
    await user.type(screen.getByLabelText("API 密钥"), "test-key");
    const protocol = screen.getByRole("combobox", { name: "API 格式" });
    await user.click(protocol);
    expect(screen.getByRole("option", { name: /Anthropic Messages/ })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /Chat Completions/ })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /Responses/ })).toBeInTheDocument();
    await user.click(screen.getByRole("option", { name: /Chat Completions/ }));
    expect(screen.queryByRole("combobox", { name: "认证方式" })).not.toBeInTheDocument();
    expect(screen.getByText(/Bearer Token：以 Authorization: Bearer <API 密钥> 请求头发送密钥/)).toBeInTheDocument();

    await user.click(protocol);
    await user.click(screen.getByRole("option", { name: /Anthropic Messages/ }));
    expect(screen.getByText(/x-api-key：以 x-api-key: <API 密钥> 请求头发送密钥/)).toBeInTheDocument();
    expect(screen.queryByText(/Bearer Token：以 Authorization: Bearer <API 密钥> 请求头发送密钥/)).not.toBeInTheDocument();
    expect(screen.getByLabelText("最大输出 Token")).toHaveValue(8192);
    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        upstreamProtocol: "anthropicMessages",
        responsesOptions: null,
        maxOutputTokens: 8192,
      }),
    );
  });

  it("explains all Codex routes use the gateway and only translated protocols disable search", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockResolvedValue(gatewayStatus(31817));
    const user = userEvent.setup();
    try {
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

      expect(await screen.findByText(/先完成官方登录/)).toHaveTextContent("openai");
      expect(screen.queryByText(/网页搜索会关闭/)).not.toBeInTheDocument();

      const protocol = screen.getByRole("combobox", { name: "API 格式" });
      await user.click(protocol);
      await user.click(screen.getByRole("option", { name: /Anthropic Messages/ }));

      const warning = await screen.findByText(/本机协议网关 http:\/\/127\.0\.0\.1:31817/);
      expect(warning).not.toHaveTextContent("127.0.0.1:端口");
      expect(invokeMock).toHaveBeenCalledWith("gateway_status");
      expect(screen.getByText(/网页搜索会关闭/)).toBeInTheDocument();
    } finally {
      invokeMock.mockReset();
    }
  });

  it("explains the gateway rewrite for a cross-protocol Claude provider", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockResolvedValue(gatewayStatus(31818));
    const user = userEvent.setup();
    try {
      render(
        <ProviderEditor
          profile={null}
          initialApp="claude"
          busy={false}
          officialTakenApps={[]}
          userConfigModel={null}
          onSave={vi.fn()}
          onCancel={() => {}}
        />,
      );

      const protocol = screen.getByRole("combobox", { name: "API 格式" });
      await user.click(protocol);
      await user.click(screen.getByRole("option", { name: /Chat Completions/ }));

      const warning = await screen.findByText(/本机协议网关 http:\/\/127\.0\.0\.1:31818/);
      expect(warning).not.toHaveTextContent("127.0.0.1:端口");
      expect(screen.queryByText(/网页搜索/)).not.toBeInTheDocument();
    } finally {
      invokeMock.mockReset();
    }
  });

  it("does not display a placeholder port when the gateway status is unavailable", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockRejectedValue(new Error("gateway unavailable"));
    const user = userEvent.setup();
    try {
      render(
        <ProviderEditor
          profile={null}
          initialApp="claude"
          busy={false}
          officialTakenApps={[]}
          userConfigModel={null}
          onSave={vi.fn()}
          onCancel={() => {}}
        />,
      );

      await user.click(screen.getByRole("combobox", { name: "API 格式" }));
      await user.click(screen.getByRole("option", { name: /Chat Completions/ }));

      expect(await screen.findByText(/网关当前未在监听/)).toBeInTheDocument();
      expect(screen.queryByText(/127\.0\.0\.1:端口/)).not.toBeInTheDocument();
    } finally {
      invokeMock.mockReset();
    }
  });

  it("keeps all three protocol choices available when editing a Claude provider", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <ProviderEditor
        profile={{
          id: "claude-protocol-editor",
          app: "claude",
          routeMode: "custom",
          name: "Claude 中继",
          model: null,
          baseUrl: "https://relay.example",
          apiKey: "test-key",
          upstreamProtocol: "anthropicMessages",
          responsesOptions: null,
          maxOutputTokens: null,
          parameters: providerParameters("claude"),
          modelOptions: null,
          websiteUrl: null,
        }}
        initialApp="claude"
        busy={false}
        officialTakenApps={[]}
        userConfigModel={null}
        onSave={onSave}
        onCancel={() => {}}
      />,
    );

    const protocol = screen.getByRole("combobox", { name: "API 格式" });
    await user.click(protocol);
    expect(screen.getByRole("option", { name: /Anthropic Messages/ })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /Chat Completions/ })).toBeInTheDocument();
    expect(screen.getByRole("option", { name: /Responses/ })).toBeInTheDocument();
    await user.click(screen.getByRole("option", { name: /Chat Completions/ }));
    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        upstreamProtocol: "chatCompletions",
        responsesOptions: null,
        maxOutputTokens: null,
      }),
    );
  });

  it("requires and persists a positive output limit for an edited Codex-to-Anthropic route", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <ProviderEditor
        profile={{
          id: "codex-anthropic-editor",
          app: "codex",
          routeMode: "custom",
          name: "Anthropic 上游",
          model: "claude-sonnet-4-6",
          baseUrl: "https://relay.example",
          apiKey: "test-key",
          upstreamProtocol: "anthropicMessages",
          responsesOptions: null,
          maxOutputTokens: 16_384,
          parameters: providerParameters("codex"),
          modelOptions: null,
          websiteUrl: null,
        }}
        initialApp="codex"
        busy={false}
        officialTakenApps={[]}
        userConfigModel={null}
        onSave={onSave}
        onCancel={() => {}}
      />,
    );

    const limit = screen.getByLabelText("最大输出 Token");
    expect(limit).toHaveAttribute("required");
    expect(limit).toHaveValue(16_384);

    await user.clear(limit);
    expect(limit).toBeInvalid();
    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    expect(onSave).not.toHaveBeenCalled();

    await user.type(limit, "32768");
    expect(limit).toBeValid();
    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    expect(onSave).toHaveBeenLastCalledWith(
      expect.objectContaining({
        upstreamProtocol: "anthropicMessages",
        responsesOptions: null,
        maxOutputTokens: 32_768,
      }),
    );

    const protocol = screen.getByRole("combobox", { name: "API 格式" });
    await user.click(protocol);
    await user.click(screen.getByRole("option", { name: /Responses/ }));
    expect(screen.queryByLabelText("最大输出 Token")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    expect(onSave).toHaveBeenLastCalledWith(
      expect.objectContaining({
        upstreamProtocol: "responses",
        responsesOptions: { requestMode: "standard" as const },
        maxOutputTokens: null,
      }),
    );
  });
});
