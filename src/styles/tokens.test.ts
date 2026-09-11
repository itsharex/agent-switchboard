import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const srcRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const tokenOwner = "styles/tokens.css";

// The vendored 图表组件层 foundation is a quarantined second owner: it defines
// the upstream design-system primitives the 图表组件层 components consume. App
// code must never add raw colors outside tokens.css or this file.
const vendoredFoundationOwners = new Set(["styles/theme.css"]);

function collectSourceFiles(dir: string): string[] {
  const files: string[] = [];
  for (const entry of readdirSync(dir)) {
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      if (entry === "test") continue;
      files.push(...collectSourceFiles(full));
    } else if (/\.(ts|tsx|css)$/.test(entry)) {
      files.push(full);
    }
  }
  return files;
}

describe("token ownership", () => {
  it("defines visual constants only in the token stylesheet", () => {
    const offenders: string[] = [];
    for (const file of collectSourceFiles(srcRoot)) {
      const rel = relative(srcRoot, file).replace(/\\/g, "/");
      if (rel === tokenOwner || vendoredFoundationOwners.has(rel)) continue;
      const text = readFileSync(file, "utf8");
      const hex = text.match(/#[0-9a-fA-F]{3,8}\b/g);
      if (hex) offenders.push(`${rel}: raw color ${hex.join(", ")}`);
    }
    expect(offenders).toEqual([]);
  });

  it("keeps the vendored boardui utility classes quarantined in the chart layer", () => {
    // The chart foundation (boardui.css) loads its Tailwind utilities globally,
    // so nothing stops app components from consuming the second visual
    // language. This guard is the fence: only components/charts/** may use the
    // vendored color/type/radius utility classes (user directive 2026-09-11).
    const boarduiUtility =
      /(?:text-(?:large-title|display-[1-4]|title-[1-3]|headline|body-2|body|caption-[12])-(?:regular|medium|semibold|bold)|text-text-|text-foreground-|text-status-|bg-background-|bg-status-|bg-stat-card-icon-background|rounded-(?:2lg|2xl|3xl))/;
    const offenders: string[] = [];
    for (const file of collectSourceFiles(srcRoot)) {
      const rel = relative(srcRoot, file).replace(/\\/g, "/");
      if (!rel.endsWith(".tsx")) continue;
      if (rel.startsWith("components/charts/")) continue;
      if (/\.(test|spec)\./.test(rel)) continue;
      const match = readFileSync(file, "utf8").match(boarduiUtility);
      if (match) offenders.push(`${rel}: ${match[0]}`);
    }
    expect(offenders).toEqual([]);
  });

  it("uses one opaque surface path without backdrop sampling or exit states", () => {
    const tokens = readFileSync(join(srcRoot, tokenOwner), "utf8");
    const base = readFileSync(join(srcRoot, "styles/base.css"), "utf8");

    expect(tokens).toContain("--asb-surface-rail:");
    expect(tokens).toContain("--asb-surface-menu:");
    expect(tokens).toContain("--asb-surface-sheet:");
    expect(tokens).not.toMatch(/--asb-(blur|glass|saturation)-/);
    expect(base).not.toMatch(/(?:-webkit-)?backdrop-filter|filter:\s*blur\(/);
    expect(base).not.toMatch(
      /button:not\(:disabled\):active|asb-(select-in|tooltip-in|sheet-in|toast-in|toast-out)|is-leaving/,
    );
  });

  it("keeps module perimeters neutral instead of using colored decorative strips", () => {
    const tokens = readFileSync(join(srcRoot, tokenOwner), "utf8");
    const base = readFileSync(join(srcRoot, "styles/base.css"), "utf8");

    expect(tokens).not.toMatch(/--asb-accent-(?:ring|border)-/);
    expect(base).not.toMatch(/conic-gradient\(/);
    expect(base).not.toMatch(
      /border-left:\s*\d+px\s+solid\s+var\(--asb-(?:action|warning|danger)\)/,
    );
  });

  it("supports explicit theme and motion overrides without a second stylesheet owner", () => {
    const tokens = readFileSync(join(srcRoot, tokenOwner), "utf8");
    expect(tokens).toContain(':root:not([data-theme="light"])');
    expect(tokens).toContain(':root[data-theme="dark"]');
    expect(tokens).toContain(':root[data-motion="reduce"]');
    expect(tokens).toContain('--asb-motion-fast: 0ms');
  });
});

/*
 * Typography and layout guards (user directive 2026-09-11).
 *
 * These three tests are the reason the type ramp, the control heights and the
 * spacing rhythm cannot silently rot again:
 *
 *  1. a consumed `--asb-*` token must exist (the old `--asb-text-caption` /
 *     `--asb-text-primary` references resolved to nothing and the text fell
 *     back to its parent's font — invisible in review, wrong on screen);
 *  2. spacing and sizing declarations must name their value: either a token,
 *     a `1px` hairline, an icon-sized glyph, or a locally declared
 *     `--asb-*` constant (a number is allowed to exist, but it must have a
 *     name — a bare literal in a declaration is what made the extension page
 *     stack four different control heights on one screen);
 *  3. every heading is claimed by a class, because root.css resets heading
 *     font and margin to `inherit`.
 */

const spacingSizingProperties = [
  "padding",
  "padding-top",
  "padding-right",
  "padding-bottom",
  "padding-left",
  "padding-inline",
  "padding-inline-start",
  "padding-inline-end",
  "padding-block",
  "padding-block-start",
  "padding-block-end",
  "margin",
  "margin-top",
  "margin-right",
  "margin-bottom",
  "margin-left",
  "margin-inline",
  "margin-inline-start",
  "margin-inline-end",
  "margin-block",
  "margin-block-start",
  "margin-block-end",
  "gap",
  "row-gap",
  "column-gap",
  "inset",
  "inset-inline",
  "inset-block",
  "top",
  "right",
  "bottom",
  "left",
  "width",
  "height",
  "min-width",
  "max-width",
  "min-height",
  "max-height",
  "flex-basis",
  "border-radius",
];

const intrinsicSizeProperties = new Set([
  "width",
  "height",
  "min-width",
  "max-width",
  "min-height",
  "max-height",
  "flex-basis",
]);

// A selector that sizes a glyph rather than a layout box. Icon and logo
// dimensions are the one sanctioned bare-px size (DESIGN.md §5).
const glyphSelector = /icon|logo|svg|img|glyph/i;

function stripCssComments(text: string): string {
  return text.replace(/\/\*[\s\S]*?\*\//g, (comment) => comment.replace(/[^\n]/g, " "));
}

describe("typography and layout scale", () => {
  it("resolves every consumed --asb-* custom property to a definition", () => {
    const defined = new Set<string>();
    const consumed: Array<{ rel: string; token: string }> = [];
    for (const file of collectSourceFiles(srcRoot)) {
      const rel = relative(srcRoot, file).replace(/\\/g, "/");
      const text = stripCssComments(readFileSync(file, "utf8"));
      // A definition is `--asb-x: value` in CSS or `"--asb-x": value` in a
      // React inline style object — both are real owners of the property.
      for (const match of text.matchAll(/(--asb-[a-z0-9-]+)"?\s*:/g)) defined.add(match[1]);
      for (const match of text.matchAll(/var\(\s*(--asb-[a-z0-9-]+)/g)) {
        consumed.push({ rel, token: match[1] });
      }
    }
    const offenders = consumed
      .filter(({ token }) => !defined.has(token))
      .map(({ rel, token }) => `${rel}: undefined ${token}`);
    expect(offenders).toEqual([]);
  });

  it("names every spacing and size instead of writing a bare pixel literal", () => {
    const offenders: string[] = [];
    for (const file of collectSourceFiles(srcRoot)) {
      const rel = relative(srcRoot, file).replace(/\\/g, "/");
      if (rel === tokenOwner || vendoredFoundationOwners.has(rel)) continue;
      // Stylesheets only. A .tsx file is not parsed as CSS here: object
      // literals such as a chart's `top: { width: 12 }` would read as
      // declarations. Layout numbers in TSX are geometry, not stylesheet
      // values, and are reported by review rather than by this scan.
      if (!rel.endsWith(".css")) continue;
      const text = stripCssComments(readFileSync(file, "utf8"));
      for (const rule of text.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
        const selector = rule[1].trim().replace(/\s+/g, " ");
        const body = rule[2];
        // A locally declared constant is the sanctioned home for one
        // component's intrinsic geometry: the number still has a name.
        const declarations = body.matchAll(
          /(?:^|;)\s*(--asb-[a-z0-9-]+|[a-z-]+)\s*:\s*([^;]+)/g,
        );
        for (const [, property, value] of declarations) {
          if (!spacingSizingProperties.includes(property)) continue;
          const literals = value.match(/\d+(?:\.\d+)?px/g);
          if (!literals) continue;
          if (literals.every((literal) => literal === "1px")) continue;
          if (intrinsicSizeProperties.has(property) && glyphSelector.test(selector)) continue;
          offenders.push(`${rel}: ${selector} { ${property}: ${value.trim()} }`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  it("claims every heading with a class, since root.css resets heading defaults", () => {
    const root = readFileSync(join(srcRoot, "styles/base/root.css"), "utf8");
    expect(root).toMatch(/h1,\s*h2,\s*h3,\s*h4,\s*h5,\s*h6\s*\{[^}]*margin:\s*0/);
    expect(root).toMatch(/h1,\s*h2,\s*h3,\s*h4,\s*h5,\s*h6\s*\{[^}]*font:\s*inherit/);

    const offenders: string[] = [];
    for (const file of collectSourceFiles(srcRoot)) {
      const rel = relative(srcRoot, file).replace(/\\/g, "/");
      if (!rel.endsWith(".tsx") || /\.(test|spec)\./.test(rel)) continue;
      const text = readFileSync(file, "utf8");
      for (const match of text.matchAll(/<h[1-6]\b[^>]*>/g)) {
        if (!/className=/.test(match[0])) offenders.push(`${rel}: ${match[0]}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it("sets heading type only through the three heading classes", () => {
    // The counterpart to the reset above. Typography for a heading comes from
    // `.asb-panel-title` / `.asb-section-title` / `.asb-group-title`; a
    // component that reaches a heading through an element selector
    // (`.asb-card h4 { font: … }`) silently outranks whichever class the
    // heading carries and flattens the ramp — the bug that once rendered a
    // dialog's h2 and its inner h3 at the same size. Layout, colour and
    // truncation on a heading are still fine; only type is fenced.
    const offenders: string[] = [];
    for (const file of collectSourceFiles(srcRoot)) {
      const rel = relative(srcRoot, file).replace(/\\/g, "/");
      if (!rel.endsWith(".css") || vendoredFoundationOwners.has(rel)) continue;
      const text = stripCssComments(readFileSync(file, "utf8"));
      for (const rule of text.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
        const selector = rule[1].trim().replace(/\s+/g, " ");
        // A bare heading element, i.e. not `.asb-foo` and not a class name
        // that merely starts with those letters.
        if (!/(?:^|[\s,>+~])h[1-6](?![\w-])/.test(selector)) continue;
        for (const [, property, value] of rule[2].matchAll(
          /(?:^|;)\s*(font(?:-size|-weight|-family|-style|-variant)?)\s*:\s*([^;]+)/g,
        )) {
          if (property === "font" && value.trim() === "inherit") continue;
          offenders.push(`${rel}: ${selector} { ${property}: ${value.trim()} }`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });
});
