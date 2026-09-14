import { useState } from "react";
import { Button } from "../Button";
import { ExtensionDialog } from "../extensions/ExtensionDialog";
import { ModuleHeader } from "../WorkspaceHeader";
import { Tabs } from "../Tabs";
import { AccountsPane } from "./AccountsPane";
import { ProvidersPane } from "./ProvidersPane";
import { CommonPane } from "./CommonPane";
import { GatewayPane } from "./GatewayPane";
import { MeteringPane } from "./MeteringPane";
import { PromptsPane } from "./PromptsPane";
import { useCodexOperations } from "./operations";
const PANES = [
  { value: "providers", label: "供应商" }, { value: "accounts", label: "认证" },
  { value: "common", label: "通用配置" }, { value: "gateway", label: "网关与 Failover" },
  { value: "metering", label: "计量" }, { value: "prompts", label: "指令预设" },
] as const;
type Pane = typeof PANES[number]["value"];
function ToolsDialog({ onClose, onChanged }: { onClose: () => void; onChanged: () => void }) {
  const [pane, setPane] = useState<Pane>("providers");
  const operations = useCodexOperations(onChanged);
  return <ExtensionDialog title="Codex 本地功能管理" busy={operations.busy} onClose={onClose} wide>
    <Tabs scope="codex-management" label="Codex 功能" value={pane} onChange={setPane}
      tabs={PANES.map((tab) => ({ ...tab, disabled: operations.busy, controls: `codex-management-${tab.value}` }))} />
    {operations.error && <p role="alert" className="asb-field-error">{operations.error}</p>}
    {operations.notice && <p role="status" className="asb-scope-note">{operations.notice}</p>}
    <div role="tabpanel" id={`codex-management-${pane}`} aria-labelledby={`codex-management-${pane}-tab`}>
      {pane === "providers" && <ProvidersPane operations={operations} />}
      {pane === "accounts" && <AccountsPane operations={operations} />}
      {pane === "common" && <CommonPane operations={operations} />}
      {pane === "gateway" && <GatewayPane operations={operations} />}
      {pane === "metering" && <MeteringPane operations={operations} />}
      {pane === "prompts" && <PromptsPane operations={operations} />}
    </div>
  </ExtensionDialog>;
}
/** Existing provider/editor layouts stay unchanged; additional functions live in a settings dialog. */
export function CodexToolsLauncher({ onChanged }: { onChanged: () => void }) {
  const [open, setOpen] = useState(false);
  return <section className="asb-panel" aria-label="Codex 本地功能管理入口">
    <ModuleHeader title="Codex 本地功能" primaryActions={<Button variant="secondary" onClick={() => setOpen(true)}>管理 Codex 功能</Button>} />
    <p className="asb-scope-note">独立管理供应商预设、账号、通用配置、故障转移与计量。MCP 和 Skills 仍在扩展工作区管理。</p>
    {open && <ToolsDialog onClose={() => setOpen(false)} onChanged={onChanged} />}
  </section>;
}
