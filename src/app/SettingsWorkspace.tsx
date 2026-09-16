import { BackupsPage } from "../pages/BackupsPage";
import { DiagnosticsPage } from "../pages/DiagnosticsPage";
import { SettingsPage } from "../pages/SettingsPage";
import type { SwitchboardModel } from "./useSwitchboardModel";

export function SettingsWorkspace({ model, active }: { model: SwitchboardModel; active: boolean }) {
  const { appSettingsState: settings, updateCheck: update, snapshot, operations, busy } = model;
  return (
    <SettingsPage section={model.settingsSection} onSectionChange={model.setSettingsSection}
      onReturnToProviders={model.settingsReturnToProviders ? model.returnToProviders : undefined}
      backups={<BackupsPage records={snapshot.backups} busy={busy} lastSwitch={model.lastSwitchOverall}
        cloudBackup={model.cloudBackup} onRestore={operations.runRestore} onUndo={operations.requestUndo}
        onOpenDir={model.openBackupFolder} />}
      diagnostics={<DiagnosticsPage active={active && model.settingsSection === "diagnostics"}
        section={model.diagnosticSection} onSectionChange={model.setDiagnosticSection}
        statuses={snapshot.statuses} profiles={snapshot.profiles} locks={snapshot.locks} busy={busy}
        onRefresh={() => void snapshot.refresh()} onRecoverLock={operations.setRecoverLockPending}
        logLevel={settings.appSettings?.runtimeLogLevel ?? null}
        onLogLevelChange={(runtimeLogLevel) => settings.saveSettingsPatch({ runtimeLogLevel })} />}
      settings={settings.appSettings} loadError={settings.loadError} onRetryLoad={settings.retryLoad}
      onRepair={() => void settings.repairSettings()} busy={busy} onPatch={settings.saveSettingsPatch}
      onRestart={() => void settings.restart()} updateCheck={update.updateCheck} updateChannel={update.updateChannel}
      appVersion={update.appVersion} updateChecking={update.checking} updateInstalling={update.installing}
      updateProgress={update.downloadProgress} updateCheckedAt={update.lastCheckedAt}
      updateRestartRequired={update.restartRequired} onCheckUpdate={() => void update.runUpdateCheck()}
      onInstallUpdate={() => void update.installAvailableUpdate()} onRestartInstalledUpdate={() => void update.restartInstalledUpdate()} />
  );
}
