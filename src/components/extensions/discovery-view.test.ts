import { describe, expect, it } from "vitest";
import {
  diagnosticInView,
  observationInView,
  type BindingViewInfo,
  type DiscoveryViewFilter,
} from "./discovery-view";
import type { ExtensionDiagnostic, ObservedExtension } from "../../api/client";

const skillFilter: DiscoveryViewFilter = { kind: "skill", client: "all", search: "" };

function observed(overrides: Partial<ObservedExtension>): ObservedExtension {
  return {
    observationId: "obs-1",
    kind: "skill",
    client: "codex",
    name: "api-spec",
    origin: { origin: "userRoot" },
    managed: false,
    contentDigest: null,
    actions: {
      import: { supported: true, inLibrary: false },
      takeover: { supported: true },
    },
    ...overrides,
  };
}

function diagnostic(overrides: Partial<ExtensionDiagnostic>): ExtensionDiagnostic {
  return {
    id: "diag-1",
    code: "managedTargetMissing",
    client: "codex",
    subject: { kind: "managedBinding", bindingId: "bind-skill" },
    message: "托管 Skill 目录 api-spec 已缺失",
    remediation: { kind: "auto", reason: "可恢复" },
    ...overrides,
  };
}

const bindings = new Map<string, BindingViewInfo>([
  ["bind-skill", { name: "api-spec", kind: "skill", client: "codex" }],
  ["bind-mcp", { name: "docs", kind: "mcp", client: "claude" }],
]);

describe("discovery view scoping", () => {
  it("shows only the active tab's kind and applies the client filter", () => {
    expect(observationInView(observed({ kind: "skill" }), skillFilter)).toBe(true);
    expect(observationInView(observed({ kind: "mcp" }), skillFilter)).toBe(false);
    expect(
      observationInView(observed({ client: "claude" }), {
        ...skillFilter,
        client: "codex",
      }),
    ).toBe(false);
    expect(
      observationInView(observed({ client: "codex" }), {
        ...skillFilter,
        client: "codex",
      }),
    ).toBe(true);
  });

  it("filters rows by name or description search text", () => {
    expect(
      observationInView(observed({ name: "api-spec" }), {
        ...skillFilter,
        search: "API",
      }),
    ).toBe(true);
    expect(
      observationInView(
        observed({ name: "other", description: "撰写接口规范" }),
        { ...skillFilter, search: "接口" },
      ),
    ).toBe(true);
    expect(
      observationInView(observed({ name: "other" }), {
        ...skillFilter,
        search: "api",
      }),
    ).toBe(false);
  });

  it("keeps entry diagnostics in step with their visible row", () => {
    const viewIds = new Set(["obs-1"]);
    const entry = diagnostic({
      subject: { kind: "discoveryEntry", observationId: "obs-1" },
    });
    expect(
      diagnosticInView(entry, skillFilter, viewIds, bindings),
    ).toBe(true);
    expect(
      diagnosticInView(entry, skillFilter, new Set(), bindings),
    ).toBe(false);
  });

  it("scopes managed-binding diagnostics by the binding's kind, client, and name", () => {
    const skillDiagnostic = diagnostic({});
    expect(
      diagnosticInView(skillDiagnostic, skillFilter, new Set(), bindings),
    ).toBe(true);
    expect(
      diagnosticInView(skillDiagnostic, { ...skillFilter, kind: "mcp" }, new Set(), bindings),
    ).toBe(false);
    expect(
      diagnosticInView(
        skillDiagnostic,
        { ...skillFilter, client: "claude" },
        new Set(),
        bindings,
      ),
    ).toBe(false);
    expect(
      diagnosticInView(
        skillDiagnostic,
        { ...skillFilter, search: "docs" },
        new Set(),
        bindings,
      ),
    ).toBe(false);
    expect(
      diagnosticInView(
        skillDiagnostic,
        { ...skillFilter, search: "api" },
        new Set(),
        bindings,
      ),
    ).toBe(true);
    // An unknown binding never silently belongs to a view.
    expect(
      diagnosticInView(
        diagnostic({ subject: { kind: "managedBinding", bindingId: "bind-x" } }),
        skillFilter,
        new Set(),
        bindings,
      ),
    ).toBe(false);
  });

  it("never hides scan-location problems behind a search filter", () => {
    const location = diagnostic({
      code: "mcpCollectionInvalid",
      subject: { kind: "scanLocation", label: "MCP 配置文档", resourceKind: "mcp" },
    });
    // The kind still scopes it to the right tab...
    expect(
      diagnosticInView(location, skillFilter, new Set(), bindings),
    ).toBe(false);
    // ...but a search that hides every row cannot hide it.
    expect(
      diagnosticInView(
        location,
        { ...skillFilter, kind: "mcp", search: "nothing-matches-this" },
        new Set(),
        bindings,
      ),
    ).toBe(true);
    // The client filter still applies.
    expect(
      diagnosticInView(
        diagnostic({
          code: "mcpCollectionInvalid",
          client: "claude",
          subject: { kind: "scanLocation", label: "MCP 配置文档", resourceKind: "mcp" },
        }),
        { ...skillFilter, kind: "mcp", client: "codex" },
        new Set(),
        bindings,
      ),
    ).toBe(false);
  });
});
