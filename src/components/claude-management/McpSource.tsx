import { useState } from "react";
import * as api from "../../api/claude-mcp-source";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Input } from "../Input";
import { Table } from "../Table";
import type { ClaudeOperations } from "./operations";
/** Read-only scan of the source application's MCP table, then one confirmed import into the
 * extension library. Deployment to Claude stays in the extension workspace. */
export function McpSource({ operations: op }: { operations: ClaudeOperations }) {
  const [path, setPath] = useState("");
  const [source, setSource] = useState<api.ClaudeMcpSource | null>(null);
  const [selected, setSelected] = useState<string[]>([]);
  const disabled = op.busy;
  const importable = (server: api.ClaudeSourceMcpServer) => !server.problem && !server.existing;
  return <section aria-label="导入本机 MCP 服务" className="asb-provider-section-fields">
    <h3 className="asb-section-title">从本机导入 MCP 服务</h3>
    <p className="asb-scope-note">只读取来源的 MCP 定义写入扩展库；来源中的「已为 Claude 启用」只作提示，部署到 Claude 仍在扩展工作区预览并确认。</p>
    <label className="asb-field"><span>导入源数据库路径</span><Input value={path} disabled={disabled} placeholder="输入源数据库路径" onChange={(e) => { setPath(e.target.value); setSource(null); setSelected([]); }} /></label>
    <Button variant="secondary" disabled={disabled || !path.trim()} onClick={() => void op.run(async () => { setSelected([]); setSource(await api.scanClaudeMcpSource(path.trim())); })}>只读扫描 MCP 服务</Button>
    {source && source.servers.length === 0 && <p role="status">来源没有 MCP 服务。</p>}
    {source && source.servers.length > 0 && <>
      <Table ariaLabel="来源 MCP 服务" rows={source.servers} rowKey={(server) => server.sourceId} columns={[
        { key: "pick", header: "选择", render: (server) => <Checkbox label={"导入 " + server.name} checked={selected.includes(server.sourceId)} disabled={disabled || !importable(server)}
          onChange={(on) => setSelected(on ? [...selected, server.sourceId] : selected.filter((id) => id !== server.sourceId))} /> },
        { key: "transport", header: "传输", render: (server) => server.transport },
        { key: "state", header: "状态", render: (server) => server.problem ? "无法导入：" + server.problem : server.existing ? "扩展库已有同名定义" : server.enabledForClaude ? "来源已为 Claude 启用（不会自动部署）" : "来源未启用" },
      ]} />
      <Button variant="primary" disabled={disabled || selected.length === 0} onClick={() => void op.run(async () => {
        const result = await api.importClaudeMcpSource(path.trim(), selected, source.sourceRevision, true);
        setSource(null); setSelected([]);
        op.changed(`已导入 ${result.imported.length} 个 MCP 定义到扩展库，已有 ${result.unchanged} 个。` + result.warnings.join("；"));
      })}>确认导入所选 MCP 服务到扩展库</Button>
    </>}
  </section>;
}
