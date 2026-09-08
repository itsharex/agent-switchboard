import {
  providerParameters,
  providerParametersCatalog,
} from "../test/provider-parameters";
import { useState } from "react";
import type { ProviderView } from "../app/navigation";
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { FilePreview, ProviderProfile } from "../api/client";
import * as client from "../api/client";
import { ProvidersPage } from "./ProvidersPage";
import { statuses } from "../test/app-fixtures";

const profile: ProviderProfile = {
  id: "relay-a",
  app: "codex",
  routeMode: "custom",
  name: "中继 A",
  model: null,
  baseUrl: "https://relay.example/v1",
  apiKey: "test-key",
  upstreamProtocol: "responses",
  responsesOptions: { requestMode: "standard" as const },
  maxOutputTokens: null,
  parameters: providerParameters("codex"),
  modelOptions: null,
  websiteUrl: null,
  usageQuery: null,
};

const previewFile: FilePreview = {
  contentHash: "hash-1",
  renderedHash: "rendered-1",
  content: `model = "gpt-5.4"\n`,
  preview: {
    app: "codex",
    target: "C:/Users/test/.codex/config.toml",
    changes: [{ key: "model", kind: "set", before: null, after: "gpt-5.4" }],
    warnings: [],
    backupDir: "C:/backups",
  },
};

type PageProps = Omit<
  Parameters<typeof ProvidersPage>[0],
  "view" | "onViewChange"
>;

function TestPage(props: PageProps) {
  const [view, onViewChange] = useState<ProviderView>({ kind: "list" });
  return <ProvidersPage {...props} view={view} onViewChange={onViewChange} />;
}

function renderPage(overrides: Partial<PageProps> = {}) {
  const props: PageProps = {
    active: true,
    profiles: [profile],
    appFilter: "codex",
    activeProfileId: null,
    statuses: null,
    locks: {},
    userConfigModel: null,
    userConfigWarnings: [],
    selectedId: null,
    editorSession: null,
    preview: null,
    busy: false,
    collapsedUsageIds: [],
    onSelectApp() {},
    onNew() {},
    onImport() {},
    onOpenClientSettings() {},
    onOpenHistory() {},
    onOpenDiagnostics() {},
    onOpenQuota() {},
    onCloseEditor() {},
    onSelect() {},
    onReorder() {},
    onToggleUsage() {},
    onActivate() {},
    onTogglePreview() {},
    onEdit() {},
    onDelete() {},
    onRequestSwitch() {},
    onCancelPreview() {},
    onSave: async () => {},
    onSaveUsageQuery: async () => true,
    onSaveQuotaInterval: async () => true,
    ...overrides,
  };
  const view = render(<TestPage {...props} />);
  return {
    ...view,
    rerenderPage: (next: Partial<PageProps>) =>
      view.rerender(<TestPage {...props} {...next} />),
  };
}

afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("ProvidersPage", () => {
  it("opens and saves the independent usage workspace from a provider card", async () => {
    const user = userEvent.setup();
    const onSaveUsageQuery = vi.fn(async () => true);
    renderPage({ onSaveUsageQuery });

    await user.click(screen.getByRole("button", { name: "配置 中继 A 用量" }));
    expect(
      screen.getByRole("region", { name: "用量查询" }),
    ).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("用量查询地址"), {
      target: { value: "{{baseUrl}}/balance" },
    });
    fireEvent.change(screen.getByLabelText("余额提取路径"), {
      target: { value: "data/balance" },
    });
    await user.click(screen.getByRole("button", { name: "保存查询" }));

    expect(onSaveUsageQuery).toHaveBeenCalledWith(profile, {
      kind: "declarative",
      url: "{{baseUrl}}/balance",
      remainingPath: "data/balance",
      usedPath: null,
      totalPath: null,
      unit: null,
      refreshIntervalMinutes: 0,
    });
    expect(
      await screen.findByRole("region", { name: "供应商工作区" }),
    ).toBeInTheDocument();
  });

  it("cancels or confirms the pending switch from the preview header", async () => {
    const user = userEvent.setup();
    const onCancelPreview = vi.fn();
    const onRequestSwitch = vi.fn();
    renderPage({
      preview: { profileId: profile.id, file: previewFile },
      userConfigModel: "gpt-5.3-codex",
      userConfigWarnings: ["使用 --profile 启动时会覆盖这里的用户级设置"],
      onCancelPreview,
      onRequestSwitch,
    });

    const previewPanel = screen.getByRole("region", { name: "变更预览" });
    expect(
      within(previewPanel).getByText("当前用户级配置模型"),
    ).toBeInTheDocument();
    expect(within(previewPanel).getByText("gpt-5.3-codex")).toBeInTheDocument();
    expect(
      within(previewPanel).getByText(
        "使用 --profile 启动时会覆盖这里的用户级设置",
      ),
    ).toBeInTheDocument();
    await user.click(
      within(previewPanel).getByRole("button", { name: "取消" }),
    );
    expect(onCancelPreview).toHaveBeenCalledTimes(1);

    await user.click(
      within(previewPanel).getByRole("button", { name: "确认切换" }),
    );
    expect(onRequestSwitch).toHaveBeenCalledTimes(1);
  });
});

describe("ProvidersPage editing", () => {
  it("opens an existing official profile when that mode is chosen in a new form", async () => {
    const user = userEvent.setup();
    const official: ProviderProfile = {
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
      usageQuery: null,
    };
    const onSelectApp = vi.fn();
    const onEdit = vi.fn();

    renderPage({
      profiles: [profile, official],
      editorSession: { app: "codex", record: null },
      onSelectApp,
      onEdit,
    });

    await user.click(screen.getByRole("radio", { name: "官方登录" }));

    expect(onSelectApp).toHaveBeenCalledWith("codex");
    expect(onEdit).toHaveBeenCalledWith(official);
  });
});

describe("ProvidersPage navigation", () => {
  it("keeps both observed connections above the picker when list selection changes", () => {
    const { rerenderPage, container } = renderPage({ statuses });
    const cards = screen.getByRole("group", { name: "当前启用配置" });
    const picker = screen.getByRole("radiogroup", { name: "供应商客户端" });
    expect(
      cards.compareDocumentPosition(picker) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(within(cards).getByText("gpt-5.3-codex")).toBeInTheDocument();
    expect(within(cards).getByText("claude-sonnet-4")).toBeInTheDocument();
    rerenderPage({
      statuses,
      appFilter: "claude",
      selectedId: "some-other-profile",
    });
    expect(within(cards).getByText("gpt-5.3-codex")).toBeInTheDocument();
    expect(container.querySelectorAll("canvas")).toHaveLength(2);
    rerenderPage({ active: false });
    expect(container.querySelector("canvas")).toBeNull();
  });

  it("exposes import, client preferences and switch history next to the client picker", async () => {
    const onImport = vi.fn();
    const onOpenClientSettings = vi.fn();
    const onOpenHistory = vi.fn();
    const onSelectApp = vi.fn();
    const { rerenderPage } = renderPage({
      onImport,
      onOpenClientSettings,
      onOpenHistory,
      onSelectApp,
    });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "导入" }));
    await user.click(screen.getByRole("button", { name: "偏好设置" }));
    await user.click(screen.getByRole("button", { name: "切换历史" }));
    await user.click(
      within(
        screen.getByRole("radiogroup", { name: "供应商客户端" }),
      ).getByRole("radio", { name: "Claude" }),
    );
    expect(onImport).toHaveBeenCalledOnce();
    expect(onOpenClientSettings).toHaveBeenCalledOnce();
    expect(onOpenHistory).toHaveBeenCalledOnce();
    expect(onSelectApp).toHaveBeenCalledWith("claude");
    rerenderPage({ appFilter: "claude" });
    expect(
      screen.getByRole("region", { name: "Claude 当前连接" }),
    ).toBeInTheDocument();
  });

  it("unmounts provider cards while inactive and omits the connection cards from the editor", () => {
    const { rerenderPage } = renderPage({ active: false });
    expect(
      screen.queryByRole("listbox", { name: "供应商列表" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("region", { name: "Codex 当前连接" }),
    ).not.toBeInTheDocument();
    rerenderPage({
      active: true,
      editorSession: { app: "codex", record: null },
    });
    expect(
      screen.getByRole("form", { name: "新建供应商" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("region", { name: "Codex 当前连接" }),
    ).not.toBeInTheDocument();
  });
});

describe("ProvidersPage draft lifetime", () => {
  it("retains the editor and runtime-parameter draft when another workspace changes the selected client", async () => {
    vi.spyOn(client, "getProviderParametersCatalog").mockResolvedValue(
      providerParametersCatalog("codex"),
    );
    const editorSession = {
      app: "codex" as const,
      record: { profile, fileHash: "editing-hash" },
    };
    const { rerenderPage } = renderPage({ editorSession });
    const user = userEvent.setup();
    fireEvent.change(screen.getByLabelText("名称"), {
      target: { value: "保留的供应商草稿" },
    });
    await user.click(screen.getByRole("button", { name: /配置运行参数/ }));
    expect(
      screen.getByRole("heading", { name: "运行参数" }),
    ).toBeInTheDocument();
    rerenderPage({ active: false, appFilter: "claude" });
    expect(
      screen.queryByRole("heading", { name: "运行参数" }),
    ).not.toBeInTheDocument();
    rerenderPage({ active: true, appFilter: "claude" });
    expect(
      screen.getByRole("heading", { name: "运行参数" }),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "返回供应商编辑" }));
    expect(screen.getByLabelText("名称")).toHaveValue("保留的供应商草稿");
    expect(screen.getByRole("combobox", { name: "客户端" })).toHaveTextContent(
      "Codex",
    );
  });

  it("retains the usage-query draft while inactive without rendering a connection summary", async () => {
    const { rerenderPage } = renderPage();
    await userEvent.click(
      screen.getByRole("button", { name: "配置 中继 A 用量" }),
    );
    fireEvent.change(screen.getByLabelText("用量查询地址"), {
      target: { value: "{{baseUrl}}/draft" },
    });
    rerenderPage({ active: false, appFilter: "claude" });
    expect(
      screen.queryByRole("region", { name: "用量查询" }),
    ).not.toBeInTheDocument();
    rerenderPage({ active: true, appFilter: "claude" });
    expect(screen.getByLabelText("用量查询地址")).toHaveValue(
      "{{baseUrl}}/draft",
    );
    expect(
      screen.queryByRole("region", { name: /当前连接/ }),
    ).not.toBeInTheDocument();
  });
});

describe("ProvidersPage polling lifetime", () => {
  it("stops official quota polling when the supplier list becomes inactive", async () => {
    vi.useFakeTimers();
    const query = vi
      .spyOn(client, "queryCodexOfficialQuota")
      .mockResolvedValue({
        status: "signInRequired",
        windows: [],
        at: null,
        stale: false,
        lastReset: null,
      });
    vi.spyOn(client, "getUsageHistory").mockResolvedValue([]);
    const official: ProviderProfile = {
      ...profile,
      id: "codex-official",
      name: "Codex 官方",
      routeMode: "official",
      baseUrl: null,
      apiKey: "",
      upstreamProtocol: null,
      responsesOptions: null,
      officialQuotaRefreshIntervalMinutes: 1,
    };
    const { rerenderPage } = renderPage({ profiles: [official] });
    await act(async () => {});
    expect(query).toHaveBeenCalledTimes(1);
    rerenderPage({ active: false });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(180_000);
    });
    expect(query).toHaveBeenCalledTimes(1);
    expect(
      screen.queryByRole("listbox", { name: "供应商列表" }),
    ).not.toBeInTheDocument();
  });
});
