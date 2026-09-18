import { ClientSettingsPanel } from "../components/ClientSettingsPanel";
import { CodexSubagentSettingsPanel } from "../components/CodexSubagentSettingsPanel";
import { GlobalPromptManager } from "../components/GlobalPromptManager";
import { WorkspaceHeader } from "../components/WorkspaceHeader";
import type { SwitchboardModel } from "./useSwitchboardModel";

export function ClientConfigurationWorkspace({ model }: { model: SwitchboardModel }) {
  const {
    clientSettings: settings,
    codexSubagentSettings,
    promptDocuments: prompts,
    snapshot,
    busy,
    appFilter: app,
  } = model;
  const promptDocument = prompts.documents[app];
  return (
    <section className="asb-panel" aria-label="客户端配置">
      <WorkspaceHeader title="客户端配置" />
      <ClientSettingsPanel
        key={app}
        app={app}
        onSelectApp={model.providers.selectApp}
        editorState={settings.editorState}
        busy={busy}
        configStatus={snapshot.statuses?.find((status) => status.app === app)}
        hasUnsavedConfigurationDraft={settings.editorState.phase === "dirty" ||
          (app === "codex" && codexSubagentSettings.editorState.phase === "dirty")}
        onValueChange={settings.changeValue}
        onApplied={() => {
          settings.reloadFromStored(app);
          if (app === "codex") codexSubagentSettings.reloadFromFile();
          model.providerSwitch.invalidateCandidates();
          void snapshot.refresh();
        }} onRetryLoad={settings.retryLoad}
        onReview={settings.reviewConfiguration}
        onClaudeExtraChange={settings.changeClaudeExtra}
        globalInstructions={
          <GlobalPromptManager document={promptDocument}
            draft={prompts.drafts[app] ?? promptDocument?.content ?? ""}
            dirty={prompts.isPromptDirty(app)} busy={busy}
            onChange={(content) => prompts.setPromptDraft(app, content)}
            onSave={() => void prompts.savePromptDocument(app)}
            onDiscard={() => prompts.discardPromptDraft(app)}
            onReload={() => prompts.reloadPromptDocument(app)} />
        }
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
