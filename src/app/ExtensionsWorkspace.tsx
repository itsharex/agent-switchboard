import { ClientPicker } from "../components/ClientPicker";
import { GlobalPromptManager } from "../components/GlobalPromptManager";
import { ExtensionsPage } from "../pages/ExtensionsPage";
import type { SwitchboardModel } from "./useSwitchboardModel";

type ExtensionsWorkspaceModel = Pick<SwitchboardModel,
  "appFilter" | "extensionSection" | "setExtensionSection" | "promptDocuments" |
  "busy" | "setBusy" | "clearError" | "reportError"
> & { providers: Pick<SwitchboardModel["providers"], "selectApp"> };

function GlobalInstructions({ model }: { model: ExtensionsWorkspaceModel }) {
  const { appFilter: app, promptDocuments: prompts, busy } = model;
  return (
    <div className="asb-ext-instructions">
      <div className="asb-ext-instructions-heading">
        <ClientPicker app={app} onChange={model.providers.selectApp} disabled={busy} label="全局指令客户端" />
      </div>
      <GlobalPromptManager document={prompts.documents[app]}
        draft={prompts.drafts[app] ?? prompts.documents[app]?.content ?? ""}
        dirty={prompts.isPromptDirty(app)} busy={busy}
        onChange={(content) => prompts.setPromptDraft(app, content)}
        onSave={() => void prompts.savePromptDocument(app)}
        onDiscard={() => prompts.discardPromptDraft(app)}
        onReload={() => prompts.reloadPromptDocument(app)} />
    </div>
  );
}

export function ExtensionsWorkspace({ model }: { model: ExtensionsWorkspaceModel }) {
  return (
    <ExtensionsPage section={model.extensionSection} onSectionChange={model.setExtensionSection}
      busy={model.busy} setBusy={model.setBusy} clearError={model.clearError} onError={model.reportError}
      instructions={<GlobalInstructions model={model} />} />
  );
}
