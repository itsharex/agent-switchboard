import { useId } from "react";
import type { AppKind, ProviderDraft, ProviderProfile } from "../api/client";
import { Button } from "./Button";
import { OfficialLoginPanel } from "./OfficialLoginPanel";
import { ClaudeModelMapping } from "./provider-editor/ClaudeModelMapping";
import { MainModelField } from "./provider-editor/MainModelField";
import { ProviderAdvancedSettings } from "./provider-editor/ProviderAdvancedSettings";
import { ProviderConnectionFields } from "./provider-editor/ProviderConnectionFields";
import { ProviderConnectionTest } from "./provider-editor/ProviderConnectionTest";
import { ProviderEditorFrame } from "./provider-editor/ProviderEditorFrame";
import {
  ProviderAccessMode,
  ProviderIdentityFields,
  ProviderNotesField,
} from "./provider-editor/ProviderIdentityFields";
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
  /** Replaces the editor session when the user picks the other client. */
  onSwitchClient: (app: AppKind) => void;
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
    <section className="asb-editor-section" aria-label="模型">
      <h3 className="asb-section-title">模型</h3>
      <div className="asb-editor-section-fields">
        <MainModelField draft={draft} busy={busy} baseUrl={connection.baseUrl}
          claudeSettings={claudeSettings}
          models={connection.models} modelsBusy={connection.modelsBusy} modelsError={connection.modelsError}
          modelsEndpointError={connection.modelsEndpointError}
          userConfigModel={userConfigModel} userConfigWarnings={userConfigWarnings}
          fetchModels={connection.fetchModels} setDraft={setDraft} />
        <ClaudeModelMapping key={profile?.id ?? draft.app} busy={busy}
          models={connection.models} claudeSettings={claudeSettings} setDraft={setDraft} />
      </div>
    </section>
  );
}

function AdvancedSettings({ editor, ...props }: Props & { editor: ProviderEditorState }) {
  const { draft, setDraft, parameters, setParametersOpen, triggerRef } = editor;
  return <ProviderAdvancedSettings>
    {draft.upstreamProtocol === "responses" && <ResponsesOptionsFields busy={props.busy}
      options={draft.responsesOptions}
      onChange={(next) => setDraft((current) => ({ ...current, responsesOptions: next }))} />}
    <div className="asb-provider-advanced-action">
      <div><strong>运行参数</strong><span>随此供应商保存，不直接写入客户端配置。</span></div>
      <Button ref={triggerRef} variant="secondary" disabled={props.busy || !parameters.ready}
        onClick={() => setParametersOpen(true)}>配置运行参数 <span aria-hidden="true">→</span></Button>
    </div>
    <ParametersLoadStatus busy={props.busy} ready={parameters.ready}
      error={parameters.error} retry={parameters.retry} />
    <ProviderNotesField key={`notes-${draft.app}`} busy={props.busy} value={draft.notes ?? null}
      onChange={(value) => setDraft((current) => ({ ...current, notes: value }))} />
  </ProviderAdvancedSettings>;
}

function ProviderForm({ editor, formId, ...props }: Props & { editor: ProviderEditorState; formId: string }) {
  const { draft } = editor;
  const { busy, profile, onSave } = props;
  const official = draft.routeMode === "official";
  return (
    <form id={formId} className="asb-provider-form" aria-label={profile ? "编辑供应商" : "新建供应商"}
      onSubmit={(event) => { event.preventDefault(); editor.save(onSave); }}>
      {!profile && <ProviderAccessMode editor={editor} busy={busy} officialTakenApps={props.officialTakenApps}
        onOpenOfficial={props.onOpenOfficial} />}
      <ProviderIdentityFields editor={editor} busy={busy} editing={Boolean(profile)}
        onSwitchClient={props.onSwitchClient} />
      {!official && <>
        <ProviderConnectionFields key={`connection-${draft.app}`} editor={editor} busy={busy} />
        <ProviderConnectionTest app={draft.app} busy={busy} active={props.active && !editor.parametersOpen}
          baseUrl={draft.baseUrl} apiKey={draft.apiKey} upstreamProtocol={draft.upstreamProtocol}
          connection={draft.connection} authentication={draft.authentication}
          responsesOptions={draft.responsesOptions} defaultModel={draft.model} />
        <ModelSection editor={editor} busy={busy} profile={profile}
          userConfigModel={props.userConfigModel} userConfigWarnings={props.userConfigWarnings} />
      </>}
      {official && <section className="asb-editor-section" aria-label="官方登录">
        <h3 className="asb-section-title">官方登录</h3>
        <div className="asb-editor-section-fields"><OfficialLoginPanel app={draft.app} onFinished={editor.setLoginDone} /></div>
      </section>}
      <AdvancedSettings {...props} editor={editor} />
    </form>
  );
}

function ProviderEditorSession(props: Props) {
  const editor = useProviderEditor(props.profile, props.initialApp, props.busy);
  const formId = useId();
  const parametersOpen = editor.parametersOpen;
  const title = parametersOpen ? "运行参数" : props.profile ? "编辑供应商" : "新建供应商";
  const backLabel = parametersOpen ? "返回编辑" : "返回供应商";
  const goBack = () => parametersOpen ? editor.setParametersOpen(false) : props.onCancel();
  return <ProviderEditorFrame title={title} titleRef={editor.headingRef} backLabel={backLabel}
    busy={props.busy} onBack={goBack} onCancel={props.onCancel} formId={formId} canSave={editor.canSave}>
    <div hidden={parametersOpen}><ProviderForm {...props} editor={editor} formId={formId} /></div>
    {parametersOpen && <ProviderParametersPage value={editor.draft.parameters} parameters={editor.parameters}
      onChange={(parameters) => editor.setDraft((current) => ({ ...current, parameters }))} busy={props.busy}
      baselineValues={props.profile?.parameters.settings} />}
  </ProviderEditorFrame>;
}

/** Each profile owns one draft across both editor levels and the save transaction. */
export function ProviderEditor(props: Props) {
  return <ProviderEditorSession key={props.profile?.id ?? props.initialApp} {...props} />;
}
