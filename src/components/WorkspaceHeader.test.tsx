import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ModuleHeader, WorkspaceHeader } from "./WorkspaceHeader";

describe("WorkspaceHeader", () => {
  it("keeps the title alone in row 1 with the workspace h2", () => {
    render(<WorkspaceHeader title="扩展" />);

    const heading = screen.getByRole("heading", { level: 2, name: "扩展" });
    expect(heading).toHaveClass("asb-panel-title");
    expect(heading.closest(".asb-panel-heading")).toBe(heading.parentElement?.parentElement);
  });

  it("renders the back button left of the title inside row 1", () => {
    render(
      <WorkspaceHeader title="用量查询" back={<button type="button">返回</button>} />,
    );

    const row = screen.getByRole("heading", { name: "用量查询" }).closest(".asb-panel-heading")!;
    expect(row.firstElementChild).toHaveTextContent("返回");
  });

  it("pairs primary navigation left with page actions right in row 2", () => {
    render(
      <WorkspaceHeader
        title="供应商"
        primary={<div data-testid="nav" />}
        primaryActions={<button type="button">新建供应商</button>}
      />,
    );

    const row = document.querySelector(".asb-header-primary")!;
    expect(row.firstElementChild).toHaveAttribute("data-testid", "nav");
    expect(row.lastElementChild).toHaveClass("asb-panel-actions");
    expect(screen.getByRole("button", { name: "新建供应商" })).toBeInTheDocument();
  });

  it("promotes secondary to the second row when primary is absent", () => {
    const { container } = render(
      <WorkspaceHeader title="会话" secondary={<input aria-label="搜索会话" />} />,
    );

    expect(container.querySelector(".asb-header-primary")).toBeNull();
    expect(container.querySelector(".asb-header-secondary")).not.toBeNull();
  });

  it("renders the secondary view-controls row below the primary row", () => {
    const { container } = render(
      <WorkspaceHeader
        title="扩展"
        primary={<div data-testid="tabs" />}
        secondary={<input aria-label="搜索扩展" />}
      />,
    );

    const rows = Array.from(container.querySelector(".asb-workspace-header")!.children);
    expect(rows[1]).toHaveClass("asb-header-primary");
    expect(rows[2]).toHaveClass("asb-header-secondary");
  });
});

describe("ModuleHeader", () => {
  it("renders the module title as h3 with the section-title token", () => {
    render(<ModuleHeader title="模型消耗" />);

    const heading = screen.getByRole("heading", { level: 3, name: "模型消耗" });
    expect(heading).toHaveClass("asb-section-title");
  });

  it("wires the heading id for aria-labelledby owners", () => {
    render(<ModuleHeader id="runtime-overview-heading" title="运行环境" />);

    expect(screen.getByRole("heading", { name: "运行环境" })).toHaveAttribute(
      "id",
      "runtime-overview-heading",
    );
  });

  it("omits both control rows when only a title is given", () => {
    const { container } = render(<ModuleHeader title="配置状态" />);

    expect(container.querySelector(".asb-module-header")!.children).toHaveLength(1);
  });
});
