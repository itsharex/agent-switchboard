import { useState } from "react";
import * as api from "../../api/claude-prompts";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Input } from "../Input";
import { Table } from "../Table";
import type { ClaudeOperations } from "./operations";
export function PromptSource({ fileHash, disabled, operations: op, onImported }: {
  fileHash: string; disabled: boolean; operations: ClaudeOperations; onImported: (view: api.ClaudePromptsView) => void;
}) {
  const [path, setPath] = useState("");
  const [source, setSource] = useState<api.ClaudePromptSource | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  return <section aria-label="导入本机 Claude Prompt" className="asb-provider-section-fields">
    <label className="asb-field"><span>导入源数据库路径</span><Input value={path} disabled={disabled} onChange={(e) => { setPath(e.target.value); setSource(null); setSelected([]); }} /></label>
    <Button variant="secondary" disabled={disabled || !path.trim()} onClick={() => void op.run(async () => {
      setSource(null); setSelected([]); setSource(await api.scanClaudePromptSource(path.trim()));
    })}>只读扫描 Claude Prompt</Button>
    {source && <>
      <Table ariaLabel="来源 Claude Prompt" rows={source.prompts} rowKey={(p) => p.sourceId} columns={[
        { key: "select", header: "选择", render: (p) => <Checkbox label={'导入 ' + p.draft.name} checked={selected.includes(p.sourceId)} disabled={disabled}
          onChange={(checked) => setSelected(checked ? [...selected, p.sourceId] : selected.filter((id) => id !== p.sourceId))} /> },
        { key: "name", header: "名称", render: (p) => p.draft.name }, { key: "active", header: "来源状态", render: (p) => p.enabledInSource ? "来源已启用（不会自动启用）" : "未启用" },
      ]} />
      <Button variant="primary" disabled={disabled || selected.length === 0} onClick={() => void op.run(async () => {
        const result = await api.importClaudePromptSource(path.trim(), selected, source.sourceRevision, fileHash, true);
        onImported(result.view); setSource(null); setSelected([]); op.changed('导入 ' + result.imported + ' 项，已有 ' + result.unchanged + ' 项。CLAUDE.md 未改动。' + result.warnings.join("；"));
      })}>确认导入所选 Claude Prompt</Button>
    </>}
  </section>;
}
