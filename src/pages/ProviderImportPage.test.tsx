import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
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
  importProposals: [{ app: "claude", basis: "由当前 Claude 配置生成",
    draft: { app: "claude", name: "导入 Claude", routeMode: "official", model: null, baseUrl: null,
      apiKey: "", upstreamProtocol: null, responsesOptions: null, maxOutputTokens: null,
      parameters: providerParameters("claude"), modelOptions: null, websiteUrl: null } }],
};

const ccScan: CcSwitchScan = {
  dbPath: "test/cc-switch.db",
  providers: [
    { key: "codex:a", app: "codex", routeMode: "official", name: "导入 Codex", model: null, baseUrl: null,
      usageScriptImportable: false, usageScriptUpdatesExisting: false, existing: false, warnings: [] },
    { key: "claude:b", app: "claude", routeMode: "official", name: "导入 Claude", model: null, baseUrl: null,
      usageScriptImportable: false, usageScriptUpdatesExisting: false, existing: false, warnings: [] },
  ], skipped: [],
};

type Props = Parameters<typeof ProviderImportPage>[0];
function renderPage(overrides: Partial<Props> = {}) {
  const props: Props = { appFilter: "claude", discovery, ccScan: null, ccSelected: {}, ccResult: null, busy: false,
    onSelectApp() {}, onBack() {}, onScanLocal() {}, onScanCc() {}, onSelectCc() {},
    onImportLocal: async () => false, onImportCc: async () => false, ...overrides };
  const view = render(<ProviderImportPage {...props} />);
  return { ...view, rerenderPage: (next: Partial<Props>) => view.rerender(<ProviderImportPage {...props} {...next} />) };
}

describe("ProviderImportPage local source", () => {
  it("carries the selected client into the scan and leaves only after a successful import", async () => {
    const onImportLocal = vi.fn(async () => true);
    const onBack = vi.fn();
    renderPage({ onImportLocal, onBack });
    expect(screen.getByRole("radio", { name: "Claude" })).toBeChecked();
    expect(screen.getByLabelText("Claude 扫描结果")).toBeInTheDocument();
    expect(screen.queryByLabelText("Codex 扫描结果")).not.toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "导入供应商" }));
    expect(onImportLocal).toHaveBeenCalledWith("claude");
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("stays on the import view after a failed import", async () => {
    const onBack = vi.fn();
    renderPage({ onBack, onImportLocal: async () => false });
    await userEvent.click(screen.getByRole("button", { name: "导入供应商" }));
    expect(onBack).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Claude 扫描结果")).toBeInTheDocument();
  });

  it("changes the shared client explicitly and returns without resetting it", async () => {
    const onSelectApp = vi.fn();
    const onBack = vi.fn();
    const { rerenderPage } = renderPage({ onSelectApp, onBack });
    await userEvent.click(within(screen.getByRole("radiogroup", { name: "导入客户端" })).getByRole("radio", { name: "Codex" }));
    expect(onSelectApp).toHaveBeenCalledWith("codex");
    rerenderPage({ appFilter: "codex" });
    expect(screen.getByLabelText("Codex 扫描结果")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "返回供应商" }));
    expect(onBack).toHaveBeenCalledOnce();
    expect(onSelectApp).toHaveBeenCalledTimes(1);
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
    renderPage({ ccScan, ccSelected: { "codex:a": true, "claude:b": true }, onImportCc, onBack, onSelectCc });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "CC Switch" }));
    expect(screen.getByRole("checkbox", { name: "导入 Codex" })).toBeChecked();
    expect(screen.getByRole("checkbox", { name: "导入 Claude" })).toBeChecked();
    await user.click(screen.getByRole("checkbox", { name: "导入 Codex" }));
    expect(onSelectCc).toHaveBeenCalledWith("codex:a", false);
    await user.click(screen.getByRole("button", { name: "导入所选 2 项" }));
    expect(onImportCc).toHaveBeenCalledOnce();
    expect(onBack).toHaveBeenCalledOnce();
  });

  it("keeps partial import diagnostics available when the operation is not completed", async () => {
    const onBack = vi.fn();
    renderPage({ onBack, ccResult: { importedCount: 1, usageScriptImportedCount: 0, skippedExisting: [],
      notImported: [{ key: "claude:b", appType: "claude", name: "导入 Claude", reason: "源档案已改变" }] } });
    await userEvent.click(screen.getByRole("button", { name: "CC Switch" }));
    expect(screen.getByRole("status", { name: "导入结果" })).toHaveTextContent("已导入 1 项 · 未导入 1 项");
    expect(screen.getByText("源档案已改变")).toBeInTheDocument();
    expect(onBack).not.toHaveBeenCalled();
  });
});
