import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";

// The global setup swaps ChartFrame for a fixed-size passthrough because jsdom
// cannot measure; this file tests the real contract and opts back in.
vi.unmock("./ChartFrame");

import { ChartFrame } from "./ChartFrame";

let triggerResize: (() => void) | undefined;

class ResizeObserverStub {
  private readonly callback: () => void;
  constructor(callback: () => void) {
    this.callback = callback;
  }
  observe() {
    triggerResize = this.callback;
  }
  unobserve() {
    triggerResize = undefined;
  }
  disconnect() {
    triggerResize = undefined;
  }
}

function stubBox(width: number, height: number) {
  vi.spyOn(Element.prototype, "getBoundingClientRect").mockReturnValue({
    x: 0,
    y: 0,
    top: 0,
    left: 0,
    right: width,
    bottom: height,
    width,
    height,
    toJSON: () => ({}),
  } as DOMRect);
}

describe("ChartFrame", () => {
  beforeEach(() => {
    vi.stubGlobal("ResizeObserver", ResizeObserverStub);
  });
  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("盒子尺寸为 0 时不挂载图表", () => {
    stubBox(0, 0);
    const { container } = render(
      <ChartFrame>
        <p>plot</p>
      </ChartFrame>,
    );
    expect(screen.queryByText("plot")).toBeNull();
    expect(container.firstElementChild).toBeTruthy();
    expect(container.firstElementChild?.childElementCount).toBe(0);
  });

  it("盒子可见时挂载图表", () => {
    stubBox(320, 160);
    render(
      <ChartFrame>
        <p>plot</p>
      </ChartFrame>,
    );
    expect(screen.getByText("plot")).toBeInTheDocument();
  });

  it("盒子隐藏后卸载图表，恢复可见后重新挂载", () => {
    stubBox(320, 160);
    render(
      <ChartFrame>
        <p>plot</p>
      </ChartFrame>,
    );
    expect(screen.getByText("plot")).toBeInTheDocument();

    stubBox(0, 0);
    act(() => triggerResize?.());
    expect(screen.queryByText("plot")).toBeNull();

    stubBox(320, 160);
    act(() => triggerResize?.());
    expect(screen.getByText("plot")).toBeInTheDocument();
  });
});
