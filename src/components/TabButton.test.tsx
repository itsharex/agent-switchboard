import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TabButton } from "./TabButton";

describe("TabButton", () => {
  it("uses tab semantics when requested", () => {
    render(
      <TabButton role="tab" selected controls="panel-a" tabId="tab-a">
        选项
      </TabButton>,
    );

    const tab = screen.getByRole("tab", { name: "选项" });
    expect(tab).toHaveAttribute("aria-selected", "true");
    expect(tab).toHaveAttribute("aria-controls", "panel-a");
    expect(tab).not.toHaveAttribute("aria-pressed");
  });

  it("uses pressed-button semantics by default", () => {
    render(<TabButton selected={false}>筛选</TabButton>);

    const button = screen.getByRole("button", { name: "筛选" });
    expect(button).toHaveAttribute("aria-pressed", "false");
    expect(button).not.toHaveAttribute("aria-selected");
  });
});
