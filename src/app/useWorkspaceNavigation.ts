import { useCallback, useState } from "react";
import type { DiagnosticSection, ExtensionSection, Page, ProviderView, SettingsSection, UsageSection } from "./navigation";

export function useWorkspaceNavigation() {
  const [page, changePage] = useState<Page>("供应商切换");
  const [providerView, setProviderView] = useState<ProviderView>({ kind: "list" });
  const [settingsSection, setSettingsSection] = useState<SettingsSection>("application");
  const [diagnosticSection, setDiagnosticSection] = useState<DiagnosticSection>("configuration");
  const [extensionSection, setExtensionSection] = useState<ExtensionSection>("skill");
  const [usageSection, setUsageSection] = useState<UsageSection>("consumption");
  const [settingsReturnToProviders, setSettingsReturnToProviders] = useState(false);
  const setPage = useCallback((next: Page) => {
    setSettingsReturnToProviders(false);
    changePage(next);
  }, []);
  const openSettings = useCallback((section: SettingsSection, diagnostic?: DiagnosticSection) => {
    setSettingsReturnToProviders(page === "供应商切换");
    setSettingsSection(section);
    if (diagnostic) setDiagnosticSection(diagnostic);
    changePage("设置");
  }, [page]);
  const returnToProviders = useCallback(() => setPage("供应商切换"), [setPage]);
  const openQuota = useCallback(() => {
    setUsageSection("quota");
    setPage("用量监控");
  }, [setPage]);
  return { page, setPage, providerView, setProviderView, settingsSection, setSettingsSection, diagnosticSection, setDiagnosticSection,
    extensionSection, setExtensionSection, usageSection, setUsageSection, settingsReturnToProviders,
    openSettings, returnToProviders, openQuota };
}
