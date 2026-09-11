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
    await user.click(screen.getByRole("option", { name: /Responses/ }));
    expect(screen.getByText(/Bearer Token：以 Authorization: Bearer <API 密钥> 请求头发送密钥/)).toBeInTheDocument();

    await user.type(screen.getByLabelText("名称"), "跨协议供应商");
    await user.type(screen.getByLabelText("服务地址"), "https://relay.example");
    await user.type(screen.getByLabelText("API 密钥"), "test-key");
    await user.click(protocol);
    await user.click(screen.getByRole("option", { name: /Chat Completions/ }));
    expect(screen.queryByRole("combobox", { name: "认证方式" })).not.toBeInTheDocument();
    expect(screen.getByText(/Bearer Token：以 Authorization: Bearer <API 密钥> 请求头发送密钥/)).toBeInTheDocument();

    await user.click(protocol);
    await user.click(screen.getByRole("option", { name: /Anthropic Messages/ }));
    expect(screen.getByText(/x-api-key：以 x-api-key: <API 密钥> 请求头发送密钥/)).toBeInTheDocument();
    expect(screen.queryByText(/Bearer Token：以 Authorization: Bearer <API 密钥> 请求头发送密钥/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        upstreamProtocol: "anthropicMessages",
        responsesOptions: null,
        maxOutputTokens: null,
      }),
    );
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
});
