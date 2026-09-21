import { useCallback, useEffect, useRef, useState } from "react";
import { getStartupPage, rememberWorkspacePage, type CommandError } from "../api/client";
import { WORKSPACE_PAGE_LABELS, workspacePageId } from "./navigation";
import type { DiagnosticSection, ExtensionSection, Page, ProviderView, SettingsSection, UsageSection } from "./navigation";

export function useWorkspaceNavigation(settingsReady: boolean, settingsError: string | null, onError: (error: CommandError) => void) {
  const [page, changePage] = useState<Page>("供应商切换");
  const [navigationReady, setNavigationReady] = useState(false);
  const initialized = useRef(false);
  const userNavigated = useRef(false);
  const writes = useRef<Promise<void>>(Promise.resolve());
  useEffect(() => {
    if (!settingsReady || initialized.current) return;
    if (settingsError) {
      initialized.current = true;
      setNavigationReady(true);
      return;
    }
    let disposed = false;
    void getStartupPage().then((saved) => {
      if (!disposed && !userNavigated.current) changePage(WORKSPACE_PAGE_LABELS[saved]);
    }).catch((error: CommandError) => {
      if (!disposed) onError(error);
    }).finally(() => {
      if (!disposed) { initialized.current = true; setNavigationReady(true); }
    });
    return () => { disposed = true; };
  }, [settingsReady, settingsError, onError]);
  useEffect(() => {
    if (!navigationReady || !userNavigated.current) return;
    // Serialize explicit visits so a slower earlier save cannot replace a newer page.
    writes.current = writes.current.then(() => rememberWorkspacePage(workspacePageId(page))).catch(onError);
  }, [navigationReady, page, onError]);
  const [providerView, setProviderView] = useState<ProviderView>({ kind: "list" });
  const [settingsSection, setSettingsSection] = useState<SettingsSection>("application");
  const [diagnosticSection, setDiagnosticSection] = useState<DiagnosticSection>("configuration");
  const [extensionSection, setExtensionSection] = useState<ExtensionSection>("skill");
  const [usageSection, setUsageSection] = useState<UsageSection>("consumption");
  const [settingsReturnToProviders, setSettingsReturnToProviders] = useState(false);
  const setPage = useCallback((next: Page) => {
    userNavigated.current = true;
    setSettingsReturnToProviders(false);
    changePage(next);
  }, []);
  const openSettings = useCallback((section: SettingsSection, diagnostic?: DiagnosticSection) => {
    userNavigated.current = true;
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
  return { page, setPage, navigationReady, providerView, setProviderView, settingsSection, setSettingsSection, diagnosticSection, setDiagnosticSection,
    extensionSection, setExtensionSection, usageSection, setUsageSection, settingsReturnToProviders,
    openSettings, returnToProviders, openQuota };
}
