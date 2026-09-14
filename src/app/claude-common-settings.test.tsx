import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { getClientSettingsEditor, parseClientSettings, previewClientSettings, saveClientSettings, type ClientSettingsEditor, type SettingsValues } from "../api/client";
import { useClientSettings } from "./useClientSettings";
import { clientSettingsPayload } from "./claude-common-settings";
vi.mock("../api/client", () => ({ getClientSettingsEditor: vi.fn(), parseClientSettings: vi.fn(), previewClientSettings: vi.fn(), saveClientSettings: vi.fn() }));
const extra = { env: { DISABLE_TELEMETRY: "1" }, hooks: { SessionStart: [{ hooks: [{ type: "command", command: "echo fixture" }] }] } };
const initial: SettingsValues = { settings: { spinnerTipsEnabled: { mode: "automatic" } }, claudeExtra: extra };
const editor: ClientSettingsEditor = { app: "claude", settings: initial, settingsHash: "common-r1", groups: ["界面与交互"], specs: [{ key: "spinnerTipsEnabled", label: "提示", group: "界面与交互", control: "toggle", options: [] }], directory: [] };
const deps = () => ({ app: "claude" as const, active: true, busy: false, onError: vi.fn(), clearError: vi.fn(), invalidateSwitchCandidates: vi.fn(), refresh: vi.fn(async () => {}), setBusy: vi.fn() });
beforeEach(() => {
  vi.mocked(getClientSettingsEditor).mockReset().mockResolvedValue(editor);
  vi.mocked(parseClientSettings).mockReset();
  vi.mocked(previewClientSettings).mockReset().mockImplementation(async (_app, settings) => ({ app: "claude", target: "Claude 通用配置片段", content: JSON.stringify({ ...settings.claudeExtra, spinnerTipsEnabled: false }) }));
  vi.mocked(saveClientSettings).mockReset().mockImplementation(async (_app, settings) => ({ settings, settingsHash: "common-r2" }));
});
it("visual changes and previews preserve Claude extra configuration in the same store write", async () => {
  const { result } = renderHook(() => useClientSettings(deps())); await waitFor(() => expect(result.current.editorState.phase).toBe("clean"));
  act(() => result.current.changeValue("claude", "spinnerTipsEnabled", { mode: "explicit", value: false }));
  act(() => result.current.previewSettings("claude"));
  await waitFor(() => expect(previewClientSettings).toHaveBeenCalledWith("claude", { settings: { spinnerTipsEnabled: { mode: "explicit", value: false } }, claudeExtra: extra }));
  await act(async () => { expect(await result.current.saveSettings("claude")).toBe(true); });
  expect(saveClientSettings).toHaveBeenCalledWith("claude", expect.objectContaining({ claudeExtra: extra }), "common-r1");
  expect(result.current.editorState.claudeExtra).toEqual(extra);
});
it("extra-only edits become dirty and persist without losing the visual preference state", async () => {
  const next = { ...initial, claudeExtra: { env: { DISABLE_TELEMETRY: "0" } } };
  vi.mocked(parseClientSettings).mockResolvedValue(next);
  const { result } = renderHook(() => useClientSettings(deps())); await waitFor(() => expect(result.current.editorState.phase).toBe("clean"));
  act(() => result.current.previewSettings("claude")); await waitFor(() => expect(result.current.editorState.preview).toBeDefined());
  act(() => result.current.changePreviewContent("claude", '{"env":{"DISABLE_TELEMETRY":"0"}}'));
  await waitFor(() => expect(result.current.editorState.phase).toBe("dirty"));
  expect(result.current.editorState.draft).toEqual(initial.settings);
  await act(async () => { await result.current.saveSettings("claude"); });
  expect(saveClientSettings).toHaveBeenCalledWith("claude", next, "common-r1");
});
it("removing extra fields or resetting all is an explicit dirty intent, not a fallback to old extras", async () => {
  const { result } = renderHook(() => useClientSettings(deps())); await waitFor(() => expect(result.current.editorState.phase).toBe("clean"));
  act(() => result.current.resetGroupToDefaults("claude", "界面与交互")); expect(result.current.editorState.claudeExtra).toEqual(extra);
  act(() => result.current.resetGroupToDefaults("claude", null)); expect(result.current.editorState.phase).toBe("dirty");
  await act(async () => { await result.current.saveSettings("claude"); });
  expect(saveClientSettings).toHaveBeenCalledWith("claude", { settings: initial.settings }, "common-r1");
  expect(() => clientSettingsPayload("codex", initial.settings, extra)).toThrow("不能写入 Codex");
});
it("rejects a late fragment preview after an extra-only edit", async () => {
  let finish!: (value: unknown) => void;
  vi.mocked(previewClientSettings).mockResolvedValueOnce({ app: "claude", target: "Claude 通用配置片段", content: "initial" }).mockImplementationOnce(() => new Promise((resolve) => { finish = resolve as (value: unknown) => void; }));
  vi.mocked(parseClientSettings).mockResolvedValue({ settings: initial.settings, claudeExtra: { env: { NEXT: "yes" } } });
  const { result } = renderHook(() => useClientSettings(deps())); await waitFor(() => expect(result.current.editorState.phase).toBe("clean"));
  act(() => result.current.previewSettings("claude")); await waitFor(() => expect(result.current.editorState.preview).toBeDefined());
  act(() => result.current.previewSettings("claude"));
  act(() => result.current.changePreviewContent("claude", "latest manual content"));
  await waitFor(() => expect(result.current.editorState.claudeExtra).toEqual({ env: { NEXT: "yes" } }));
  await act(async () => finish({ app: "claude", target: "Claude 通用配置片段", content: "stale" }));
  expect(result.current.editorState.preview?.content).toBe("latest manual content");
});
