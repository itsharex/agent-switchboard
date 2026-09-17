import { useState } from "react";
import type { AppKind } from "../api/client";
import { ClientLogo } from "../components/ClientLogo";
import { ModuleHeader } from "../components/WorkspaceHeader";
import { Tabs } from "../components/Tabs";
import { IntegrationPane } from "../components/claude-management/IntegrationPane";
import { useClaudeOperations } from "../components/claude-management/operations";
import { ProjectPlansPane } from "../components/codex-management/ProjectPlansPane";
import { useCodexOperations } from "../components/codex-management/operations";

interface ClientManagementPageProps {
  onChanged: () => void;
}

/** Only client-specific tools without another workspace owner belong here. */
export function ClientManagementPage({ onChanged }: ClientManagementPageProps) {
  const [client, setClient] = useState<AppKind>("codex");
  const codex = useCodexOperations(onChanged);
  const claude = useClaudeOperations(onChanged);
  const operations = client === "codex" ? codex : claude;
  const contentId = "client-management-content";

  return (
    <section className="asb-panel" aria-label="客户端管理">
      <ModuleHeader
        title="客户端管理"
        primary={
          <Tabs scope="client-management-client" label="客户端" value={client} onChange={setClient}
            tabs={[
              {
                value: "codex",
                label: <span className="asb-client-management-tab-label"><ClientLogo app="codex" className="asb-client-management-tab-logo" />Codex</span>,
                controls: contentId,
                disabled: operations.busy,
              },
              {
                value: "claude",
                label: <span className="asb-client-management-tab-label"><ClientLogo app="claude" className="asb-client-management-tab-logo" />Claude</span>,
                controls: contentId,
                disabled: operations.busy,
              },
            ]} />
        }
      />
      <div id={contentId} role="tabpanel" aria-labelledby={`client-management-client-${client}-tab`}>
        {operations.error && <p role="alert" className="asb-field-error">{operations.error}</p>}
        {operations.notice && <p role="status" className="asb-scope-note">{operations.notice}</p>}
        {client === "codex"
          ? <ProjectPlansPane operations={codex} />
          : <IntegrationPane operations={claude} />}
      </div>
    </section>
  );
}