import { useState } from "react";
import { Button } from "../Button";
import { ExtensionDialog } from "../extensions/ExtensionDialog";
import { ModuleHeader } from "../WorkspaceHeader";
import { Tabs } from "../Tabs";
import { useClaudeOperations } from "./operations";
import { ProvidersPane } from "./ProvidersPane";
import { AccountsPane } from "./AccountsPane";
import { GatewayPane } from "./GatewayPane";
import { MeteringPane } from "./MeteringPane";
import { PromptsPane } from "./PromptsPane";
const PANES = [
  { value: "providers", label: "供应商" }, { value: "accounts", label: "认证" },
  { value: "gateway", label: "网关与 Failover" }, { value: "metering", label: "请求计量" },
  { value: "prompts", label: "Prompt 预设" },
] as const;
type Pane = typeof PANES[number]["value"];
function ClaudeDialog({ onClose, onChanged }: { onClose: () => void; onChanged: () => void }) {
  const [pane, setPane] = useState<Pane>("providers");
  const operations = useClaudeOperations(onChanged);
  return <ExtensionDialog title="Claude 本地功能管理" busy={operations.busy} onClose={onClose} wide>
    <Tabs scope="claude-management" label="Claude 功能" value={pane} onChange={setPane}
      tabs={PANES.map((tab) => ({ ...tab, disabled: operations.busy, controls: 'claude-management-' + tab.value }))} />
    {operations.error && <p role="alert" className="asb-field-error">{operations.error}</p>}
    {operations.notice && <p role="status" className="asb-scope-note">{operations.notice}</p>}
    <div role="tabpanel" id={'claude-management-' + pane} aria-labelledby={'claude-management-' + pane + '-tab'}>
      {pane === "providers" && <ProvidersPane operations={operations} />}
      {pane === "accounts" && <AccountsPane operations={operations} />}
      {pane === "gateway" && <GatewayPane operations={operations} />}
      {pane === "metering" && <MeteringPane operations={operations} />}
      {pane === "prompts" && <PromptsPane operations={operations} />}
    </div>
  </ExtensionDialog>;
}
/** Additional workflows live in diagnostics; the Claude provider workspace and visual preferences stay intact. */
export function ClaudeToolsLauncher({ onChanged }: { onChanged: () => void }) {
  const [open, setOpen] = useState(false);
  return <section className="asb-panel" aria-label="Claude 本地功能管理入口">
    <ModuleHeader title="Claude 本地功能" primaryActions={<Button variant="secondary" onClick={() => setOpen(true)}>管理 Claude 功能</Button>} />
    <p className="asb-scope-note">Claude 独立账号、预设、请求策略与账本。通用偏好保留原可视化编辑；MCP 和 Skills 仍在扩展工作区管理。</p>
    {open && <ClaudeDialog onClose={() => setOpen(false)} onChanged={onChanged} />}
  </section>;
}
