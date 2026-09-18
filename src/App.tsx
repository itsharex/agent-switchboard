import { AppErrorBoundary } from "./components/AppErrorBoundary";
import { Toaster } from "./components/Toaster";
import { AppShell } from "./app/AppShell";
import { AppWorkspace } from "./app/AppWorkspace";
import { useDevtoolsShortcut, useKeyboardFocusMarker } from "./app/global-effects";
import { OperationConfirmSheets } from "./app/OperationConfirmSheets";
import { useSwitchboardModel } from "./app/useSwitchboardModel";

/** The shell composes the domain model, workspaces and operation confirmations. */
export default function App() {
  const model = useSwitchboardModel();
  const { page, setPage, error, busy, providers, operations, appSettingsState, updateCheck } = model;
  useDevtoolsShortcut();
  useKeyboardFocusMarker();
  return (
    <>
      <AppShell page={page} onPageChange={setPage} error={error} busy={busy}
        onResetStore={() => providers.setResetStorePending(true)}
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
