import "@testing-library/jest-dom/vitest";
import { afterEach, vi } from "vitest";
import { cleanup } from "@testing-library/react";
import { cloneElement, createElement, isValidElement, type ReactElement, type ReactNode } from "react";
import { clearToasts } from "../components/use-toast";

vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

// jsdom reports every element as 0x0 and has no ResizeObserver, so ChartFrame
// never measures a positive size and renders nothing. Swap in a fixed-size
// passthrough that stamps the plot size onto the chart element; plot
// geometry is not under test. ChartFrame's own contract is covered by its
// dedicated test file, which unmocks this module.
vi.mock("../components/charts/ChartFrame", () => ({
  ChartFrame: ({ children }: { children: ReactNode }) =>
    createElement(
      "div",
      { style: { width: 640, height: 240 } },
      isValidElement(children)
        ? cloneElement(children as ReactElement<{ width?: number; height?: number }>, {
            width: 640,
            height: 240,
          })
        : children,
    ),
}));

// jsdom has no canvas implementation and logs an error on getContext;
// the particle effect intentionally bails out when the context is null.
// Node-environment suites (e.g. boundary.test.ts) have no DOM globals at
// all — the jsdom-only stubs below are guarded for them.
if (typeof HTMLCanvasElement !== "undefined") {
  HTMLCanvasElement.prototype.getContext = () => null;
}

// jsdom lacks the pointer-capture APIs Radix Select uses for its listbox
// pointer handling, and scrollIntoView for focusing the open menu's chosen
// item; stub them so dropdown interactions can be tested.
if (typeof Element !== "undefined") {
  Element.prototype.hasPointerCapture = () => false;
  Element.prototype.setPointerCapture = () => {};
  Element.prototype.releasePointerCapture = () => {};
  Element.prototype.scrollIntoView = () => {};
}

afterEach(() => {
  cleanup();
  // Error toasts never auto-close; the module-level store must not leak
  // alerts from one test into the next.
  clearToasts();
});
