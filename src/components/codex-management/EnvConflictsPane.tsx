import { useState } from "react";
import * as api from "../../api/codex-env";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Table } from "../Table";
import type { CodexOperations } from "./operations";
const key = (entry: { varName: string; source: api.CodexEnvSource }) => entry.varName + "@" + api.describeCodexEnvSource(entry.source);
/** `OPENAI*` variables outside the Codex 配置文件 override the switched route or credential.
 * Scan is read-only; removal writes a full-value backup first. */
export function EnvConflictsPane({ operations: op }: { operations: CodexOperations }) {
  const [scan, setScan] = useState<api.CodexEnvScan | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const [backups, setBackups] = useState<api.CodexEnvBackup[] | null>(null);
  const { run, busy, changed } = op;
  const rescan = async () => { setScan(await api.scanCodexEnvConflicts()); setSelected([]); };
  const chosen = scan?.conflicts.filter((entry) => selected.includes(key(entry))) ?? [];
  return <section aria-label="Codex 环境变量冲突" className="asb-provider-section-fields">
    <p className="asb-scope-note">系统环境或 shell 启动文件里的 OPENAI 变量（如 OPENAI_API_KEY、OPENAI_BASE_URL）会盖过已切换的 Codex 路由与凭据。扫描只读；删除前先写入完整备份，可随时恢复。</p>
    <div className="asb-form-actions">
      <Button variant="secondary" disabled={busy} onClick={() => void run(rescan)}>扫描 OPENAI 环境变量</Button>
      <Button variant="secondary" disabled={busy} onClick={() => void run(async () => setBackups(await api.listCodexEnvBackups()))}>列出环境变量备份</Button>
    </div>
    {scan && scan.conflicts.length === 0 && <p role="status">没有发现 OPENAI 环境变量冲突。</p>}
    {scan && scan.conflicts.length > 0 && <>
      <Table ariaLabel="Codex 环境变量冲突列表" rows={scan.conflicts} rowKey={key} columns={[
        { key: "pick", header: "删除", render: (entry) => <Checkbox label={"删除 " + entry.varName} ariaLabel={"删除 " + entry.varName + "（" + api.describeCodexEnvSource(entry.source) + "）"} checked={selected.includes(key(entry))} disabled={busy}
          onChange={(on) => setSelected(on ? [...selected, key(entry)] : selected.filter((id) => id !== key(entry)))} /> },
        { key: "value", header: "值", render: (entry) => entry.valuePreview },
        { key: "source", header: "来源", render: (entry) => api.describeCodexEnvSource(entry.source) },
      ]} />
      <Button variant="danger" disabled={busy || chosen.length === 0} onClick={() => void run(async () => {
        const backup = await api.removeCodexEnvConflicts(chosen.map(({ varName, source }) => ({ varName, source })), scan.revision, true);
        await rescan(); setBackups(await api.listCodexEnvBackups());
        changed(`已删除 ${backup.entries.length} 个环境变量，备份 ${backup.fileName}。新终端才会看到变化。`);
      })}>备份并删除所选环境变量</Button>
    </>}
    {backups && backups.length === 0 && <p role="status">没有环境变量备份。</p>}
    {backups && backups.length > 0 && <Table ariaLabel="Codex 环境变量备份" rows={backups} rowKey={(backup) => backup.fileName} columns={[
      { key: "file", header: "备份", render: (backup) => backup.fileName },
      { key: "entries", header: "变量", render: (backup) => backup.entries.map((entry) => entry.varName).join(", ") },
      { key: "restore", header: "操作", render: (backup) => <Button variant="secondary" disabled={busy} onClick={() => void run(async () => {
        const count = await api.restoreCodexEnvBackup(backup.fileName, true); await rescan(); changed(`已恢复 ${count} 个环境变量。`);
      })}>恢复 {backup.fileName}</Button> },
    ]} />}
  </section>;
}
