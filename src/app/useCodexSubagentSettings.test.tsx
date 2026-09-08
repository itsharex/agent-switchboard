import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type {
  CodexSubagentSettings,
  CodexSubagentSettingsPreview,
  CodexSubagentSettingsSnapshot,
} from "../api/client";
import {
  applyCodexSubagentSettings,
  getCodexSubagentSettings,
  previewCodexSubagentSettings,
} from "../api/client";
import { useCodexSubagentSettings } from "./useCodexSubagentSettings";

vi.mock("../api/client", () => ({
  applyCodexSubagentSettings: vi.fn(),
  getCodexSubagentSettings: vi.fn(),
  previewCodexSubagentSettings: vi.fn(),
}));

const automaticSettings: CodexSubagentSettings = {
  enabled: { mode: "automatic" },
  maxConcurrentThreadsPerSession: { mode: "automatic" },
  interruptMessage: { mode: "automatic" },
};

const snapshot: CodexSubagentSettingsSnapshot = {
  app: "codex",
  settings: automaticSettings,
  configHash: "before-hash",
  fileExists: true,
  deprecatedKeys: [],
};

const preview: CodexSubagentSettingsPreview = {
  app: "codex",
  target: "Codex 用户级配置",
  content: "[agents]\nenabled = true\n",
  configHash: "before-hash",
  renderedHash: "candidate-hash",
};

function dependencies() {
  return {
    active: true,
    busy: false,
    onError: vi.fn(),
    clearError: vi.fn(),
    setBusy: vi.fn(),
    invalidateSwitchCandidates: vi.fn(),
    refresh: vi.fn().mockResolvedValue(undefined),
  };
}

describe("useCodexSubagentSettings", () => {
  beforeEach(() => {
    vi.mocked(getCodexSubagentSettings).mockReset();
    vi.mocked(previewCodexSubagentSettings).mockReset();
    vi.mocked(applyCodexSubagentSettings).mockReset();
    vi.mocked(getCodexSubagentSettings).mockResolvedValue(snapshot);
  });

  it("loads the real configuration snapshot without creating a second stored copy", async () => {
    const deps = dependencies();
    const { result } = renderHook(() => useCodexSubagentSettings(deps));

    await waitFor(() => expect(result.current.editorState.phase).toBe("clean"));

    expect(getCodexSubagentSettings).toHaveBeenCalledOnce();
    expect(result.current.editorState.snapshot).toEqual(snapshot);
    expect(result.current.editorState.draft).toEqual(automaticSettings);
  });

  it("binds apply to the exact preview and refreshes configuration observation after success", async () => {
    const deps = dependencies();
    vi.mocked(previewCodexSubagentSettings).mockResolvedValue(preview);
    const changed: CodexSubagentSettings = {
      ...automaticSettings,
      enabled: { mode: "explicit", value: true },
    };
    const saved: CodexSubagentSettingsSnapshot = {
      ...snapshot,
      settings: changed,
      configHash: "after-hash",
    };
    vi.mocked(applyCodexSubagentSettings).mockResolvedValue(saved);
    const { result } = renderHook(() => useCodexSubagentSettings(deps));
    await waitFor(() => expect(result.current.editorState.phase).toBe("clean"));

    act(() => result.current.changeValue("enabled", changed.enabled));
    expect(result.current.editorState.phase).toBe("dirty");
    act(() => result.current.preview());
    await waitFor(() => expect(result.current.editorState.preview).toEqual(preview));

    await act(async () => {
      await result.current.applyPreview();
    });

    expect(previewCodexSubagentSettings).toHaveBeenCalledWith(changed, "before-hash");
    expect(applyCodexSubagentSettings).toHaveBeenCalledWith({
      settings: changed,
      expectedHash: "before-hash",
      expectedTargetExisted: true,
      renderedHash: "candidate-hash",
    }, true);
    expect(deps.invalidateSwitchCandidates).toHaveBeenCalledOnce();
    expect(deps.refresh).toHaveBeenCalledOnce();
    expect(result.current.editorState).toMatchObject({
      phase: "clean",
      snapshot: saved,
      draft: changed,
    });
  });

  it("keeps a changed draft when rereading after an external-change failure", async () => {
    const deps = dependencies();
    vi.mocked(getCodexSubagentSettings)
      .mockResolvedValueOnce(snapshot)
      .mockResolvedValueOnce({ ...snapshot, configHash: "new-hash" });
    const { result } = renderHook(() => useCodexSubagentSettings(deps));
    await waitFor(() => expect(result.current.editorState.phase).toBe("clean"));

    act(() => result.current.changeValue("enabled", { mode: "explicit", value: false }));
    act(() => result.current.retryLoad());
    await waitFor(() => expect(result.current.editorState.snapshot?.configHash).toBe("new-hash"));

    expect(result.current.editorState.phase).toBe("dirty");
    expect(result.current.editorState.draft?.enabled).toEqual({ mode: "explicit", value: false });
    expect(result.current.editorState.preview).toBeUndefined();
  });
});
