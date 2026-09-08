import type { AppKind, ProviderDraft, ProviderProfile } from "../api/client";
import { Button } from "./Button";
import { OfficialLoginPanel } from "./OfficialLoginPanel";
import { ClaudeModelMapping } from "./provider-editor/ClaudeModelMapping";
import { MainModelField } from "./provider-editor/MainModelField";
import { ProviderConnectionFields } from "./provider-editor/ProviderConnectionFields";
import { ProviderConnectionTest } from "./provider-editor/ProviderConnectionTest";
import { ProviderIdentityFields, ProviderNotesField } from "./provider-editor/ProviderIdentityFields";
import { ParametersLoadStatus, ProviderParametersPage } from "./provider-editor/ProviderParametersPage";
import { ResponsesOptionsFields } from "./provider-editor/ResponsesOptionsFields";
import { useProviderEditor, type ProviderEditorState } from "./provider-editor/useProviderEditor";
import "../styles/base/provider-editor.css";

interface Props {
  active: boolean;
  profile: ProviderProfile | null;
  initialApp: AppKind;
  busy: boolean;
  officialTakenApps: AppKind[];
  onOpenOfficial: (app: AppKind) => void;
  userConfigModel: string | null;
  userConfigWarnings: string[];
  onSave: (draft: ProviderDraft) => void;
  onCancel: () => void;
}

function ModelSection({ editor, busy, userConfigModel, userConfigWarnings, profile }:
  Pick<Props, "busy" | "userConfigModel" | "userConfigWarnings" | "profile"> & { editor: ProviderEditorState }) {
  const { draft, setDraft, connection } = editor;
  const claudeSettings = draft.modelOptions?.kind === "claude" ? draft.modelOptions : null;
  return (
    <section className="asb-provider-section" aria-label="模型">
      <h3>模型</h3>
      <div className="asb-provider-section-fields">
        <MainModelField draft={draft} busy={busy} baseUrl={connection.baseUrl}
          codex={draft.app === "codex"} claudeSettings={claudeSettings}
          models={connection.models} modelsBusy={connection.modelsBusy} modelsError={connection.modelsError}
          userConfigModel={userConfigModel} userConfigWarnings={userConfigWarnings}
          fetchModels={connection.fetchModels} setDraft={setDraft} />
        {draft.app === "claude" && <ClaudeModelMapping key={profile?.id ?? draft.app} busy={busy}
          models={connection.models} claudeSettings={claudeSettings} setDraft={setDraft} />}
      </div>
    </section>
  );
}

function ProviderForm({ editor, ...props }: Props & { editor: ProviderEditorState }) {
  const { draft } = editor;
  const { busy, profile, onCancel, onSave } = props;
  const official = draft.routeMode === "official";
  return (
    <form className="asb-provider-form" aria-label={profile ? "编辑供应商" : "新建供应商"}
      onSubmit={(event) => { event.preventDefault(); editor.save(onSave); }}>
      <ProviderIdentityFields editor={editor} busy={busy} editing={Boolean(profile)}
        officialTakenApps={props.officialTakenApps} onOpenOfficial={props.onOpenOfficial} />
      {!official && <>
        <ProviderConnectionFields key={`connection-${draft.app}`} editor={editor} busy={busy} />
        <ModelSection editor={editor} busy={busy} profile={profile}
          userConfigModel={props.userConfigModel} userConfigWarnings={props.userConfigWarnings} />
        {draft.upstreamProtocol === "responses" &&
          <ResponsesOptionsFields key={`responses-${draft.app}`} editor={editor} busy={busy} />}
        <ProviderConnectionTest draft={draft} busy={busy} active={props.active && !editor.parametersOpen} />
      </>}
      {official && (
        <section className="asb-provider-section" aria-label="官方登录">
          <h3>官方登录</h3>
          <OfficialLoginPanel app={draft.app} onFinished={editor.setLoginDone} />
        </section>
      )}
      <ProviderNotesField key={`notes-${draft.app}`} editor={editor} busy={busy} />
      <ParametersLoadStatus editor={editor} busy={busy} />
      <footer className="asb-provider-form-footer">
        <Button variant="secondary" disabled={busy} onClick={onCancel}>取消</Button>
        <Button type="submit" variant="primary" disabled={!editor.canSave}>保存供应商</Button>
      </footer>
    </form>
  );
}

function ProviderEditorSession(props: Props) {
  const editor = useProviderEditor(props.profile, props.initialApp, props.busy);
  const title = editor.parametersOpen ? "运行参数" : props.profile ? "编辑供应商" : "新建供应商";
  return (
    <div className="asb-edit-view asb-provider-editor">
      <div className="asb-panel-heading">
        <div className="asb-panel-heading-main">
          <Button variant="back" disabled={props.busy}
            aria-label={editor.parametersOpen ? "返回供应商编辑" : "返回供应商列表"}
            onClick={() => editor.parametersOpen ? editor.setParametersOpen(false) : props.onCancel()}>←</Button>
          <h2 ref={editor.headingRef} tabIndex={-1} className="asb-panel-title">{title}</h2>
        </div>
        {!editor.parametersOpen && (
          <Button ref={editor.triggerRef} variant="secondary" disabled={props.busy}
            onClick={() => editor.setParametersOpen(true)}>配置运行参数 <span aria-hidden="true">→</span></Button>
        )}
      </div>
      <section className="asb-panel asb-edit-panel">
        <div hidden={editor.parametersOpen}><ProviderForm {...props} editor={editor} /></div>
        {editor.parametersOpen && <ProviderParametersPage editor={editor} busy={props.busy} />}
      </section>
    </div>
  );
}

/** Each profile owns one draft across both editor levels and the save transaction. */
export function ProviderEditor(props: Props) {
  return <ProviderEditorSession key={props.profile?.id ?? props.initialApp} {...props} />;
}
