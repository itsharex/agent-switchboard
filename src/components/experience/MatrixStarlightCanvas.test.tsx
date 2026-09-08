import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MatrixStarlightCanvas } from "./MatrixStarlightCanvas";

const frames = new Map<number, FrameRequestCallback>();
const disconnect = vi.fn();
let nextFrame = 0;

beforeEach(() => {
  frames.clear();
  disconnect.mockClear();
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({
    setTransform: vi.fn(), clearRect: vi.fn(), fillRect: vi.fn(),
  } as unknown as CanvasRenderingContext2D);
  vi.spyOn(window, "getComputedStyle").mockReturnValue({ getPropertyValue: () => "239 248 255" } as unknown as CSSStyleDeclaration);
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({ width: 400, height: 240 } as DOMRect);
  vi.spyOn(document, "hidden", "get").mockReturnValue(false);
  vi.stubGlobal("ResizeObserver", class { observe() {} disconnect = disconnect; });
  vi.stubGlobal("matchMedia", () => ({ matches: false, addEventListener() {}, removeEventListener() {} }));
  vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
    frames.set(++nextFrame, callback);
    return nextFrame;
  });
  vi.stubGlobal("cancelAnimationFrame", (id: number) => frames.delete(id));
});

afterEach(() => {
  cleanup();
  delete document.documentElement.dataset.motion;
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

describe("starlight lifecycle", () => {
  it("cancels frames while the document is hidden and after unmount", () => {
    const { unmount } = render(<MatrixStarlightCanvas variant="cool" />);
    expect(frames.size).toBe(1);
    vi.spyOn(document, "hidden", "get").mockReturnValue(true);
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    expect(frames.size).toBe(0);
    vi.spyOn(document, "hidden", "get").mockReturnValue(false);
    act(() => document.dispatchEvent(new Event("visibilitychange")));
    expect(frames.size).toBe(1);
    unmount();
    expect(frames.size).toBe(0);
    expect(disconnect).toHaveBeenCalledOnce();
  });

  it("renders a static matrix for app or system reduced motion and responds to live settings", async () => {
    document.documentElement.dataset.motion = "reduce";
    const { unmount } = render(<MatrixStarlightCanvas variant="violet" />);
    expect(frames.size).toBe(0);
    await act(async () => { delete document.documentElement.dataset.motion; });
    expect(frames.size).toBe(1);
    await act(async () => { document.documentElement.dataset.motion = "reduce"; });
    expect(frames.size).toBe(0);
    unmount();
    delete document.documentElement.dataset.motion;
    vi.stubGlobal("matchMedia", () => ({ matches: true, addEventListener() {}, removeEventListener() {} }));
    render(<MatrixStarlightCanvas variant="cool" />);
    expect(frames.size).toBe(0);
  });
});
