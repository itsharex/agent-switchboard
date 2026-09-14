import { useEffect } from "react";
import { CodexProvidersPage } from "../pages/CodexProvidersPage";
import { ProviderImportPage } from "../pages/ProviderImportPage";
import { ProvidersPage } from "../pages/ProvidersPage";
import type { SwitchboardModel } from "./useSwitchboardModel";

export function ProvidersWorkspace({ model, active }: { model: SwitchboardModel; active: boolean }) {
  const importing = model.providerView.kind === "import";
  const { snapshot, appFilter, activeProfileId, providers, switchPreview, appSettingsState, busy } = model;
  const { discoveryState, ccImport } = model;
  const editorApp = providers.editorSession?.app ?? appFilter;
  const userConfigRoute = snapshot.statuses?.find((status) => status.app === editorApp)?.route ?? null;
  const codexEditorSession = providers.editorSession?.app === "codex" ? providers.editorSession : null;
  const claudeEditorSession = providers.editorSession?.app === "claude" ? providers.editorSession : null;
  useEffect(() => {
    if (!active || importing || appFilter !== "claude" || claudeEditorSession) switchPreview.retractPreview();
  }, [active, importing, appFilter, claudeEditorSession, switchPreview.retractPreview]);
  return (
    <>
      {importing && <div hidden={!active}>
        <ProviderImportPage appFilter={appFilter} discovery={discoveryState.discovery} busy={busy}
          ccScan={ccImport.ccScan} ccSelected={ccImport.ccSelected} ccResult={ccImport.ccResult}
          onBack={() => model.setProviderView({ kind: "list" })}
          onScanLocal={() => void discoveryState.runDiscovery()} onImportLocal={discoveryState.runImport}
          onScanCc={() => void ccImport.runCcScan()} onImportCc={ccImport.runCcImport}
          onSelectCc={(key, checked) => ccImport.setCcSelected((current) => ({ ...current, [key]: checked }))} />
      </div>}
      <CodexProvidersPage
        active={active && !importing && appFilter === "codex"} records={snapshot.codexRecords}
        officialRecord={snapshot.codexOfficialRecords[0] ?? null}
        activeProfileId={activeProfileId("codex")}
        busy={busy} onBusy={model.setBusy} onError={model.reportError} onRefresh={async () => { await snapshot.refresh(); }}
        onSelectApp={providers.selectApp} requestedPreviewId={model.requestedCodexPreviewId}
        onPreviewRequestHandled={model.clearRequestedCodexPreview}
        onImport={() => { switchPreview.retractPreview(); model.setProviderView({ kind: "import" }); }}
        onOpenClientSettings={() => model.openSettings("client")} onOpenHistory={() => model.openSettings("backups")}
        onDelete={(record) => providers.setDeletePending({ kind: "codexThirdParty", record })}
        onDeleteOfficial={(record) => providers.setDeletePending({ kind: "generic", profile: record.profile })}
        editorSession={codexEditorSession} onNew={providers.newEditor} onEdit={providers.openCodexEditor}
        onEditOfficial={providers.openCodexOfficialEditor}
        onCloseEditor={providers.closeEditor}
        onSave={providers.saveCodexProfile} onSaveOfficial={providers.saveCodexOfficialProfile}
        onSwitchAccessMode={providers.switchCodexAccessMode}
        onSwitchClient={providers.newEditorFor}
        onSaveOfficialQuotaInterval={providers.saveOfficialQuotaInterval}
        loginBlocker={snapshot.loginBlocker}
        statuses={snapshot.statuses} profiles={snapshot.profiles} locks={snapshot.locks}
        userConfigModel={userConfigRoute?.model ?? null} userConfigWarnings={userConfigRoute?.scopeWarnings ?? []} />
    <ProvidersPage active={active && !importing && appFilter === "claude"} view={model.providerView} onViewChange={model.setProviderView}
      profiles={snapshot.profiles} appFilter={appFilter}
      activeProfileId={activeProfileId(appFilter)} statuses={snapshot.statuses} locks={snapshot.locks}
      userConfigModel={userConfigRoute?.model ?? null} userConfigWarnings={userConfigRoute?.scopeWarnings ?? []}
      selectedId={snapshot.selectedId} editorSession={claudeEditorSession}
      preview={switchPreview.preview} busy={busy} collapsedUsageIds={appSettingsState.appSettings?.collapsedUsageIds ?? []}
      onSelectApp={providers.selectApp} onNew={providers.newEditor}
      onImport={() => { switchPreview.retractPreview(); model.setProviderView({ kind: "import" }); }}
      onOpenClientSettings={() => model.openSettings("client")} onOpenHistory={() => model.openSettings("backups")}
      onCloseEditor={providers.closeEditor} onSave={providers.saveProfile}
      onSwitchClient={providers.newEditorFor}
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
      onActivate={switchPreview.activateProfile} onTogglePreview={switchPreview.togglePreviewProfile}
      onEdit={providers.openEditor}
      onDelete={(profile) => providers.setDeletePending({ kind: "generic", profile })} />
    </>
  );
}
