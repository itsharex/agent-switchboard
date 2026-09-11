import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { providerParameters } from "../test/provider-parameters";
import { ProviderEditor } from "../test/provider-editor";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("ProviderEditor.official", () => {
  it("offers the access-mode choice only when creating and defaults to custom", () => {
    const onSave = vi.fn();
    const { rerender } = render(
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

    expect(screen.getByRole("radiogroup", { name: "接入方式" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "自定义 API 中继" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "官方登录" })).not.toBeChecked();
    expect(screen.getByLabelText("服务地址")).toBeInTheDocument();
    expect(screen.getByLabelText("API 密钥")).toBeInTheDocument();

    rerender(
      <ProviderEditor
        profile={{
          id: "existing",
          app: "codex",
          routeMode: "custom",
          name: "中继",
          model: null,
          baseUrl: "https://gateway.example/v1",
          apiKey: "sk-test",
          upstreamProtocol: "responses",
          responsesOptions: { requestMode: "standard" as const },
          maxOutputTokens: null,
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

    expect(screen.queryByRole("radiogroup", { name: "接入方式" })).toBeNull();
  });

  it("opens the existing official profile instead of creating a duplicate", async () => {
    const user = userEvent.setup();
    const onOpenOfficial = vi.fn();
    render(
      <ProviderEditor
        profile={null}
        initialApp="codex"
        busy={false}
        officialTakenApps={["codex"]}
        userConfigModel={null}
        onOpenOfficial={onOpenOfficial}
        onSave={vi.fn()}
        onCancel={() => {}}
      />,
    );

    expect(screen.getByRole("radio", { name: "自定义 API 中继" })).toBeEnabled();
    const official = screen.getByRole("radio", { name: "官方登录" });
    expect(official).toBeEnabled();
    await user.click(official);
    expect(onOpenOfficial).toHaveBeenCalledWith("codex");
  });

  it("hides custom fields in official mode and gates saving until the login completes", async () => {
    const invokeMock = vi.mocked(invoke);
    invokeMock.mockImplementation((command: string) => {
      if (command === "official_login_start") {
        return Promise.resolve({
          userCode: "CODE-1234",
          verificationUrl: "https://auth.openai.com/codex/device",
        });
      }
      if (command === "official_login_poll") {
        return Promise.resolve({
          phase: "completed",
          userCode: null,
          verificationUrl: "",
          message: null,
        });
      }
      return Promise.resolve([]);
    });
    vi.useFakeTimers();
    const onSave = vi.fn();

    try {
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

      fireEvent.click(screen.getByRole("radio", { name: "官方登录" }));

      expect(screen.getByLabelText("名称")).toHaveValue("Codex 官方登录");
      expect(screen.queryByLabelText("服务地址")).toBeNull();
      expect(screen.queryByLabelText("主模型")).toBeNull();
      expect(screen.queryByLabelText("API 密钥")).toBeNull();

      const save = screen.getByRole("button", { name: "保存供应商" });
      expect(save).toBeDisabled();

      fireEvent.click(screen.getByRole("button", { name: "开始官方登录" }));
      await act(async () => {});
      expect(screen.getByText(/验证码/)).toBeInTheDocument();
      expect(save).toBeDisabled();

      await act(async () => {
        vi.advanceTimersByTime(3000);
      });
      expect(screen.getByText("登录完成，登录凭据已写入客户端本地文件。")).toBeInTheDocument();
      expect(save).toBeEnabled();

      fireEvent.click(save);
      expect(onSave).toHaveBeenCalledWith(
        expect.objectContaining({
          app: "codex",
          routeMode: "official",
          name: "Codex 官方登录",
          model: null,
          baseUrl: null,
          apiKey: "",
          upstreamProtocol: null,
          responsesOptions: null,
          parameters: providerParameters("codex"),
          modelOptions: null,
          usageQuery: null,
        }),
      );
    } finally {
      vi.useRealTimers();
      invokeMock.mockReset();
    }
  });

  it("saves an edited official profile without demanding a fresh login", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <ProviderEditor
        profile={{
          id: "codex-official",
          app: "codex",
          routeMode: "official",
          name: "Codex 官方登录",
          model: null,
          baseUrl: null,
          apiKey: "",
          upstreamProtocol: null,
          responsesOptions: null,
          maxOutputTokens: null,
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

    expect(screen.queryByLabelText("服务地址")).toBeNull();
    expect(screen.getByRole("button", { name: "开始官方登录" })).toBeInTheDocument();

    const save = screen.getByRole("button", { name: "保存供应商" });
    await waitFor(() => expect(save).toBeEnabled());

    await user.clear(screen.getByLabelText("名称"));
    await user.type(screen.getByLabelText("名称"), "Codex 官方");
    await user.click(save);

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({ routeMode: "official", name: "Codex 官方" }),
    );
  });

  it("clears the custom-route fields the moment official mode is chosen", async () => {
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

    await user.type(screen.getByLabelText("名称"), "我的中继");
    await user.type(screen.getByLabelText("服务地址"), "https://gateway.example/v1");
    await user.type(screen.getByLabelText("API 密钥"), "sk-test-key");
    await user.click(screen.getByRole("radio", { name: "官方登录" }));

    expect(screen.getByLabelText("名称")).toHaveValue("我的中继");
    expect(screen.queryByLabelText("服务地址")).toBeNull();

    await user.click(screen.getByRole("radio", { name: "自定义 API 中继" }));

    expect(screen.getByLabelText("名称")).toHaveValue("我的中继");
    expect(screen.getByLabelText("服务地址")).toHaveValue("");
    expect(screen.getByLabelText("API 密钥")).toHaveValue("");
  });

  it("keeps an existing custom profile in the custom draft contract", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <ProviderEditor
        profile={{
          id: "legacy-official",
          app: "claude",
          routeMode: "custom",
          name: "Claude 中继",
          model: null,
          baseUrl: "https://relay.example",
          apiKey: "sk-test-key",
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

    expect(screen.getByLabelText("服务地址")).toHaveValue("https://relay.example");
    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({ baseUrl: "https://relay.example" }),
    );
  });
});
