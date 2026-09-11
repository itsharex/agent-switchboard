import { render, screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { Button } from "./Button";

const buttonsCss = readFileSync(resolve(process.cwd(), "src/styles/base/buttons.css"), "utf8");

describe("Button", () => {
  it("emits the shared standard action contract", () => {
    render(<Button variant="primary">保存</Button>);

    expect(screen.getByRole("button", { name: "保存" })).toHaveClass(
      "asb-btn",
      "asb-btn-primary",
    );
  });

  it("supports intentionally unstyled action surfaces", () => {
    render(<Button variant="unstyled">自定义操作</Button>);

    expect(screen.getByRole("button", { name: "自定义操作" })).toHaveClass(
      "asb-btn",
      "asb-btn-unstyled",
    );
  });

  it("keeps icon-only glyphs legible within their shared touch target", () => {
    const iconRule = buttonsCss.match(/\.asb-btn-icon > svg\s*\{[^}]+\}/)?.[0] ?? "";

    expect(iconRule).toContain("flex: none");
    expect(iconRule).toContain("width: 20px");
    expect(iconRule).toContain("height: 20px");
  });
});
