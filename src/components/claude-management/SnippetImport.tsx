import { useState } from "react";
import { scanClaudeSnippetSource, importClaudeSnippetSource, type ClaudeSnippetScan } from "../../api/claude-providers";
import { Button } from "../Button";
import { Input } from "../Input";
import type { ClaudeOperations } from "./operations";

/** Read-only scan of the source-wide shared snippet, then one confirmed
 * import into the visual client preferences and the shared extra. */
export function SnippetImport({ operations: op }: { operations: ClaudeOperations }) {
  const [path, setPath] = useState("");
  const [scanned, setScanned] = useState<ClaudeSnippetScan | null>(null);
  const disabled = op.busy;
  const preview = scanned?.preview ?? null;
  return <div className="asb-provider-section-fields">
    <label className="asb-field"><span>CC Switch 数据库路径</span><Input value={path} disabled={disabled} placeholder="~/.cc-switch/cc-switch.db" onChange={(e) => { setPath(e.target.value); setScanned(null); }} /></label>
    <Button variant="secondary" disabled={disabled || !path.trim()} onClick={() => void op.run(async () => setScanned(await scanClaudeSnippetSource(path.trim())))}>只读扫描 Claude 通用配置片段</Button>
    {scanned && !scanned.found && <p role="status">来源没有 Claude 通用配置片段。</p>}
    {preview && <section aria-label="Claude 通用配置片段预览" className="asb-provider-section-fields">
      {preview.visual.length > 0 && <p className="asb-scope-note">可视化偏好 {preview.visual.length} 项：{preview.visual.map((entry) => `${entry.key}=${entry.value}`).join(" · ")}</p>}
      {preview.extraKeys.length > 0 && <p className="asb-scope-note">通用片段 {preview.extraKeys.length} 个键：{preview.extraKeys.join(" ")}</p>}
      {preview.rejected.map((name) => <p className="asb-scope-note" key={name}>未导入: {name}（凭据、供应商路由或扩展模块所有）</p>)}
      <p className="asb-scope-note">只写入应用内的偏好与片段；真实 Claude 配置仍由切换预览确认后应用。</p>
      <Button variant="primary" disabled={disabled} onClick={() => void op.run(async () => {
        const result = await importClaudeSnippetSource(path.trim(), scanned!.sourceRevision, scanned!.settingsRevision, true);
        setScanned(null);
        op.changed(`已导入 Claude 通用配置：偏好 ${result.visualChanged + result.visualUnchanged} 项、片段 ${result.extraChanged} 键。`);
      })}>确认导入到客户端偏好与通用片段</Button>
    </section>}
  </div>;
}
