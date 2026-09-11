import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { providerParameters } from "../test/provider-parameters";
import { ProviderEditor } from "../test/provider-editor";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

describe("ProviderEditor", () => {
  it("hands the session to the other client's editor instead of swapping in place", async () => {
    const user = userEvent.setup();
    const onSwitchClient = vi.fn();
    render(
      <ProviderEditor
        profile={null}
        initialApp="claude"
        busy={false}
        officialTakenApps={[]}
        userConfigModel={null}
        onSwitchClient={onSwitchClient}
        onSave={vi.fn()}
        onCancel={() => {}}
      />,
    );

    const mark = screen.getByLabelText("客户端")
      .closest(".asb-client-control")
      ?.querySelector("img.asb-edit-logo");
    expect(mark).not.toBeNull();

    await user.click(screen.getByRole("combobox", { name: "客户端" }));
    await user.click(await screen.findByRole("option", { name: "Codex" }));

    expect(onSwitchClient).toHaveBeenCalledWith("codex");
    // The Claude draft stays mounted; the parent replaces the session.
    expect(
      screen.getByLabelText("客户端").closest(".asb-client-control")
        ?.querySelector("img.asb-edit-logo")
        ?.getAttribute("src"),
    ).toBe(mark?.getAttribute("src"));
  });

  it("places shared supplier testing outside the main-model field", () => {
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

    fireEvent.change(screen.getByLabelText("服务地址"), {
      target: { value: "https://relay.example/v1" },
    });
    const connectivityToggle = screen.getByRole("button", { name: "测试供应商" });
    expect(connectivityToggle.closest(".asb-model-actions")).toBeNull();
    expect(screen.getByLabelText("服务地址").parentElement).not.toContainElement(connectivityToggle);
    expect(screen.getByRole("button", { name: "获取模型" }).parentElement).toHaveClass(
      "asb-model-actions",
    );
    expect(screen.queryByRole("button", { name: "查询用量" })).not.toBeInTheDocument();
  });

  it("shows the user-level configuration model and its scope warning while editing", () => {
    const { rerender } = render(
      <ProviderEditor
        profile={null}
        initialApp="codex"
        busy={false}
        officialTakenApps={[]}
        userConfigModel="glm-4.6"
        userConfigWarnings={["使用 --profile 启动时会覆盖这里的用户级设置"]}
        onSave={vi.fn()}
        onCancel={() => {}}
      />,
    );

    expect(screen.getByText("当前用户级配置模型：glm-4.6")).toBeInTheDocument();
    expect(screen.getByText("使用 --profile 启动时会覆盖这里的用户级设置")).toBeInTheDocument();

    rerender(
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
    expect(screen.queryByText(/当前用户级配置模型/)).toBeNull();
  });

  it("preserves an existing usage query while saving other provider fields", async () => {
    const onSave = vi.fn();
    render(
      <ProviderEditor
        profile={{
          id: "profile-a",
          app: "codex",
          routeMode: "custom",
          name: "中继 A",
          model: null,
          baseUrl: "https://relay.example/v1",
          apiKey: "sk-test",
          upstreamProtocol: "responses",
          responsesOptions: { requestMode: "standard" as const },
          maxOutputTokens: null,
          parameters: providerParameters("codex"),
          modelOptions: null,
          websiteUrl: null,
          usageQuery: {
            kind: "declarative",
            url: "{{baseUrl}}/balance",
            remainingPath: "data/balance",
            refreshIntervalMinutes: 0,
          },
        }}
        initialApp="codex"
        busy={false}
        officialTakenApps={[]}
        userConfigModel={null}
        onSave={onSave}
        onCancel={() => {}}
      />,
    );

    fireEvent.change(screen.getByLabelText("名称"), { target: { value: "中继 A（更新）" } });
    await waitFor(() => expect(screen.getByRole("button", { name: "保存供应商" })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "保存供应商" }));
    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({
        usageQuery: {
          kind: "declarative",
          url: "{{baseUrl}}/balance",
          remainingPath: "data/balance",
          usedPath: null,
          totalPath: null,
          unit: null,
          refreshIntervalMinutes: 0,
        },
      }),
    );
  });

  it("reveals and hides the API key only on explicit action", async () => {
    const user = userEvent.setup();
    render(
      <ProviderEditor
        profile={{
          id: "secret-profile",
          app: "codex",
          routeMode: "custom",
          name: "密钥档案",
          model: null,
          baseUrl: "https://gateway.example/v1",
          apiKey: "sk-test-secret",
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
        onSave={vi.fn()}
        onCancel={() => {}}
      />,
    );

    const apiKey = screen.getByLabelText("API 密钥");
    expect(apiKey).toHaveAttribute("type", "password");
    const reveal = screen.getByRole("button", { name: "查看密钥" });
    expect(reveal).toHaveAttribute("aria-pressed", "false");

    await user.click(reveal);

    expect(apiKey).toHaveAttribute("type", "text");
    expect(apiKey).toHaveValue("sk-test-secret");
    const hide = screen.getByRole("button", { name: "隐藏密钥" });
    expect(hide).toHaveAttribute("aria-pressed", "true");

    await user.click(hide);

    expect(apiKey).toHaveAttribute("type", "password");
    expect(screen.getByRole("button", { name: "查看密钥" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("keeps metadata fields optional and submits them when filled", async () => {
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

    await user.type(screen.getByLabelText("名称"), "中继 E");
    await user.type(screen.getByLabelText("官网地址"), "https://provider.example");
    await user.type(screen.getByLabelText("备注"), "团队共用");
    await user.type(screen.getByLabelText("服务地址"), "https://relay-e.example");
    await user.type(screen.getByLabelText("API 密钥"), "sk-test-key");
    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    expect(onSave).toHaveBeenCalledWith(
      expect.objectContaining({ notes: "团队共用", websiteUrl: "https://provider.example" }),
    );
  });
});
