import { useState } from "react";
import { Button } from "../Button";
import { ExtensionDialog } from "../extensions/ExtensionDialog";
import { ModuleHeader } from "../WorkspaceHeader";
import { Tabs } from "../Tabs";
import { ClaudeManagementGroup } from "../claude-management/ClaudeToolsLauncher";
import { useClaudeOperations } from "../claude-management/operations";
import { CodexManagementGroup } from "../codex-management/CodexToolsLauncher";
import { useCodexOperations } from "../codex-management/operations";

const GROUPS = [
  { value: "connection", label: "连接与认证" },
  { value: "routing", label: "路由与计量" },
  { value: "workspace", label: "工作区配置" },
] as const;

type Client = "codex" | "claude";
type Group = typeof GROUPS[number]["value"];

function ManagementDialog({ onClose, onChanged }: { onClose: () => void; onChanged: () => void }) {
  const [client, setClient] = useState<Client>("codex");
  const [group, setGroup] = useState<Group>("connection");
  const codex = useCodexOperations(onChanged);
  const claude = useClaudeOperations(onChanged);
  const operations = client === "codex" ? codex : claude;

  return <ExtensionDialog title="本地功能管理" busy={operations.busy} onClose={onClose} wide>
    <Tabs scope="local-management-client" label="客户端" value={client} onChange={setClient}
      tabs={[{ value: "codex", label: "Codex", disabled: operations.busy }, { value: "claude", label: "Claude", disabled: operations.busy }]} />
    <Tabs scope="local-management-group" label="管理分类" value={group} onChange={setGroup}
      tabs={GROUPS.map((item) => ({ ...item, disabled: operations.busy, controls: `local-management-${client}-${item.value}` }))} />
    {operations.error && <p role="alert" className="asb-field-error">{operations.error}</p>}
    {operations.notice && <p role="status" className="asb-scope-note">{operations.notice}</p>}
    <div role="tabpanel" id={`local-management-${client}-${group}`} aria-labelledby={`local-management-group-${group}-tab`}>
      {client === "codex"
        ? <CodexManagementGroup group={group} operations={codex} />
        : <ClaudeManagementGroup group={group} operations={claude} />}
    </div>
  </ExtensionDialog>;
}

/** One entry point keeps client-specific management discoverable without changing either provider editor. */
export function LocalManagementLauncher({ onChanged }: { onChanged: () => void }) {
  const [open, setOpen] = useState(false);
  return <section className="asb-panel" aria-label="本地功能管理入口">
    <ModuleHeader title="本地功能管理" primaryActions={<Button variant="secondary" onClick={() => setOpen(true)}>管理本地功能</Button>} />
    <p className="asb-scope-note">按客户端管理连接、认证、路由、计量和工作区配置。MCP 与 Skills 仍在扩展工作区。</p>
    {open && <ManagementDialog onClose={() => setOpen(false)} onChanged={onChanged} />}
  </section>;
}
