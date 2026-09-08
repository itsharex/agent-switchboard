import { ProvidersPage } from "../pages/ProvidersPage";
import { ProviderImportPage } from "../pages/ProviderImportPage";
import type { SwitchboardModel } from "./useSwitchboardModel";

export function ProvidersWorkspace({ model, active }: { model: SwitchboardModel; active: boolean }) {
  const importing = model.providerView.kind === "import";
  const { snapshot, appFilter, activeProfileId, providers, switchPreview, operations, appSettingsState, busy } = model;
  const { discoveryState, ccImport } = model;
  const editorApp = providers.editorSession?.app ?? appFilter;
  const userConfigRoute = snapshot.statuses?.find((status) => status.app === editorApp)?.route ?? null;
  return (
    <>
      {importing && <div hidden={!active}>
        <ProviderImportPage appFilter={appFilter} discovery={discoveryState.discovery} busy={busy}
          ccScan={ccImport.ccScan} ccSelected={ccImport.ccSelected} ccResult={ccImport.ccResult}
          onSelectApp={providers.selectApp} onBack={() => model.setProviderView({ kind: "list" })}
          onScanLocal={() => void discoveryState.runDiscovery()} onImportLocal={discoveryState.runImport}
          onScanCc={() => void ccImport.runCcScan()} onImportCc={ccImport.runCcImport}
          onSelectCc={(key, checked) => ccImport.setCcSelected((current) => ({ ...current, [key]: checked }))} />
      </div>}
    <ProvidersPage active={active && !importing} view={model.providerView} onViewChange={model.setProviderView}
      profiles={snapshot.profiles} appFilter={appFilter}
      activeProfileId={activeProfileId(appFilter)} statuses={snapshot.statuses} locks={snapshot.locks}
      userConfigModel={userConfigRoute?.model ?? null} userConfigWarnings={userConfigRoute?.scopeWarnings ?? []}
      selectedId={snapshot.selectedId} editorSession={providers.editorSession}
      preview={switchPreview.preview} busy={busy} collapsedUsageIds={appSettingsState.appSettings?.collapsedUsageIds ?? []}
      onSelectApp={providers.selectApp} onNew={providers.newEditor}
      onImport={() => { switchPreview.retractPreview(); model.setProviderView({ kind: "import" }); }}
      onOpenClientSettings={() => model.openSettings("client")} onOpenHistory={() => model.openSettings("backups")}
      onOpenDiagnostics={(section) => model.openSettings("diagnostics", section)} onOpenQuota={model.openQuota}
      onCloseEditor={providers.closeEditor} onSave={providers.saveProfile}
      onSaveUsageQuery={async (profile, query) => {
        const saved = await providers.saveProfileUsageQuery(profile, query);
        if (saved) {
          providers.selectApp(profile.app);
          switchPreview.selectProfile(profile.id);
        }
        return saved;
      }} onSaveQuotaInterval={providers.saveOfficialQuotaInterval}
      onSelect={switchPreview.selectProfile} onReorder={providers.dragReorderProfiles}
      onToggleUsage={(profile) => appSettingsState.toggleUsageCollapsed(profile.id)}
      onActivate={switchPreview.previewProfile} onTogglePreview={switchPreview.togglePreviewProfile}
      onEdit={providers.openEditor} onDelete={providers.setDeletePending}
      onRequestSwitch={() => operations.setConfirmingSwitch(true)} onCancelPreview={switchPreview.retractPreview} />
    </>
  );
}
