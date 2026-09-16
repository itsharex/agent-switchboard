import { ClientSettingsPanel } from "../components/ClientSettingsPanel";
import { CodexSubagentSettingsPanel } from "../components/CodexSubagentSettingsPanel";
import { WorkspaceHeader } from "../components/WorkspaceHeader";
import type { SwitchboardModel } from "./useSwitchboardModel";

export function ClientConfigurationWorkspace({ model }: { model: SwitchboardModel }) {
  const { clientSettings: settings, codexSubagentSettings, snapshot, busy, appFilter: app } = model;
  return (
    <section className="asb-panel" aria-label="客户端通用配置">
      <WorkspaceHeader title="客户端通用配置" />
      <ClientSettingsPanel key={app} app={app} onSelectApp={model.providers.selectApp} editorState={settings.editorState}
        busy={busy} configStatus={snapshot.statuses?.find((status) => status.app === app)}
        onValueChange={settings.changeValue}
        onApplied={() => {
          settings.reloadFromStored(app);
          if (app === "codex") codexSubagentSettings.reloadFromFile();
          model.switchPreview.invalidateSwitchCandidates();
          void snapshot.refresh();
        }} onRetryLoad={settings.retryLoad} onPreview={settings.previewSettings}
        onPreviewContentChange={settings.changePreviewContent}
        subagentDraft={app === "codex" ? codexSubagentSettings.editorState.draft : undefined}
        subagentSettings={app === "codex" ? (
          <CodexSubagentSettingsPanel
            editorState={codexSubagentSettings.editorState}
            busy={busy}
            onChange={codexSubagentSettings.changeValue}
            onRetryLoad={codexSubagentSettings.retryLoad}
          />
        ) : undefined}
      />
    </section>
  );
}
