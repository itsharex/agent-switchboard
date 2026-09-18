import { useEffect, useState, type ReactNode } from "react";

import {
  commitClientConfigurationRepair,
  commitManualClientConfiguration,
  previewClientConfigurationRepair,
  previewManualClientConfiguration,
  type AppKind,
  type ClientConfigurationApplyPreview,
  type ClientConfigurationRepairPreview,
  type CodexSubagentSettings,
  type CurrentClientConfiguration,
} from "../api/client";
import { clientSettingsPayload } from "../app/claude-common-settings";
import type { ClientSettingsEditorState } from "../app/useClientSettings";
import { Button } from "./Button";
import { CodePreview } from "./CodePreview";
import { EditableCodePreview } from "./EditableCodePreview";
import { EditIcon } from "./icons";

interface Props {
  app: AppKind;
  busy: boolean;
  editorState: ClientSettingsEditorState;
  source: CurrentClientConfiguration;
  subagentDraft?: CodexSubagentSettings;
  onApplied: () => void;
}

function currentSettings({ app, editorState }: Pick<Props, "app" | "editorState">) {
  if (!editorState.editor || !editorState.draft) return null;
  return clientSettingsPayload(app, editorState.draft, editorState.claudeExtra);
}

const REVIEW_HELP = "只读展示脱敏后的真实配置；点击右上角编辑图标可修改界面未拥有的字段。";
const EDIT_HELP =
  "敏感值以脱敏标记显示，保持标记不变会在后台保留原值；仅可修改界面未拥有的字段，标准设置、供应商参数与 ASB 管理的额外配置必须在各自模块修改。";

function CurrentConfigurationRepair({ app, busy, source, onApplied }: Pick<Props, "app" | "busy" | "source" | "onApplied">) {
  const [preview, setPreview] = useState<ClientConfigurationRepairPreview | null>(null);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => { setPreview(null); setError(null); }, [source.contentHash]);
  const prepare = () => {
    if (busy || working) return;
    setWorking(true); setError(null); setPreview(null);
    void previewClientConfigurationRepair(app, source.contentHash).then(setPreview)
      .catch((caught: { message?: string }) => setError(caught.message ?? "无法生成自动修复预览"))
      .finally(() => setWorking(false));
  };
  const commit = () => {
    if (!preview || busy || working) return;
    setWorking(true); setError(null);
    void commitClientConfigurationRepair(app, source.contentHash, preview.file.renderedHash, preview.targetExisted)
      .then(() => { setPreview(null); onApplied(); })
      .catch((caught: { message?: string }) => setError(caught.message ?? "自动修复客户端配置失败"))
      .finally(() => setWorking(false));
  };
  return <>
    <p className="asb-field-error" role="alert">当前机器真实配置格式无效：{source.syntaxError ?? "无法安全解析"}</p>
    {!preview ? <div className="asb-client-configuration-file-actions">
      <Button variant="secondary" disabled={busy || working} onClick={prepare}>
        {working ? "正在生成修复预览" : "自动修复配置"}
      </Button>
    </div> : <>
      <CodePreview target={`自动修复候选 · ${preview.file.preview.target}`} content={preview.file.content} />
      <div className="asb-client-configuration-file-actions">
        <Button variant="secondary" disabled={busy || working} onClick={() => setPreview(null)}>返回</Button>
        <Button variant="primary" disabled={busy || working} onClick={commit}>{working ? "正在修复" : "确认自动修复"}</Button>
      </div>
    </>}
    {error && <p className="asb-field-error" role="alert">{error}</p>}
  </>;
}

function useCurrentConfigurationEdit({ app, busy, editorState, source, subagentDraft, onApplied }: Props) {
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(source.content);
  const [preview, setPreview] = useState<ClientConfigurationApplyPreview | null>(null);
  const [working, setWorking] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const settings = currentSettings({ app, editorState });
  const canWrite = !!settings && (app !== "codex" || !!subagentDraft);
  useEffect(() => {
    setDraft(source.content); setEditing(false); setPreview(null); setError(null);
  }, [source.content, source.contentHash]);
  const changeDraft = (content: string) => { setDraft(content); setPreview(null); setError(null); };
  const prepare = () => {
    if (!settings || busy || working) return;
    setWorking(true); setError(null); setPreview(null);
    void previewManualClientConfiguration(app, source.contentHash, draft, settings, subagentDraft).then(setPreview)
      .catch((caught: { message?: string }) => setError(caught.message ?? "无法生成手动配置预览"))
      .finally(() => setWorking(false));
  };
  const commit = () => {
    if (!settings || !preview || busy || working) return;
    setWorking(true); setError(null);
    void commitManualClientConfiguration(app, source.contentHash, preview.file.renderedHash, preview.settingsHash,
      preview.targetExisted, draft, settings, subagentDraft).then(() => { setPreview(null); onApplied(); })
      .catch((caught: { message?: string }) => setError(caught.message ?? "应用手动客户端配置失败"))
      .finally(() => setWorking(false));
  };
  return { canWrite, changeDraft, commit, draft, editing, error, prepare, preview, setEditing, setPreview, setDraft, setError, working };
}

/** The configuration-review file module: one surface reviews the redacted
 * real client configuration and turns editable in place through the edit
 * icon in its heading, instead of a read-only preview above a separate
 * manual editor below. */
export function CurrentConfigurationEditor(props: Props) {
  const { app, busy, source } = props;
  const edit = useCurrentConfigurationEdit(props);
  const heading = (help: string, icon?: ReactNode) => (
    <div className="asb-client-configuration-file-heading">
      <div>
        <h3 className="asb-section-title">当前机器真实配置</h3>
        <p className="asb-field-help">{help}</p>
      </div>
      {icon}
    </div>
  );
  if (!source.syntaxOk) {
    return (
      <section className="asb-client-configuration-file" aria-label="自动修复当前机器真实配置">
        {heading("真实配置无法安全解析时不可直接编辑；自动修复会重建为可解析内容。")}
        <CurrentConfigurationRepair app={app} busy={busy} source={source} onApplied={props.onApplied} />
      </section>
    );
  }
  return (
    <section className="asb-client-configuration-file" aria-label="当前机器真实配置">
      {heading(edit.editing || edit.preview ? EDIT_HELP : REVIEW_HELP, !edit.editing && !edit.preview ? (
        <Button variant="icon" aria-label="编辑当前机器真实配置" disabled={busy} onClick={() => edit.setEditing(true)}>
          <EditIcon />
        </Button>
      ) : undefined)}
      {edit.preview ? <>
        <CodePreview target={`手动配置候选 · ${edit.preview.file.preview.target}`} content={edit.preview.file.content} />
        <div className="asb-client-configuration-file-actions">
          <Button variant="secondary" disabled={busy || edit.working} onClick={() => edit.setPreview(null)}>返回继续编辑</Button>
          <Button variant="primary" disabled={!edit.canWrite || busy || edit.working} onClick={edit.commit}>
            {edit.working ? "正在应用" : "确认应用手动修改"}
          </Button>
        </div>
      </> : edit.editing ? <>
        <EditableCodePreview target={`当前机器真实配置 · ${source.target}`} content={edit.draft}
          disabled={busy || edit.working} onChange={edit.changeDraft} />
        <div className="asb-client-configuration-file-actions">
          <Button variant="secondary" disabled={busy || edit.working} onClick={() => {
            edit.setDraft(source.content); edit.setEditing(false); edit.setError(null);
          }}>放弃修改</Button>
          <Button variant="primary" disabled={!edit.canWrite || edit.draft === source.content || busy || edit.working} onClick={edit.prepare}>
            {edit.working ? "正在生成预览" : "预览手动修改"}
          </Button>
        </div>
      </> : source.exists ? (
        <CodePreview target={`当前机器真实配置 · ${source.target}`} content={source.content} />
      ) : (
        <p className="asb-empty">当前机器尚未创建 {source.target}；点击右上角编辑图标可直接创建。</p>
      )}
      {edit.error && <p className="asb-field-error" role="alert">{edit.error}</p>}
    </section>
  );
}
