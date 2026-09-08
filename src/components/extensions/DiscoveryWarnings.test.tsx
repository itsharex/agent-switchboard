import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DiscoveryWarnings } from "./DiscoveryWarnings";
import type { BindingViewInfo } from "./discovery-view";
import type { ExtensionDiagnostic } from "../../api/client";

const bindingInfo = new Map<string, BindingViewInfo>([
  ["bind-skill", { name: "api-spec", kind: "skill", client: "codex" }],
]);

function diagnostic(overrides: Partial<ExtensionDiagnostic>): ExtensionDiagnostic {
  return {
    id: "diag-1",
    code: "managedTargetMissing",
    client: "codex",
    subject: { kind: "managedBinding", bindingId: "bind-1" },
    message: "托管 Skill 目录 api-spec 已缺失",
    remediation: { kind: "auto", reason: "本地内容库保存了已部署版本" },
    ...overrides,
  };
}

const autoDiagnostic = diagnostic({
  id: "diag-auto",
  subject: { kind: "managedBinding", bindingId: "bind-skill" },
});

function renderWarnings(
  diagnostics: ExtensionDiagnostic[],
  props: Partial<Parameters<typeof DiscoveryWarnings>[0]> = {},
) {
  const onOpenChange = vi.fn();
  const view = render(
    <DiscoveryWarnings
      diagnostics={diagnostics}
      scanId="scan-1"
      scannedAt="2026-09-07T00:00:00.000Z"
      stale={false}
      busy={false}
      repairPreparing={false}
      observationNames={new Map([["obs-1", "api-spec"]])}
      bindingInfo={bindingInfo}
      focusDiagnosticId={null}
      open={false}
      onOpenChange={onOpenChange}
      onRepair={vi.fn()}
      {...props}
    />,
  );
  return { onOpenChange, onRepair: vi.fn(), unmount: view.unmount };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("DiscoveryWarnings", () => {
  it("rests collapsed on exactly one counted line with its actions", () => {
    const { onOpenChange } = renderWarnings([autoDiagnostic]);

    const toggle = screen.getByRole("button", { name: /展开警告详情/ });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(toggle).toHaveAttribute("aria-controls");
    expect(screen.getByText("当前结果有 1 条警告，1 条可修复")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "修复这 1 项可修复警告" })).toBeEnabled();
    // The summary row is the only toggle; a separate expand button would
    // duplicate it.
    expect(screen.queryByRole("button", { name: "展开" })).not.toBeInTheDocument();
    // The details are not rendered while collapsed.
    expect(screen.queryByText(/可自动修复/)).not.toBeInTheDocument();
    expect(onOpenChange).not.toHaveBeenCalledWith(true);
  });

  it("expands into per-object diagnostics and never merges same-text objects", async () => {
    const user = userEvent.setup();
    const duplicated =
      "SKILL.md 的 frontmatter 无法解析：frontmatter 缺少 name 字段";
    renderWarnings(
      [
        autoDiagnostic,
        diagnostic({
          id: "diag-manual-1",
          code: "skillFrontmatterInvalid",
          client: "codex",
          subject: { kind: "discoveryEntry", observationId: "obs-1" },
          message: duplicated,
          remediation: { kind: "manual", reason: "请补齐后重新扫描" },
        }),
        diagnostic({
          id: "diag-manual-2",
          code: "skillFrontmatterInvalid",
          client: "claude",
          subject: { kind: "discoveryEntry", observationId: "obs-1" },
          message: duplicated,
          remediation: { kind: "manual", reason: "请补齐后重新扫描" },
        }),
      ],
      { open: true },
    );

    // Two distinct objects with identical text stay two diagnostics.
    const items = screen.getAllByRole("listitem");
    expect(items).toHaveLength(3);
    expect(screen.getAllByText(duplicated)).toHaveLength(2);
    // The managed-binding diagnostic names its extension, not an opaque id.
    expect(screen.getAllByText("api-spec").length).toBeGreaterThanOrEqual(1);

    // Every diagnostic names its remediation class and reason.
    expect(screen.getAllByText(/需人工处理：请补齐后重新扫描/)).toHaveLength(2);
    expect(screen.getByText(/可自动修复：本地内容库保存了已部署版本/)).toBeInTheDocument();

    const toggle = screen.getByRole("button", { name: "收起警告详情" });
    // The expanded state likewise keeps the summary row as the only toggle.
    expect(screen.queryByRole("button", { name: "收起" })).not.toBeInTheDocument();
    await user.click(toggle);
  });

  it("offers no fake repair button when nothing is auto-repairable", () => {
    renderWarnings(
      [
        diagnostic({
          id: "diag-info",
          code: "mcpUnknownFields",
          subject: { kind: "discoveryEntry", observationId: "obs-1" },
          message: "未识别字段 mystery；按原文保留",
          remediation: { kind: "info" },
        }),
      ],
      { open: true },
    );

    expect(screen.getByText("当前结果有 0 条警告，0 条可修复，1 条提示")).toBeInTheDocument();
    expect(screen.getByText("未识别字段 mystery；按原文保留")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /修复这/ })).not.toBeInTheDocument();
  });

  it("collapses again whenever a new scan replaces the snapshot", () => {
    const onOpenChange = vi.fn();
    const view = render(
      <DiscoveryWarnings
        diagnostics={[autoDiagnostic]}
        scanId="scan-1"
        scannedAt="2026-09-07T00:00:00.000Z"
        stale={false}
        busy={false}
        repairPreparing={false}
        observationNames={new Map()}
        bindingInfo={new Map()}
        focusDiagnosticId={null}
        open={true}
        onOpenChange={onOpenChange}
        onRepair={vi.fn()}
      />,
    );
    expect(screen.getAllByRole("listitem")).toHaveLength(1);
    view.rerender(
      <DiscoveryWarnings
        diagnostics={[autoDiagnostic]}
        scanId="scan-2"
        scannedAt="2026-09-07T00:01:00.000Z"
        stale={false}
        busy={false}
        repairPreparing={false}
        observationNames={new Map()}
        bindingInfo={new Map()}
        focusDiagnosticId={null}
        open={true}
        onOpenChange={onOpenChange}
        onRepair={vi.fn()}
      />,
    );
    // The new scan demands the collapsed resting state again.
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("marks stale results as not refreshed instead of silently showing them", () => {
    renderWarnings([autoDiagnostic], {
      stale: true,
      scannedAt: "2026-09-07T08:00:00.000Z",
      bindingInfo,
    });

    expect(screen.getByText(/上次扫描未更新/)).toBeInTheDocument();
    expect(
      screen.getByText(/上次扫描结果生成于 2026-09-07T08:00:00\.000Z；本次扫描失败，结果未更新。/),
    ).toBeInTheDocument();
  });

  it("keeps keyboard operation working on the summary toggle", async () => {
    const user = userEvent.setup();
    const onOpenChange = vi.fn();
    render(
      <DiscoveryWarnings
        diagnostics={[autoDiagnostic]}
        scanId="scan-1"
        scannedAt={null}
        stale={false}
        busy={false}
        repairPreparing={false}
        observationNames={new Map()}
        bindingInfo={new Map()}
        focusDiagnosticId={null}
        open={false}
        onOpenChange={onOpenChange}
        onRepair={vi.fn()}
      />,
    );

    const toggle = screen.getByRole("button", { name: /展开警告详情/ });
    toggle.focus();
    await user.keyboard("{Enter}");
    expect(onOpenChange).toHaveBeenCalledWith(true);
  });
});
