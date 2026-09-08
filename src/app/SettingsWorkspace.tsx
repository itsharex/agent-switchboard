import { ClientSettingsPanel } from "../components/ClientSettingsPanel";
import { CodexSubagentSettingsPanel } from "../components/CodexSubagentSettingsPanel";
import { BackupsPage } from "../pages/BackupsPage";
import { DiagnosticsPage } from "../pages/DiagnosticsPage";
import { SettingsPage } from "../pages/SettingsPage";
import type { SwitchboardModel } from "./useSwitchboardModel";

function ClientPreferences({ model }: { model: SwitchboardModel }) {
  const { clientSettings: settings, codexSubagentSettings, snapshot, busy, appFilter: app } = model;
  const hasActiveProvider = snapshot.profiles.some((profile) =>
    profile.app === app && profile.id === snapshot.activeProfileId(app));
  return (
    <ClientSettingsPanel key={app} app={app} onSelectApp={model.providers.selectApp} editorState={settings.editorState}
      busy={busy} configStatus={snapshot.statuses?.find((status) => status.app === app)}
      hasActiveProvider={hasActiveProvider} onValueChange={settings.changeValue}
      previewBlockedReason={model.providers.editorSession ? "请先保存或取消供应商编辑，再预览应用。"
        : model.providerView.kind === "usage" ? "请先保存或取消用量查询编辑，再预览应用。" : null}
      onResetGroup={settings.resetGroupToDefaults} onSave={(target) => void settings.saveSettings(target)}
      onSaveAndPreview={(target) => void model.saveClientSettingsAndPreview(target)}
      onOpenProviders={model.returnToProviders} onRetryLoad={settings.retryLoad} onPreview={settings.previewSettings}
      subagentSettings={app === "codex" ? (
        <CodexSubagentSettingsPanel
          editorState={codexSubagentSettings.editorState}
          busy={busy}
          onChange={codexSubagentSettings.changeValue}
          onReset={codexSubagentSettings.resetToAutomatic}
          onRetryLoad={codexSubagentSettings.retryLoad}
          onPreview={codexSubagentSettings.preview}
          onApplyPreview={codexSubagentSettings.applyPreview}
        />
      ) : undefined}
    />
  );
}

export function SettingsWorkspace({ model, active }: { model: SwitchboardModel; active: boolean }) {
  const { appSettingsState: settings, updateCheck: update, snapshot, operations, busy } = model;
  return (
    <SettingsPage section={model.settingsSection} onSectionChange={model.setSettingsSection}
      onReturnToProviders={model.settingsReturnToProviders ? model.returnToProviders : undefined}
      clientSettings={<ClientPreferences model={model} />}
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
