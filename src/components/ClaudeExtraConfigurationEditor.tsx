import { useEffect, useRef, useState } from "react";

import { parseClaudeExtraConfiguration } from "../api/client";
import { Button } from "./Button";
import { EditableCodePreview } from "./EditableCodePreview";

interface Props {
  extra: Record<string, unknown> | undefined;
  busy: boolean;
  onChange: (extra: Record<string, unknown>) => void;
}

function serialized(extra: Record<string, unknown> | undefined): string {
  return JSON.stringify(extra ?? {}, null, 2);
}

function leafCount(value: unknown): number {
  if (!value || typeof value !== "object" || Array.isArray(value)) return 1;
  const entries = Object.values(value);
  return entries.length === 0 ? 0 : entries.reduce((total, child) => total + leafCount(child), 0);
}

export function ClaudeExtraConfigurationEditor({ extra, busy, onChange }: Props) {
  const source = serialized(extra);
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState(source);
  const [parsing, setParsing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const revision = useRef(0);
  const count = Object.values(extra ?? {}).reduce((total, value) => total + leafCount(value), 0);

  useEffect(() => {
    if (!editing) setDraft(source);
  }, [editing, source]);

  const begin = () => {
    setDraft(source); setError(null); setNotice(null); setEditing(true);
  };
  const discard = () => {
    revision.current += 1;
    setDraft(source); setError(null); setEditing(false);
  };
  const save = () => {
    if (busy || parsing || draft === source) return;
    const currentRevision = revision.current + 1;
    revision.current = currentRevision;
    setParsing(true); setError(null);
    void parseClaudeExtraConfiguration(draft).then((next) => {
      if (revision.current !== currentRevision) return;
      onChange(next);
      setEditing(false);
      setNotice("已加入当前通用设置草稿；关闭配置审阅后可保存并预览应用。");
    }).catch((caught: { message?: string }) => {
      if (revision.current === currentRevision) {
        setError(caught.message ?? "Claude 额外通用配置无效");
      }
    }).finally(() => {
      if (revision.current === currentRevision) setParsing(false);
    });
  };

  return (
    <section className="asb-client-claude-extra-editor" aria-label="ASB 管理的额外通用配置">
      <div className="asb-client-claude-extra-editor-heading">
        <div>
          <h3 className="asb-section-title">ASB 管理的额外通用配置</h3>
          <p className="asb-field-help">
            仅用于 Claude 未被可视化表单覆盖的通用字段；供应商、凭据与扩展字段会被拒绝。
          </p>
        </div>
        {!editing && <Button variant="secondary" disabled={busy} onClick={begin}>编辑额外通用配置</Button>}
      </div>
      {!editing && <p className="asb-field-help">当前管理 {count} 项额外配置。</p>}
      {editing && <>
        <EditableCodePreview
          target="Claude 额外通用配置草稿"
          content={draft}
          disabled={busy || parsing}
          onChange={(content) => { setDraft(content); setError(null); setNotice(null); }}
        />
        <div className="asb-client-claude-extra-editor-actions">
          <Button variant="secondary" disabled={busy || parsing} onClick={discard}>放弃修改</Button>
          <Button variant="primary" disabled={busy || parsing || draft === source} onClick={save}>
            {parsing ? "正在校验" : "保存额外配置草稿"}
          </Button>
        </div>
      </>}
      {notice && <p className="asb-field-help" role="status">{notice}</p>}
      {error && <p className="asb-field-error" role="alert">{error}</p>}
    </section>
  );
}