import { useEffect, useState } from "react";

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

function ClientConfigurationRepair({ app, busy, source, onApplied }: Pick<Props, "app" | "busy" | "source" | "onApplied">) {
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
  return <section className="asb-client-manual-configuration" aria-label="自动修复客户端配置">
    <p className="asb-field-error" role="alert">当前机器真实配置格式无效：{source.syntaxError ?? "无法安全解析"}</p>
    {!preview ? <Button variant="secondary" disabled={busy || working} onClick={prepare}>
      {working ? "正在生成修复预览" : "自动修复配置"}
    </Button> : <>
      <CodePreview target={`自动修复候选 · ${preview.file.preview.target}`} content={preview.file.content} />
      <div className="asb-client-manual-configuration-actions">
        <Button variant="secondary" disabled={busy || working} onClick={() => setPreview(null)}>返回</Button>
        <Button variant="primary" disabled={busy || working} onClick={commit}>{working ? "正在修复" : "确认自动修复"}</Button>
      </div>
    </>}
    {error && <p className="asb-field-error" role="alert">{error}</p>}
  </section>;
}

function useManualConfiguration({ app, busy, editorState, source, subagentDraft, onApplied }: Props) {
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

export function ClientManualConfigurationEditor(props: Props) {
  const manual = useManualConfiguration(props);
  if (!props.source.syntaxOk) {
    return <ClientConfigurationRepair app={props.app} busy={props.busy} source={props.source} onApplied={props.onApplied} />;
  }
  return <section className="asb-client-manual-configuration" aria-label="手动修改其他客户端配置">
    <div className="asb-client-manual-configuration-heading">
      <div>
        <h3 className="asb-section-title">{manual.editing ? "编辑其他配置" : "其他配置"}</h3>
        <p className="asb-field-help">仅用于界面未拥有的字段；标准设置、供应商参数与 ASB 管理的额外配置必须在各自模块修改。</p>
      </div>
      {!manual.editing && <Button variant="secondary" disabled={props.busy} onClick={() => manual.setEditing(true)}>编辑其他配置</Button>}
    </div>
    {manual.editing && !manual.preview && <>
      <p className="asb-field-help">敏感值以脱敏标记显示；保持标记不变会在后台保留原值。</p>
      <EditableCodePreview target={`手动配置草稿 · ${props.source.target}`} content={manual.draft}
        disabled={props.busy || manual.working} onChange={manual.changeDraft} />
      <div className="asb-client-manual-configuration-actions">
        <Button variant="secondary" disabled={props.busy || manual.working} onClick={() => {
          manual.setDraft(props.source.content); manual.setEditing(false); manual.setError(null);
        }}>放弃修改</Button>
        <Button variant="primary" disabled={!manual.canWrite || manual.draft === props.source.content || props.busy || manual.working} onClick={manual.prepare}>
          {manual.working ? "正在生成预览" : "预览手动修改"}
        </Button>
      </div>
    </>}
    {manual.preview && <>
      <CodePreview target={`手动配置候选 · ${manual.preview.file.preview.target}`} content={manual.preview.file.content} />
      <div className="asb-client-manual-configuration-actions">
        <Button variant="secondary" disabled={props.busy || manual.working} onClick={() => manual.setPreview(null)}>返回继续编辑</Button>
        <Button variant="primary" disabled={!manual.canWrite || props.busy || manual.working} onClick={manual.commit}>
          {manual.working ? "正在应用" : "确认应用手动修改"}
        </Button>
      </div>
    </>}
    {manual.error && <p className="asb-field-error" role="alert">{manual.error}</p>}
  </section>;
}
