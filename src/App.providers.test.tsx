import { describe, expect, it, vi } from "vitest";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import App from "./App";
import { CodexProvidersPage } from "./pages/CodexProvidersPage";
import type { CodexProviderRecord, FilePreview } from "./api/client";
import {
  codexFilePreview,
  codexOfficialFilePreview,
  codexProfiles,
  deferred,
  invokeMock,
  primeBackend,
  statuses,
} from "./test/app-fixtures";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ onResized: () => Promise.resolve(() => {}) }),
}));
vi.mock("@tauri-apps/plugin-updater", () => ({
  check: vi.fn(() => Promise.resolve(null)),
}));

function card(name: string) {
  const entry = screen.getByText(name).closest("li");
  expect(entry).not.toBeNull();
  return entry!;
}

function primeActiveCodexBackend() {
  primeBackend();
  const backend = invokeMock.getMockImplementation();
  expect(backend).toBeDefined();
  invokeMock.mockImplementation((command: string, args?: unknown) => {
    if (command === "config_status") {
      return Promise.resolve([
        { ...statuses[0], activeProfileId: codexProfiles[0].profile.id },
        statuses[1],
      ]);
    }
    return backend!(command, args as never);
  });
}

function codexRecord(
  id: string,
  name: string,
  model: string,
): CodexProviderRecord {
  const source = codexProfiles[0];
  return {
    ...source,
    profile: {
      ...source.profile,
      id,
      name,
      defaultModel: model,
      catalog: source.profile.catalog.map((entry) => ({ ...entry, id: model })),
      modelRoutes: [{ clientModel: model, upstreamModel: model }],
    },
    fileHash: `${id}-file-hash`,
  };
}

function previewFor(id: string, model: string): FilePreview {
  return {
    ...codexFilePreview,
    contentHash: `hash-${id}`,
    renderedHash: `rendered-${id}`,
    content: `model = "${model}"\nmodel_provider = "openai"\nopenai_base_url = "<redacted>"\n`,
    preview: {
      ...codexFilePreview.preview,
      changes: [
        { key: "model", kind: "set", before: "gpt-5.3-codex", after: model },
      ],
    },
  };
}

describe("App.providers", () => {
  it("renders a dedicated Codex profile and switches only from its exact preview", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    const profileCard = await screen
      .findByText("备用网关")
      .then(() => card("备用网关"));
    expect(profileCard).toHaveTextContent("gpt-5.4");
    await user.click(within(profileCard).getByRole("button", { name: "启用 备用网关" }));

    const preview = await screen.findByLabelText(
      "C:/Users/test/.codex/config.toml 配置预览",
    );
    expect(preview).toHaveTextContent("openai_base_url");
    expect(screen.getByRole("button", { name: "确认切换" })).toBeEnabled();
    expect(invokeMock).toHaveBeenCalledWith("preview_switch", {
      profileId: "codex-gateway",
    });

    await user.click(screen.getByRole("button", { name: "取消" }));
    expect(
      screen.queryByLabelText("C:/Users/test/.codex/config.toml 配置预览"),
    ).not.toBeInTheDocument();
    expect(invokeMock.mock.calls.map(([command]) => command)).not.toContain(
      "execute_switch",
    );

    await user.click(
      within(card("备用网关")).getByRole("button", { name: "启用 备用网关" }),
    );
    await screen.findByLabelText("C:/Users/test/.codex/config.toml 配置预览");
    await user.click(screen.getByRole("button", { name: "确认切换" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("execute_switch", {
        profileId: "codex-gateway",
        expectedHash: "codex-hash1",
        expectedRenderedHash: "codex-rendered-hash1",
        confirmWrite: true,
      }),
    );
  });

  it("creates a complete specialized Codex profile from the structured form", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "新建供应商" }));
    fireEvent.change(screen.getByLabelText("名称"), {
      target: { value: "新网关" },
    });
    fireEvent.change(screen.getByLabelText("服务地址"), {
      target: { value: "https://new.internal/v1" },
    });
    fireEvent.change(screen.getByLabelText("API 密钥"), {
      target: { value: "NEW_KEY" },
    });
    await user.click(screen.getByRole("button", { name: "获取模型" }));
    expect(screen.getByLabelText("模型标识 1")).toHaveValue("gpt-5.4");
    expect(screen.getByLabelText("模型标识 2")).toHaveValue("gpt-5.4-mini");
    await user.click(screen.getByRole("combobox", { name: "默认模型" }));
    await user.click(await screen.findByRole("option", { name: "gpt-5.4" }));
    await user.click(screen.getByText("模型映射"));
    await user.click(screen.getByRole("button", { name: "添加映射" }));
    fireEvent.change(screen.getByLabelText("映射上游模型 1"), {
      target: { value: "vendor-gpt-5.4" },
    });
    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("create_codex_profile", {
        draft: expect.objectContaining({
          name: "新网关",
          endpoint: "https://new.internal/v1",
          apiKey: "NEW_KEY",
          upstream: "responses",
          requestMode: "standard",
          defaultModel: "gpt-5.4",
          catalog: [
            expect.objectContaining({ id: "gpt-5.4", contextWindow: 128_000 }),
            expect.objectContaining({ id: "gpt-5.4-mini" }),
          ],
          modelRoutes: [{ clientModel: "gpt-5.4", upstreamModel: "vendor-gpt-5.4" }],
        }),
      }),
    );
  });

  it("blocks creating a Codex profile whose catalog is still empty", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "新建供应商" }));
    fireEvent.change(screen.getByLabelText("名称"), {
      target: { value: "新网关" },
    });
    fireEvent.change(screen.getByLabelText("服务地址"), {
      target: { value: "https://new.internal/v1" },
    });
    fireEvent.change(screen.getByLabelText("API 密钥"), {
      target: { value: "NEW_KEY" },
    });
    invokeMock.mockClear();
    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    expect(
      await screen.findByText("模型目录不能为空；请获取模型或手动添加"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "保存供应商" })).toBeDisabled();
    expect(invokeMock.mock.calls.map(([command]) => command)).not.toContain(
      "create_codex_profile",
    );
  });

  it("reorders specialized Codex profiles with every current file revision", async () => {
    primeBackend();
    const records = [
      codexRecord("codex-a", "网关甲", "model-a"),
      codexRecord("codex-b", "网关乙", "model-b"),
    ];
    render(
      <CodexProvidersPage
        active
        records={records}
        officialRecord={null}
        activeProfileId={null}
        busy={false}
        onBusy={() => {}}
        onError={() => {}}
        onRefresh={async () => {}}
        onSelectApp={() => {}}
        requestedPreviewId={null}
        onPreviewRequestHandled={() => {}}
        onImport={() => {}}
        onOpenClientSettings={() => {}}
        onOpenHistory={() => {}}
        onDelete={() => {}}
        onDeleteOfficial={() => {}}
        editorSession={null}
        onNew={() => {}}
        onEdit={() => {}}
        onEditOfficial={() => {}}
        onCloseEditor={() => {}}
        onSave={() => {}}
        onSaveOfficial={() => {}}
        onSwitchAccessMode={() => {}}
        onSwitchClient={() => {}}
        onSaveOfficialQuotaInterval={async () => true}
        statuses={[]}
        profiles={[]}
        locks={{}}
        userConfigModel={null}
        userConfigWarnings={[]}
      />,
    );

    // jsdom has no layout; give the rows real vertical geometry so the
    // sortable keyboard coordinate getter can resolve the next row.
    document.querySelectorAll("li.asb-row-item").forEach((row, index) => {
      const top = index * 76;
      row.getBoundingClientRect = () =>
        ({ top, bottom: top + 68, left: 0, right: 320, width: 320, height: 68, x: 0, y: top }) as DOMRect;
    });
    const grip = screen.getByRole("button", { name: "拖动调整 网关甲 的顺序" });
    grip.focus();
    const press = async (code: string, key: string) => {
      await act(async () => {
        fireEvent.keyDown(grip, { key, code, bubbles: true, cancelable: true });
        await new Promise((resolve) => setTimeout(resolve, 0));
      });
    };
    await press("Space", " ");
    await press("ArrowDown", "ArrowDown");
    await press("Space", " ");

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("reorder_codex_profiles", {
        orderedIds: ["codex-b", "codex-a"],
        expectedFileHashes: {
          "codex-a": "codex-a-file-hash",
          "codex-b": "codex-b-file-hash",
        },
      }),
    );
  });

  it("requires the active Codex profile edit to be confirmed before it is applied", async () => {
    primeActiveCodexBackend();
    const user = userEvent.setup();
    render(<App />);

    await user.click(
      within(
        await screen.findByText("备用网关").then(() => card("备用网关")),
      ).getByRole("button", { name: "编辑 备用网关" }),
    );
    fireEvent.change(screen.getByLabelText("服务地址"), {
      target: { value: "https://updated.internal/v1" },
    });
    await user.click(screen.getByRole("button", { name: "保存供应商" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "prepare_codex_profile_save",
        expect.objectContaining({
          profileId: "codex-gateway",
          expectedFileHash: "codex-provider-file-hash",
          draft: expect.objectContaining({
            endpoint: "https://updated.internal/v1",
          }),
        }),
      ),
    );
    expect(invokeMock).not.toHaveBeenCalledWith(
      "commit_codex_profile_save",
      expect.anything(),
    );

    await user.click(
      await screen.findByRole("button", { name: "确认保存并应用" }),
    );
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("commit_codex_profile_save", {
        preparationId: "prepared-codex-save",
        confirmWrite: true,
      }),
    );
  });

  it("deletes a Codex profile through the shared confirmation sheet", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    const row = await screen.findByText("备用网关").then(() => card("备用网关"));
    await user.click(within(row).getByRole("button", { name: "更多 备用网关 操作" }));
    await user.click(screen.getByRole("menuitem", { name: "删除 备用网关" }));

    const dialog = screen.getByRole("dialog", { name: "删除供应商" });
    expect(dialog).toHaveTextContent("删除本地记录 备用网关");
    await user.click(within(dialog).getByRole("button", { name: "确认删除" }));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_codex_profile", {
        profileId: "codex-gateway",
        expectedFileHash: "codex-provider-file-hash",
      }),
    );
  });

  it("switches back to Codex official routing from the official row's own preview", async () => {
    primeActiveCodexBackend();
    const user = userEvent.setup();
    render(<App />);

    const row = await screen.findByText("Codex 官方登录").then(() => card("Codex 官方登录"));
    expect(row).toHaveTextContent("官方登录");
    await user.click(within(row).getByRole("button", { name: "启用 Codex 官方登录" }));

    const preview = await screen.findByLabelText(
      "C:/Users/test/.codex/config.toml 配置预览",
    );
    expect(preview).toHaveTextContent("model_provider");
    expect(invokeMock).toHaveBeenCalledWith("preview_switch", {
      profileId: "codex-official",
    });

    await user.click(screen.getByRole("button", { name: "确认切换" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("execute_switch", {
        profileId: "codex-official",
        expectedHash: codexOfficialFilePreview.contentHash,
        expectedRenderedHash: codexOfficialFilePreview.renderedHash,
        confirmWrite: true,
      }),
    );
  });

  it("creates the Codex official-login record from the editor's access-mode choice", async () => {
    primeBackend();
    const user = userEvent.setup();
    render(<App />);

    await user.click(await screen.findByRole("button", { name: "新建供应商" }));
    expect(screen.getByRole("heading", { name: "新建 Codex 供应商" })).toBeInTheDocument();

    await user.click(screen.getByRole("radio", { name: "官方登录" }));
    expect(screen.getByRole("heading", { name: "新建 Codex 官方登录" })).toBeInTheDocument();
    expect(screen.getByLabelText("名称")).toHaveValue("Codex 官方登录");
    expect(screen.getByRole("button", { name: "开始官方登录" })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("prepare_profile_save", {
        profileId: null,
        expectedFileHash: null,
        draft: expect.objectContaining({
          app: "codex",
          routeMode: "official",
          name: "Codex 官方登录",
          baseUrl: null,
          apiKey: "",
        }),
      }),
    );
  });

  it("keeps the latest Codex preview when an older request resolves late", async () => {
    primeBackend();
    const records = [
      codexRecord("codex-a", "网关甲", "model-a"),
      codexRecord("codex-b", "网关乙", "model-b"),
    ];
    const pending = new Map<string, ReturnType<typeof deferred<FilePreview>>>();
    const backend = invokeMock.getMockImplementation();
    expect(backend).toBeDefined();
    invokeMock.mockImplementation((command: string, args?: unknown) => {
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

    const user = userEvent.setup();
    render(
      <CodexProvidersPage
        active
        records={records}
        officialRecord={null}
        activeProfileId={null}
        busy={false}
        onBusy={() => {}}
        onError={() => {}}
        onRefresh={async () => {}}
        onSelectApp={() => {}}
        requestedPreviewId={null}
        onPreviewRequestHandled={() => {}}
        onImport={() => {}}
        onOpenClientSettings={() => {}}
        onOpenHistory={() => {}}
        onDelete={() => {}}
        onDeleteOfficial={() => {}}
        editorSession={null}
        onNew={() => {}}
        onEdit={() => {}}
        onEditOfficial={() => {}}
        onCloseEditor={() => {}}
        onSave={() => {}}
        onSaveOfficial={() => {}}
        onSwitchAccessMode={() => {}}
        onSwitchClient={() => {}}
        onSaveOfficialQuotaInterval={async () => true}
        statuses={[]}
        profiles={[]}
        locks={{}}
        userConfigModel={null}
        userConfigWarnings={[]}
      />,
    );

    await user.click(
      within(card("网关甲")).getByRole("button", { name: "启用 网关甲" }),
    );
    await user.click(
      within(card("网关乙")).getByRole("button", { name: "启用 网关乙" }),
    );
    expect(pending.get("codex-a")).toBeDefined();
    expect(pending.get("codex-b")).toBeDefined();

    await act(async () => {
      pending.get("codex-b")!.resolve(previewFor("codex-b", "model-b"));
    });
    const preview = await screen.findByLabelText(
      "C:/Users/test/.codex/config.toml 配置预览",
    );
    expect(preview).toHaveTextContent("model-b");

    await act(async () => {
      pending.get("codex-a")!.resolve(previewFor("codex-a", "model-a"));
    });
    expect(preview).toHaveTextContent("model-b");
    expect(preview).not.toHaveTextContent("model-a");
  });
});
