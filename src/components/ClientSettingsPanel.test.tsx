import { useState, type ComponentProps } from "react";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { AppKind, ConfigFileStatus } from "../api/client";
import type { ClientSettingsEditorState } from "../app/useClientSettings";
import { ClientSettingsPanel } from "./ClientSettingsPanel";

const cleanSettings: ClientSettingsEditorState = {
  phase: "clean",
  editor: {
    app: "codex",
    settings: { settings: { "tui.notifications": { mode: "automatic" } } },
    settingsHash: "settings-hash",
    groups: ["终端界面"],
    specs: [
      {
        key: "tui.notifications",
        label: "桌面通知",
        group: "终端界面",
        control: "toggle",
        options: [],
      },
    ],
    directory: [
      {
        title: "桌面通知",
        paths: ["tui.notifications"],
        disposition: "direct",
        detail: "通过客户端偏好编辑。",
      },
      {
        title: "MCP 服务器",
        paths: ["mcp_servers.<id>"],
        disposition: "separateModule",
        detail: "通过扩展工作区管理。",
      },
    ],
  },
  draft: { "tui.notifications": { mode: "automatic" } },
};

const appliedStatus: ConfigFileStatus = {
  app: "codex",
  path: "C:/Users/test/.codex/config.toml",
  exists: true,
  syntaxOk: true,
  route: null,
  readError: null,
  activeProfileId: "gateway",
  matchStatus: {
    kind: "matchesProfile",
    profileId: "gateway",
    profileName: "网关",
  },
  lastSwitch: null,
};

type PanelProps = ComponentProps<typeof ClientSettingsPanel>;

function SettingsHarness(
  props: Partial<Omit<PanelProps, "app" | "onSelectApp">>,
) {
  const [app, setApp] = useState<AppKind>("codex");
  return (
    <ClientSettingsPanel
      key={app}
      app={app}
      onSelectApp={setApp}
      editorState={cleanSettings}
      configStatus={undefined}
      busy={false}
      hasActiveProvider
      previewBlockedReason={null}
      onValueChange={vi.fn()}
      onResetGroup={vi.fn()}
      onSave={vi.fn()}
      onSaveAndPreview={vi.fn()}
      onOpenProviders={vi.fn()}
      onRetryLoad={vi.fn()}
      onPreview={vi.fn()}
      {...props}
    />
  );
}

describe("ClientSettingsPanel", () => {
  it("shows preferences directly and opens the official directory as dismissible help", async () => {
    const user = userEvent.setup();
    render(<SettingsHarness configStatus={appliedStatus} />);

    expect(screen.queryByRole("tablist")).not.toBeInTheDocument();
    expect(screen.getByText("桌面通知")).toBeVisible();
    expect(screen.getByText(/不会修改、切换或覆盖任何供应商/)).toBeVisible();
    expect(screen.getByText("已应用：真实配置与「网关」一致")).toBeVisible();
    expect(
      within(
        screen.getByRole("radiogroup", { name: "偏好设置客户端" }),
      ).getByRole("radio", { name: "Codex" }),
    ).toBeChecked();

    await user.click(screen.getByRole("button", { name: "官方设置目录" }));
    expect(
      screen.getByRole("region", { name: "官方设置目录" }),
    ).toHaveTextContent("MCP 服务器");
    expect(
      screen.queryByRole("button", { name: "保存客户端设置" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "返回偏好设置" }),
    ).toHaveAttribute("aria-expanded", "true");
    expect(
      screen.queryByRole("textbox", { name: "AGENTS.md 内容" }),
    ).not.toBeInTheDocument();

    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("region", { name: "官方设置目录" }),
    ).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "官方设置目录" })).toHaveFocus();
    expect(screen.getByText("桌面通知")).toBeVisible();
  });

  it("closes directory help when switching the shared client", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <SettingsHarness
        editorState={{ ...cleanSettings, phase: "dirty" }}
        onSave={onSave}
      />,
    );

    await user.click(screen.getByRole("button", { name: "官方设置目录" }));
    await user.click(screen.getByRole("radio", { name: "Claude" }));
    expect(screen.getByRole("radio", { name: "Claude" })).toBeChecked();
    expect(
      screen.queryByRole("region", { name: "官方设置目录" }),
    ).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "保存客户端设置" }));
    expect(onSave).toHaveBeenCalledWith("claude");
  });

  it("keeps saving preferences separate from requesting the complete apply preview", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const onSaveAndPreview = vi.fn();
    const onPreview = vi.fn();
    render(
      <SettingsHarness
        editorState={{ ...cleanSettings, phase: "dirty" }}
        onSave={onSave}
        onSaveAndPreview={onSaveAndPreview}
        onPreview={onPreview}
      />,
    );

    expect(screen.getByText("有未保存修改")).toBeVisible();
    const save = screen.getByRole("button", { name: "保存客户端设置" });
    expect(save).toHaveClass("asb-btn-secondary");
    await user.click(save);
    expect(onSave).toHaveBeenCalledExactlyOnceWith("codex");
    expect(onSaveAndPreview).not.toHaveBeenCalled();
    expect(onPreview).not.toHaveBeenCalled();

    await user.click(screen.getByRole("button", { name: "保存并预览应用" }));
    expect(onSaveAndPreview).toHaveBeenCalledExactlyOnceWith("codex");
    expect(onSave).toHaveBeenCalledTimes(1);
    expect(onPreview).not.toHaveBeenCalled();
  });
});

describe("ClientSettingsPanel apply guards", () => {
  it("reports the observed applied state after saved preferences have been confirmed", () => {
    render(
      <SettingsHarness
        editorState={{ ...cleanSettings, phase: "savedPendingReapply" }}
        configStatus={appliedStatus}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent(
      "已应用：真实配置与「网关」一致",
    );
    expect(
      screen.queryByText("已保存，预览并确认应用后生效"),
    ).not.toBeInTheDocument();
  });

  it("disables apply preview without an active provider and links to the provider workspace", async () => {
    const user = userEvent.setup();
    const onOpenProviders = vi.fn();
    const onSaveAndPreview = vi.fn();
    render(
      <SettingsHarness
        editorState={{ ...cleanSettings, phase: "savedPendingReapply" }}
        hasActiveProvider={false}
        onOpenProviders={onOpenProviders}
        onSaveAndPreview={onSaveAndPreview}
      />,
    );

    expect(
      screen.getByText("已保存，请在供应商页选择并启用供应商后生效"),
    ).toBeVisible();
    const preview = screen.getByRole("button", { name: "保存并预览应用" });
    expect(preview).toBeDisabled();
    expect(preview).toHaveAccessibleDescription(
      "当前客户端没有已启用的供应商；请前往供应商页选择并启用。",
    );
    await user.click(preview);
    expect(onSaveAndPreview).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "前往供应商" }));
    expect(onOpenProviders).toHaveBeenCalledOnce();
  });

  it("allows saving but blocks apply preview until the supplier editor is resolved", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(
      <SettingsHarness
        editorState={{ ...cleanSettings, phase: "dirty" }}
        onSave={onSave}
        previewBlockedReason="请先保存或取消供应商编辑，再预览应用。"
      />,
    );

    expect(
      screen.getByRole("button", { name: "保存并预览应用" }),
    ).toBeDisabled();
    expect(
      screen.getByText("请先保存或取消供应商编辑，再预览应用。"),
    ).toBeVisible();
    await user.click(screen.getByRole("button", { name: "保存客户端设置" }));
    expect(onSave).toHaveBeenCalledExactlyOnceWith("codex");
  });

  it("keeps external drift visible and resets all groups through the existing action", async () => {
    const user = userEvent.setup();
    const onResetGroup = vi.fn();
    render(
      <SettingsHarness
        onResetGroup={onResetGroup}
        configStatus={{
          ...appliedStatus,
          matchStatus: {
            kind: "externallyModified",
            at: "2026-09-08T00:00:00Z",
          },
        }}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("真实配置已被外部修改");
    await user.click(screen.getByRole("button", { name: "全部恢复默认值" }));
    expect(onResetGroup).toHaveBeenCalledExactlyOnceWith("codex", null);
  });
});

describe("ClientSettingsPanel draft recovery", () => {
  it("exposes load retry and retains the editable draft after a save error", async () => {
    const user = userEvent.setup();
    const onRetryLoad = vi.fn();
    const onSave = vi.fn();
    const { rerender } = render(
      <SettingsHarness
        onRetryLoad={onRetryLoad}
        editorState={{
          phase: "loadError",
          error: { code: "store-unreadable", message: "应用数据不可读" },
        }}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent("应用数据不可读");
    expect(screen.getByRole("button", { name: "官方设置目录" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "重新读取" }));
    expect(onRetryLoad).toHaveBeenCalledExactlyOnceWith("codex");

    rerender(
      <SettingsHarness
        onSave={onSave}
        configStatus={appliedStatus}
        editorState={{
          ...cleanSettings,
          phase: "saveError",
          error: { code: "settings-stale", message: "本地偏好已变化" },
        }}
      />,
    );
    expect(screen.getByRole("alert")).toHaveTextContent(
      "保存失败，修改已保留：本地偏好已变化",
    );
    expect(screen.getByRole("status")).toHaveTextContent("有未保存修改");
    expect(screen.getByText("桌面通知")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "保存客户端设置" }));
    expect(onSave).toHaveBeenCalledWith("codex");
  });

  it("renders the client fragment on demand and keeps it while reading directory help", async () => {
    const user = userEvent.setup();
    const onPreview = vi.fn();
    function DemandHarness() {
      const [preview, setPreview] =
        useState<ClientSettingsEditorState["preview"]>();
      return (
        <SettingsHarness
          editorState={preview ? { ...cleanSettings, preview } : cleanSettings}
          onPreview={(target) => {
            onPreview(target);
            setPreview({
              app: "codex",
              target: "~/.codex/config.toml 客户端配置片段",
              content: "tui.notifications = true\n",
            });
          }}
        />
      );
    }
    render(<DemandHarness />);

    await user.click(
      screen.getByRole("button", { name: "查看客户端配置预览" }),
    );
    expect(onPreview).toHaveBeenCalledExactlyOnceWith("codex");
    expect(
      screen.getByLabelText("~/.codex/config.toml 客户端配置片段 配置预览"),
    ).toHaveTextContent("tui.notifications = true");
    await user.click(screen.getByRole("button", { name: "官方设置目录" }));
    await user.click(screen.getByRole("button", { name: "返回偏好设置" }));
    expect(
      screen.getByRole("button", { name: "收起客户端配置预览" }),
    ).toHaveAttribute("aria-expanded", "true");
    await user.click(
      screen.getByRole("button", { name: "收起客户端配置预览" }),
    );
    expect(
      screen.queryByLabelText("~/.codex/config.toml 客户端配置片段 配置预览"),
    ).toBeNull();
    await user.click(
      screen.getByRole("button", { name: "展开客户端配置预览" }),
    );
    expect(
      screen.getByLabelText("~/.codex/config.toml 客户端配置片段 配置预览"),
    ).toBeVisible();
    expect(onPreview).toHaveBeenCalledTimes(1);
  });
});
