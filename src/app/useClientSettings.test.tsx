import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ClientSettingsEditor, ClientSettingsPreview, SettingsValues } from "../api/client";
import {
  getClientSettingsEditor,
  parseClientSettings,
  previewClientSettings,
  saveClientSettings,
} from "../api/client";
import { useClientSettings } from "./useClientSettings";

vi.mock("../api/client", () => ({
  getClientSettingsEditor: vi.fn(),
  parseClientSettings: vi.fn(),
  previewClientSettings: vi.fn(),
  saveClientSettings: vi.fn(),
}));

const automatic: SettingsValues = {
  settings: { "tui.notifications": { mode: "automatic" } },
};

const editor: ClientSettingsEditor = {
  app: "codex",
  settings: automatic,
  settingsHash: "settings-hash",
  groups: ["终端界面"],
  specs: [{
    key: "tui.notifications",
    label: "终端通知",
    group: "终端界面",
    control: "toggle",
    options: [],
  }],
  directory: [],
};

function preview(content: string): ClientSettingsPreview {
  return { app: "codex", target: "Codex 客户端配置片段", content };
}

function dependencies() {
  return {
    app: "codex" as const,
    active: true,
    busy: false,
    onError: vi.fn(),
    clearError: vi.fn(),
    invalidateSwitchCandidates: vi.fn(),
    refresh: vi.fn().mockResolvedValue(undefined),
    setBusy: vi.fn(),
  };
}

describe("useClientSettings", () => {
  beforeEach(() => {
    vi.mocked(getClientSettingsEditor).mockReset();
    vi.mocked(parseClientSettings).mockReset();
    vi.mocked(previewClientSettings).mockReset();
    vi.mocked(saveClientSettings).mockReset();
    vi.mocked(getClientSettingsEditor).mockResolvedValue(editor);
  });

  it("keeps manual fragment edits and visual controls in sync both ways", async () => {
    const text = "[tui]\nnotifications = true\n";
    const parsed: SettingsValues = {
      settings: { "tui.notifications": { mode: "explicit", value: true } },
    };
    vi.mocked(previewClientSettings)
      .mockResolvedValueOnce(preview("# 所有客户端设置均为自动\n"))
      .mockResolvedValueOnce(preview("# 所有客户端设置均为自动\n"));
    vi.mocked(parseClientSettings).mockResolvedValue(parsed);
    const { result } = renderHook(() => useClientSettings(dependencies()));

    await waitFor(() => expect(result.current.editorState.phase).toBe("clean"));
    act(() => result.current.previewSettings("codex"));
    await waitFor(() => expect(result.current.editorState.preview).toBeDefined());

    act(() => result.current.changePreviewContent("codex", text));
    await waitFor(() => expect(result.current.editorState.draft).toEqual(parsed.settings));
    expect(result.current.editorState.preview?.content).toBe(text);

    act(() => result.current.changeValue("codex", "tui.notifications", { mode: "automatic" }));
    await waitFor(() => expect(result.current.editorState.preview?.content).toContain("自动"));
    expect(previewClientSettings).toHaveBeenLastCalledWith("codex", automatic);
  });

  it("retains invalid text, leaves the last valid controls unchanged, and blocks saving", async () => {
    vi.mocked(previewClientSettings).mockResolvedValue(preview("# 所有客户端设置均为自动\n"));
    vi.mocked(parseClientSettings).mockRejectedValue({
      code: "client-settings-parse-failed",
      message: "TOML 格式无效（第 1 行）",
    });
    const { result } = renderHook(() => useClientSettings(dependencies()));

    await waitFor(() => expect(result.current.editorState.phase).toBe("clean"));
    act(() => result.current.previewSettings("codex"));
    await waitFor(() => expect(result.current.editorState.preview).toBeDefined());
    act(() => result.current.changePreviewContent("codex", "tui.notifications ="));

    await waitFor(() => expect(result.current.editorState.parseError?.message).toContain("TOML 格式无效"));
    expect(result.current.editorState.preview?.content).toBe("tui.notifications =");
    expect(result.current.editorState.draft).toEqual(automatic.settings);
    await expect(result.current.saveSettings("codex")).resolves.toBe(false);
    expect(saveClientSettings).not.toHaveBeenCalled();
  });
});
