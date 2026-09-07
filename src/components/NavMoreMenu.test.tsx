import { fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import type { Page } from "../app/AppShell";
import { NavMoreMenu } from "./NavMoreMenu";

const items: readonly Page[] = ["扩展", "用量", "网关", "日志", "备份", "发现"];

function renderMenu(page: Page, onPageChange = vi.fn()) {
  render(<NavMoreMenu items={items} page={page} onPageChange={onPageChange} />);
  return { onPageChange };
}

describe("NavMoreMenu", () => {
  it("labels the trigger 更多 while a primary page is current", () => {
    renderMenu("概览");
    const trigger = screen.getByRole("button", { name: "更多" });
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(trigger).not.toHaveAttribute("data-active");
  });

  it("shows the current overflow page name on the trigger with the active flag", () => {
    renderMenu("备份");
    expect(screen.getByRole("button", { name: "备份" })).toHaveAttribute("data-active", "true");
  });

  it("opens on click and marks the current page inside the panel", async () => {
    const user = userEvent.setup();
    renderMenu("备份");
    await user.click(screen.getByRole("button", { name: "备份" }));

    const panel = screen.getByRole("list", { name: "更多页面" });
    expect(within(panel).getByRole("button", { name: "扩展" })).toBeInTheDocument();
    expect(within(panel).getByRole("button", { name: "备份" })).toHaveAttribute(
      "aria-current",
      "page",
    );
  });

  it("navigates, closes, and refocuses the trigger on selection", async () => {
    const onPageChange = vi.fn();
    const user = userEvent.setup();
    renderMenu("概览", onPageChange);

    await user.click(screen.getByRole("button", { name: "更多" }));
    await user.click(screen.getByRole("button", { name: "扩展" }));

    expect(onPageChange).toHaveBeenCalledWith("扩展");
    expect(screen.queryByRole("list", { name: "更多页面" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "更多" })).toHaveFocus();
  });

  it("closes on Escape and returns focus to the trigger", async () => {
    const user = userEvent.setup();
    renderMenu("概览");
    const trigger = screen.getByRole("button", { name: "更多" });
    await user.click(trigger);

    const panel = screen.getByRole("list", { name: "更多页面" });
    fireEvent.keyDown(within(panel).getByRole("button", { name: "扩展" }), { key: "Escape" });

    expect(screen.queryByRole("list", { name: "更多页面" })).not.toBeInTheDocument();
    expect(trigger).toHaveFocus();
  });

  it("closes on an outside pointer press", async () => {
    const user = userEvent.setup();
    renderMenu("概览");
    await user.click(screen.getByRole("button", { name: "更多" }));

    fireEvent.pointerDown(document.body);

    expect(screen.queryByRole("list", { name: "更多页面" })).not.toBeInTheDocument();
  });
});
