import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

const baseCss = readFileSync(join(dirname(fileURLToPath(import.meta.url)), "base.css"), "utf8");

function rule(selector: RegExp): string {
  return baseCss.match(selector)?.[1] ?? "";
}

describe("primary navigation state", () => {
  it("uses text hierarchy for the current page instead of a selected surface", () => {
    const idle = rule(/\.asb-nav button\s*\{([^}]*)\}/);
    const hover = rule(/\.asb-nav button:hover\s*\{([^}]*)\}/);
    const current = rule(/\.asb-nav button\[aria-current="page"\]\s*\{([^}]*)\}/);

    expect(idle).toContain("font-weight: 500");
    expect(hover).toContain("color: var(--asb-text)");
    expect(hover).not.toMatch(/(?:background|border)/);
    expect(current).toContain("color: var(--asb-text)");
    expect(current).toContain("font-weight: 700");
    expect(current).not.toMatch(/(?:background|border|text-decoration)/);
  });

  it("gives the overflow trigger the same current-page treatment", () => {
    const active = rule(/\.asb-nav-more > button\[data-active\]\s*\{([^}]*)\}/);

    expect(active).toContain("color: var(--asb-text)");
    expect(active).toContain("font-weight: 700");
    expect(active).not.toMatch(/(?:background|border|text-decoration)/);
  });

  it("renders the collapsed panel as a menu surface above workspace layers", () => {
    const panel = rule(/\.asb-nav-more-menu\s*\{([^}]*)\}/);

    expect(panel).toContain("position: absolute");
    expect(panel).toContain("background: var(--asb-surface-menu)");
    expect(panel).toContain("z-index: 60");
  });
});
