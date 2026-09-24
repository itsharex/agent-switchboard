import { AppErrorBoundary } from "./components/AppErrorBoundary";
import { Toaster } from "./components/Toaster";
import { AppShell } from "./app/AppShell";
import { AppWorkspace } from "./app/AppWorkspace";
import { useDevtoolsShortcut, useKeyboardFocusMarker } from "./app/global-effects";
import { OperationConfirmSheets } from "./app/OperationConfirmSheets";
import { useSwitchboardModel } from "./app/useSwitchboardModel";

/** The shell composes the domain model, workspaces and operation confirmations.
 * The whole tree waits for the settings snapshot (or its load failure) so the
 * saved language is known before the first label renders — no wrong-language
 * flash. Accepted snapshots publish the language before the UI state. */
export default function App() {
  const model = useSwitchboardModel();
  const { page, setPage, error, busy, providers, operations, appSettingsState, updateCheck } = model;
  useDevtoolsShortcut();
  useKeyboardFocusMarker();
  const settingsReady = appSettingsState.appSettings !== null || appSettingsState.loadError !== null;
  if (!settingsReady) {
    return (
      <main className="asb-main" aria-label="Agent Switchboard">
        <div className="asb-settings-skeleton" role="status">
          <div className="asb-skeleton" /><div className="asb-skeleton" /><div className="asb-skeleton" />
        </div>
      </main>
    );
  }
  return (
    <>
      <AppShell page={page} onPageChange={setPage} error={error} busy={busy}
        onRepairStore={() => void providers.runRepairStore()}
        settingsError={appSettingsState.loadError} onRepairSettings={() => void appSettingsState.repairSettings()}
        pin={appSettingsState.pin} update={updateCheck.updateCheck
          ? { latestVersion: updateCheck.updateCheck.latestVersion, onOpen: () => model.openSettings("about") } : null}>
        <AppErrorBoundary><AppWorkspace model={model} /></AppErrorBoundary>
      </AppShell>
      <OperationConfirmSheets operations={operations} providers={providers} />
      <Toaster />
    </>
  );
}
