import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { previewSwitch } from "../api/client";
import { deferred, filePreview } from "../test/app-fixtures";
import { useSwitchPreview } from "./useSwitchPreview";

vi.mock("../api/client", () => ({ previewSwitch: vi.fn() }));

const profile = { id: "claude-gateway" };
const previewMock = vi.mocked(previewSwitch);

beforeEach(() => previewMock.mockReset().mockResolvedValue(filePreview));

function setup(busy = false) {
  const deps = { busy, setSelectedId: vi.fn(), onError: vi.fn(), clearError: vi.fn() };
  return { ...renderHook(() => useSwitchPreview(deps)), deps };
}

describe("useSwitchPreview intent", () => {
  it("only produces a switch candidate for explicit activation and drops intent on cancellation", async () => {
    const { result } = setup();
    await act(() => result.current.previewProfile(profile));
    expect(result.current.preview?.file).toEqual(filePreview);
    expect(result.current.switchCandidate).toBeNull();

    await act(() => result.current.activateProfile(profile));
    expect(result.current.switchCandidate?.profileId).toBe(profile.id);
    act(() => result.current.cancelSwitch());
    expect(result.current.switchCandidate).toBeNull();
    expect(result.current.preview?.file).toEqual(filePreview);

    await act(() => result.current.previewProfile(profile));
    expect(result.current.switchCandidate).toBeNull();
    expect(previewMock).toHaveBeenCalledTimes(3);
  });

  it("shares an in-flight read but never carries its earlier activation intent into a later preview", async () => {
    const pending = deferred<typeof filePreview>();
    previewMock.mockReturnValue(pending.promise);
    const { result } = setup();
    act(() => { void result.current.activateProfile(profile); });
    act(() => { void result.current.previewProfile(profile); });
    await act(async () => { pending.resolve(filePreview); });

    expect(previewMock).toHaveBeenCalledTimes(1);
    expect(result.current.preview?.profileId).toBe(profile.id);
    expect(result.current.switchCandidate).toBeNull();
  });

  it("ignores an activation result after the preview was retracted", async () => {
    const pending = deferred<typeof filePreview>();
    previewMock.mockReturnValue(pending.promise);
    const { result } = setup();
    act(() => { void result.current.activateProfile(profile); });
    act(() => result.current.retractPreview());
    await act(async () => { pending.resolve(filePreview); });

    expect(result.current.preview).toBeNull();
    expect(result.current.switchCandidate).toBeNull();
  });

  it("starts a fresh activation read even when the same profile has an earlier preview in flight", async () => {
    const pending = deferred<typeof filePreview>();
    const fresh = { ...filePreview, renderedHash: "activation-rendered" };
    previewMock.mockReturnValueOnce(pending.promise).mockResolvedValueOnce(fresh);
    const { result } = setup();
    act(() => { void result.current.previewProfile(profile); });
    await act(() => result.current.activateProfile(profile));
    await act(async () => { pending.resolve(filePreview); });
    expect(previewMock).toHaveBeenCalledTimes(2);
    expect(result.current.switchCandidate?.file).toEqual(fresh);
  });

  it("invalidates pending reads after a write and preserves the newer read-only result", async () => {
    const pending = deferred<typeof filePreview>();
    const fresh = { ...filePreview, renderedHash: "fresh-rendered" };
    previewMock.mockReturnValueOnce(pending.promise).mockResolvedValueOnce(fresh);
    const { result, deps } = setup();
    act(() => { void result.current.activateProfile(profile); });
    act(() => result.current.invalidateSwitchCandidates());
    await act(() => result.current.previewProfile(profile));
    await act(async () => { pending.resolve(filePreview); });

    expect(previewMock).toHaveBeenCalledTimes(2);
    expect(result.current.preview?.file).toEqual(fresh);
    expect(result.current.switchCandidate).toBeNull();
    expect(deps.onError).not.toHaveBeenCalled();
  });

  it("does not read or create a candidate during another write", async () => {
    const { result } = setup(true);
    await act(() => result.current.previewProfile(profile));
    await act(() => result.current.activateProfile(profile));
    expect(previewMock).not.toHaveBeenCalled();
    expect(result.current.switchCandidate).toBeNull();
  });

  it("clears an earlier preview when activation fails and preserves the diagnostic", async () => {
    const failure = { code: "preview-failed", message: "Cannot read target configuration" };
    const { result, deps } = setup();
    await act(() => result.current.previewProfile(profile));
    previewMock.mockRejectedValueOnce(failure);
    await act(() => result.current.activateProfile(profile));
    expect(result.current.preview).toBeNull();
    expect(result.current.switchCandidate).toBeNull();
    expect(deps.onError).toHaveBeenCalledWith(failure);
  });
});
