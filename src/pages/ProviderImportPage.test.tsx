import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { CcSwitchScan, DiscoveryReport } from "../api/client";
import { statuses } from "../test/app-fixtures";
import { providerParameters } from "../test/provider-parameters";
import { ProviderImportPage } from "./ProviderImportPage";

const discovery: DiscoveryReport = {
  codex: { app: "codex", path: "test/codex.toml", exists: true,
    state: { kind: "ok", route: statuses[0].route!, managed: true, importable: false, warnings: [] } },
  claude: { app: "claude", path: "test/claude.json", exists: true,
    state: { kind: "ok", route: statuses[1].route!, managed: false, importable: true, warnings: [] } },
  claudeImportProposals: [{ basis: "由当前 Claude 配置生成",
    draft: { app: "claude", name: "导入 Claude", routeMode: "official", model: null, baseUrl: null,
      apiKey: "", upstreamProtocol: null, responsesOptions: null, maxOutputTokens: null,
      parameters: providerParameters("claude"), modelOptions: null, websiteUrl: null } }],
};

const ccScan: CcSwitchScan = {
  dbPath: "test/cc-switch.db",
  providers: [
    { key: "claude:b", app: "claude", routeMode: "official", name: "导入 Claude", model: null, baseUrl: null,
      usageScriptImportable: false, usageScriptUpdatesExisting: false, existing: false, warnings: [] },
    { key: "codex:a", app: "codex", routeMode: "custom", name: "导入 Codex", model: "gpt-5",
      baseUrl: "https://provider.example/v1", usageScriptImportable: false, usageScriptUpdatesExisting: false,
      existing: false, warnings: [] },
  ], skipped: [],
};

type Props = Parameters<typeof ProviderImportPage>[0];
function renderPage(overrides: Partial<Props> = {}) {
  const props: Props = { appFilter: "claude", discovery, ccScan: null, ccSelected: {}, ccResult: null, busy: false,
    onBack() {}, onScanLocal() {}, onScanCc() {}, onSelectCc() {},
    onImportLocal: async () => false, onImportCc: async () => false, ...overrides };
  const view = render(<ProviderImportPage {...props} />);
  return { ...view, rerenderPage: (next: Partial<Props>) => view.rerender(<ProviderImportPage {...props} {...next} />) };
}

describe("ProviderImportPage local source", () => {
  it("imports the Claude local configuration and leaves only after success", async () => {
    const onImportLocal = vi.fn(async () => true);
    const onBack = vi.fn();
    renderPage({ onImportLocal, onBack });
    expect(screen.getByRole("radio", { name: "本机配置" })).toBeChecked();
    expect(screen.getByLabelText("Claude 扫描结果")).toBeInTheDocument();
    expect(screen.queryByLabelText("Codex 扫描结果")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "导入供应商" }));
    expect(onImportLocal).toHaveBeenCalledWith();
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("stays on the import view after a failed import", async () => {
    const onBack = vi.fn();
    renderPage({ onBack, onImportLocal: async () => false });
    await userEvent.click(screen.getByRole("button", { name: "导入供应商" }));
    expect(onBack).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Claude 扫描结果")).toBeInTheDocument();
  });

  it("keeps scanning an explicit action when no cached result exists", async () => {
    const onScanLocal = vi.fn();
    renderPage({ discovery: null, onScanLocal });
    expect(screen.getByText("尚未扫描 Claude 配置。")).toBeInTheDocument();
    expect(onScanLocal).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: "扫描配置" }));
    expect(onScanLocal).toHaveBeenCalledOnce();
  });
});

describe("ProviderImportPage CC Switch source", () => {
  it("preserves cross-client selection and returns after the import owner reports success", async () => {
    const onImportCc = vi.fn(async () => true);
    const onBack = vi.fn();
    const onSelectCc = vi.fn();
    renderPage({ ccScan, ccSelected: { "claude:b": true }, onImportCc, onBack, onSelectCc });
    const user = userEvent.setup();
    await user.click(screen.getByRole("radio", { name: "CC Switch" }));
    expect(screen.getByRole("checkbox", { name: "导入 Claude" })).toBeChecked();
    // Every importable row renders a checkbox; selection stays per row.
    expect(screen.getByRole("checkbox", { name: "导入 Codex" })).not.toBeChecked();
    await user.click(screen.getByRole("button", { name: "导入所选 1 项" }));
    expect(onImportCc).toHaveBeenCalledOnce();
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("offers only CC Switch import from the Codex workspace and selects Codex rows for one-click import", async () => {
    const onImportCc = vi.fn(async () => true);
    const user = userEvent.setup();
    renderPage({ appFilter: "codex", ccScan, ccSelected: { "codex:a": true }, onImportCc });
    expect(screen.queryByRole("radiogroup", { name: "导入来源" })).not.toBeInTheDocument();
    expect(screen.queryByRole("radio", { name: "本机配置" })).not.toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "导入 Codex" })).toBeChecked();
    expect(screen.getByText("Codex · gpt-5 · https://provider.example/v1")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "导入所选 1 项" }));
    expect(onImportCc).toHaveBeenCalledOnce();
  });

  it("keeps partial import diagnostics available when the operation is not completed", async () => {
    const onBack = vi.fn();
    renderPage({ onBack, ccResult: { importedCount: 1, usageScriptImportedCount: 0, skippedExisting: [],
      notImported: [{ key: "claude:b", appType: "claude", name: "导入 Claude", reason: "源档案已改变" }] } });
    await userEvent.click(screen.getByRole("radio", { name: "CC Switch" }));
    expect(screen.getByRole("status", { name: "导入结果" })).toHaveTextContent("已导入 1 项 · 未导入 1 项");
    expect(screen.getByText("源档案已改变")).toBeInTheDocument();
    expect(onBack).not.toHaveBeenCalled();
  });
});
