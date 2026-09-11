import type { AppKind, CodexProviderDraft, ProviderDraft, ProviderRecord } from "../../api/client";
import type { CodexEditorSource } from "../../app/useProviders";
import { Button } from "../Button";
import { ProviderConnectionTest } from "../provider-editor/ProviderConnectionTest";
import { ProviderNotesField } from "../provider-editor/ProviderIdentityFields";
import { ParametersLoadStatus } from "../provider-editor/ProviderParametersPage";
import { RadioOption } from "../RadioOption";
import { ResponsesOptionsFields } from "../provider-editor/ResponsesOptionsFields";
import { CodexCapabilitiesSection } from "./CodexCapabilitiesSection";
import { CodexConnectionFields } from "./CodexConnectionFields";
import { CodexIdentityFields } from "./CodexIdentityFields";
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

function CodexProviderForm({ editor, ...props }: Props & { editor: CodexEditorState }) {
  const { draft, setDraft } = editor;
  const { busy, onCancel, onSave } = props;
  const editing = props.source?.kind === "record";
  const seed = props.source?.kind === "seed" ? props.source.seed : null;
  return (
    <form className="asb-provider-form" aria-label={editing ? "编辑 Codex 供应商" : "新建 Codex 供应商"}
      onSubmit={(event) => { event.preventDefault(); editor.save(onSave); }}>
      {seed && seed.warnings.length > 0 && (
        <div className="asb-banner asb-banner-warning" role="status" aria-label="来自 CC Switch 的未导入字段">
          <span>来自 CC Switch 的未导入字段：</span>
          {seed.warnings.map((warning) => <div key={warning}>{warning}</div>)}
        </div>
      )}
      <CodexIdentityFields draft={draft} busy={busy} editing={editing}
        setDraft={setDraft} onSwitchClient={props.onSwitchClient}
        onSwitchAccessMode={(official) => props.onSwitchAccessMode(official, null)} />
      <CodexConnectionFields key={`connection-${draft.upstream}`} editor={editor} busy={busy} />
      <CodexModelSection editor={editor} busy={busy}
        userConfigModel={props.userConfigModel} userConfigWarnings={props.userConfigWarnings} />
      {draft.upstream === "responses" && (
        <ResponsesOptionsFields busy={busy} options={{ requestMode: draft.requestMode }}
          onChange={(next) => setDraft((current) => ({ ...current, requestMode: next.requestMode }))} />
      )}
      <CodexCapabilitiesSection editor={editor} busy={busy} />
      <ProviderConnectionTest busy={busy} active={props.active && !editor.parametersOpen}
        baseUrl={draft.endpoint} apiKey={draft.apiKey} upstreamProtocol={draft.upstream}
        responsesOptions={draft.upstream === "responses" ? { requestMode: draft.requestMode } : null}
        defaultModel={draft.defaultModel} />
      <ProviderNotesField busy={busy} value={draft.notes}
        onChange={(value) => setDraft((current) => ({ ...current, notes: value }))} />
      <ParametersLoadStatus busy={busy} ready={editor.parameters.ready}
        error={editor.parameters.error} retry={editor.parameters.retry} />
      {editor.problems.length > 0 && (
        <div className="asb-field-error" role="alert">
          {editor.problems.map((problem) => <p key={problem}>{problem}</p>)}
        </div>
      )}
      <footer className="asb-provider-form-footer">
        <Button variant="secondary" disabled={busy} onClick={onCancel}>取消</Button>
        <Button type="submit" variant="primary" disabled={!editor.canSave}>保存供应商</Button>
      </footer>
    </form>
  );
}

function CodexProviderEditorSession(props: Props) {
  const editor = useCodexProviderEditor(props.source, props.busy);
  const editing = props.source?.kind === "record";
  const title = editor.parametersOpen
    ? "运行参数"
    : editing ? "编辑 Codex 供应商" : "新建 Codex 供应商";
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
        <div hidden={editor.parametersOpen}><CodexProviderForm {...props} editor={editor} /></div>
        {editor.parametersOpen && <CodexParametersPage editor={editor} busy={props.busy} />}
      </section>
    </div>
  );
}

function CodexOfficialSession(props: Props) {
  // The official arm reuses the third-party editor's parameter loader so both
  // arms save the same complete parameter catalog.
  const editor = useCodexProviderEditor(null, props.busy);
  if (props.source?.kind !== "official") return null;
  const record = props.source.record;
  const parameters = editor.draft.parameters;
  if (!editor.parameters.ready || parameters === null) {
    return (
      <div className="asb-edit-view asb-provider-editor">
        <section className="asb-panel asb-edit-panel">
          <ParametersLoadStatus busy={props.busy} ready={false} error={editor.parameters.error}
            retry={editor.parameters.retry} />
        </section>
      </div>
    );
  }
  return (
    <div className="asb-edit-view asb-provider-editor">
      <div className="asb-panel-heading">
        <div className="asb-panel-heading-main">
          <Button variant="back" disabled={props.busy} aria-label="返回供应商列表"
            onClick={props.onCancel}>←</Button>
          <h2 className="asb-panel-title">
            {record ? "编辑 Codex 官方登录" : "新建 Codex 官方登录"}
          </h2>
        </div>
      </div>
      <section className="asb-panel asb-edit-panel">
        <section className="asb-provider-section" aria-label="基本资料">
          <h3 className="asb-section-title">基本资料</h3>
          <div className="asb-provider-section-fields">
            <div className="asb-provider-field-grid">
              <div className="asb-field">
                <span>接入方式</span>
                <div className="asb-segments" role="radiogroup" aria-label="接入方式">
                  <RadioOption name="codex-access-mode" checked={false} label="第三方服务"
                    disabled={props.busy} onChange={() => props.onSwitchAccessMode(false, record)} />
                  <RadioOption name="codex-access-mode" checked label="官方登录"
                    disabled={props.busy} onChange={() => props.onSwitchAccessMode(true, record)} />
                </div>
              </div>
            </div>
          </div>
        </section>
        <CodexOfficialProviderForm busy={props.busy} record={record} parameters={parameters}
          onSave={props.onSaveOfficial} onCancel={props.onCancel} />
      </section>
    </div>
  );
}

/** One Codex editor owns one session per access mode: a third-party draft and
 * an official-login record never convert into each other. */
export function CodexProviderEditor(props: Props) {
  if (props.source?.kind === "official") {
    return <CodexOfficialSession key={props.source.record?.profile.id ?? "new-codex-official"} {...props} />;
  }
  const source = props.source;
  const key = source?.kind === "record"
    ? source.record.profile.id
    : source?.kind === "seed" ? `ccswitch-${source.seedKey}` : "new-codex";
  return <CodexProviderEditorSession key={key} {...props} />;
}
