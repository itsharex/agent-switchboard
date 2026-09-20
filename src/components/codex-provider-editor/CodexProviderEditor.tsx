import { useId, useState } from "react";
import type { AppKind, CodexProviderDraft, ProviderDraft, ProviderRecord } from "../../api/client";
import type { CodexEditorSource } from "../../app/useProviders";
import { Button } from "../Button";
import { ProviderConnectionTest } from "../provider-editor/ProviderConnectionTest";
import { ProviderEditorFrame } from "../provider-editor/ProviderEditorFrame";
import { ProviderAdvancedSettings } from "../provider-editor/ProviderAdvancedSettings";
import { ProviderNotesField } from "../provider-editor/ProviderIdentityFields";
import { ParametersLoadStatus } from "../provider-editor/ProviderParametersPage";
import { ResponsesOptionsFields } from "../provider-editor/ResponsesOptionsFields";
import { CodexCapabilitiesSection } from "./CodexCapabilitiesSection";
import { CodexConnectionFields } from "./CodexConnectionFields";
import { CodexAccessMode, CodexIdentityFields } from "./CodexIdentityFields";
import { CodexModelSection } from "./CodexModelSection";
import { CodexOfficialProviderForm } from "./CodexOfficialProviderForm";
import { CodexParametersPage } from "./CodexParametersPage";
import { useCodexProviderEditor, type CodexEditorState } from "./useCodexProviderEditor";
import "../../styles/base/provider-editor.css";

interface Props {
  active: boolean;
  source: CodexEditorSource | null;
  busy: boolean;
  userConfigModel: string | null;
  userConfigWarnings: string[];
  onSave: (draft: CodexProviderDraft) => void;
  /** The official-login arm saves a generic draft: an official record is not
   * a third-party profile and never migrates into one. */
  onSaveOfficial: (draft: ProviderDraft) => void;
  /** Replaces the session when the user picks another access mode. */
  onSwitchAccessMode: (official: boolean, existing: ProviderRecord | null) => void;
  onCancel: () => void;
  /** Switching clients replaces the editor session; a Codex draft never
   * mutates into a Claude draft. */
  onSwitchClient: (app: AppKind) => void;
}

type OfficialProps = Omit<Props, "source"> & {
  source: Extract<CodexEditorSource, { kind: "official" }>;
};

function CodexProviderForm({ editor, formId, ...props }: Props & { editor: CodexEditorState; formId: string }) {
  const { draft, setDraft, parameters, setParametersOpen, triggerRef } = editor;
  const { busy, onSave } = props;
  const editing = props.source?.kind === "record";
  return (
    <form id={formId} className="asb-provider-form" aria-label={editing ? "编辑 Codex 供应商" : "新建 Codex 供应商"}
      onSubmit={(event) => { event.preventDefault(); editor.save(onSave); }}>
      {!editing && <CodexAccessMode busy={busy}
        onSwitchAccessMode={(official) => props.onSwitchAccessMode(official, null)} />}
      <CodexIdentityFields draft={draft} busy={busy} editing={editing}
        setDraft={setDraft} onSwitchClient={props.onSwitchClient} />
      <CodexConnectionFields key={`connection-${draft.upstream}`} editor={editor} busy={busy} />
      <ProviderConnectionTest app="codex" busy={busy} active={props.active && !editor.parametersOpen}
        baseUrl={draft.endpoint} apiKey={draft.apiKey} upstreamProtocol={draft.upstream}
        connection={draft.connection} authentication={draft.authentication}
        responsesOptions={draft.upstream === "responses" ? { requestMode: draft.requestMode } : null}
        defaultModel={draft.defaultModel} />
      <CodexModelSection editor={editor} busy={busy}
        userConfigModel={props.userConfigModel} userConfigWarnings={props.userConfigWarnings} />
      <ProviderAdvancedSettings>
        {draft.upstream === "responses" && <ResponsesOptionsFields busy={busy}
          options={{ requestMode: draft.requestMode }}
          onChange={(next) => setDraft((current) => ({ ...current, requestMode: next.requestMode }))} />}
        <CodexCapabilitiesSection editor={editor} busy={busy} />
        <div className="asb-provider-advanced-action">
          <div><strong>运行参数</strong><span>随此供应商保存，不直接写入客户端配置。</span></div>
          <Button ref={triggerRef} variant="secondary" disabled={busy || !parameters.ready}
            onClick={() => setParametersOpen(true)}>配置运行参数 <span aria-hidden="true">→</span></Button>
        </div>
        <ParametersLoadStatus busy={busy} ready={parameters.ready}
          error={parameters.error} retry={parameters.retry} />
        <ProviderNotesField busy={busy} value={draft.notes}
          onChange={(value) => setDraft((current) => ({ ...current, notes: value }))} />
      </ProviderAdvancedSettings>
      {editor.problems.length > 0 && <div className="asb-field-error" role="alert">
        {editor.problems.map((problem) => <p key={problem}>{problem}</p>)}
      </div>}
    </form>
  );
}

function CodexProviderEditorSession(props: Props) {
  const editor = useCodexProviderEditor(props.source, props.busy);
  const formId = useId();
  const parametersOpen = editor.parametersOpen;
  const editing = props.source?.kind === "record";
  const title = parametersOpen ? "运行参数" : editing ? "编辑 Codex 供应商" : "新建 Codex 供应商";
  const backLabel = parametersOpen ? "返回编辑" : "返回供应商";
  const goBack = () => parametersOpen ? editor.setParametersOpen(false) : props.onCancel();
  return <ProviderEditorFrame title={title} titleRef={editor.headingRef} backLabel={backLabel}
    busy={props.busy} onBack={goBack} onCancel={props.onCancel} formId={formId} canSave={editor.canSave}>
    <div hidden={parametersOpen}><CodexProviderForm {...props} editor={editor} formId={formId} /></div>
    {parametersOpen && <CodexParametersPage editor={editor} busy={props.busy}
      baselineValues={props.source?.kind === "record" ? props.source.record.parameters.settings : undefined}
      baselineRoute={props.source?.kind === "record" ? props.source.record.profile.subagentRoute ?? null : null}
      selfId={props.source?.kind === "record" ? props.source.record.profile.id : undefined} />}
  </ProviderEditorFrame>;
}

function CodexOfficialSession(props: OfficialProps) {
  const editor = useCodexProviderEditor(null, props.busy);
  const formId = useId();
  const record = props.source.record;
  const [name, setName] = useState(record?.profile.name ?? "Codex 官方登录");
  const [websiteUrl, setWebsiteUrl] = useState(record?.profile.websiteUrl ?? "");
  const [notes, setNotes] = useState(record?.profile.notes ?? "");
  const [quotaMinutes, setQuotaMinutes] = useState(record?.profile.officialQuotaRefreshIntervalMinutes ?? 0);
  const parameters = editor.draft.parameters;
  const title = record ? "编辑 Codex 官方登录" : "新建 Codex 官方登录";
  if (!editor.parameters.ready || parameters === null) {
    return <ProviderEditorFrame title={title} backLabel="返回供应商" busy={props.busy}
      onBack={props.onCancel} onCancel={props.onCancel} canSave={false}>
      <ParametersLoadStatus busy={props.busy} ready={false} error={editor.parameters.error}
        retry={editor.parameters.retry} />
    </ProviderEditorFrame>;
  }
  const canSave = Boolean(name.trim());
  const save = () => {
    if (props.busy || !canSave) return;
    props.onSaveOfficial({
      app: "codex", routeMode: "official", name: name.trim(), baseUrl: null, apiKey: "", upstreamProtocol: null,
      responsesOptions: null, maxOutputTokens: null, model: null, modelOptions: null, parameters,
      notes: notes.trim() || null, websiteUrl: websiteUrl.trim() || null,
      officialQuotaRefreshIntervalMinutes: quotaMinutes > 0 ? quotaMinutes : null,
    });
  };
  return <ProviderEditorFrame title={title} backLabel="返回供应商" busy={props.busy}
    onBack={props.onCancel} onCancel={props.onCancel} formId={formId} canSave={canSave}>
    <CodexOfficialProviderForm formId={formId} busy={props.busy} editing={Boolean(record)}
      name={name} websiteUrl={websiteUrl} notes={notes} quotaMinutes={quotaMinutes}
      onNameChange={setName} onWebsiteChange={setWebsiteUrl} onNotesChange={setNotes}
      onQuotaMinutesChange={setQuotaMinutes} onSubmit={save}
      onSwitchAccessMode={() => props.onSwitchAccessMode(false, record)} />
  </ProviderEditorFrame>;
}

/** One Codex editor owns one session per access mode: a third-party draft and
 * an official-login record never convert into each other. */
export function CodexProviderEditor(props: Props) {
  if (props.source?.kind === "official") {
    return <CodexOfficialSession key={props.source.record?.profile.id ?? "new-codex-official"}
      {...props} source={props.source} />;
  }
  const source = props.source;
  const key = source?.kind === "record" ? source.record.profile.id : "new-codex";
  return <CodexProviderEditorSession key={key} {...props} />;
}
