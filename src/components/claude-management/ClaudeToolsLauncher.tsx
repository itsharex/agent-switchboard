import { ProvidersPane } from "./ProvidersPane";
import { AccountsPane } from "./AccountsPane";
import { GatewayPane } from "./GatewayPane";
import { MeteringPane } from "./MeteringPane";
import { PromptsPane } from "./PromptsPane";
import { IntegrationPane } from "./IntegrationPane";
import { ManagementSection } from "../local-management/ManagementSection";
type Group = "connection" | "routing" | "workspace";

/** Claude owns these panes; the client-neutral dialog only selects their group. */
export function ClaudeManagementGroup({ group, operations }: {
  group: Group;
  operations: Parameters<typeof ProvidersPane>[0]["operations"];
}) {
  if (group === "connection") return <div className="asb-management-group">
    <ManagementSection title="供应商" initiallyOpen><ProvidersPane operations={operations} /></ManagementSection>
    <ManagementSection title="认证"><AccountsPane operations={operations} /></ManagementSection>
  </div>;
  if (group === "routing") return <div className="asb-management-group">
    <ManagementSection title="网关与 Failover" initiallyOpen><GatewayPane operations={operations} /></ManagementSection>
    <ManagementSection title="请求计量"><MeteringPane operations={operations} /></ManagementSection>
  </div>;
  return <div className="asb-management-group">
    <ManagementSection title="Prompt 预设" initiallyOpen><PromptsPane operations={operations} /></ManagementSection>
    <ManagementSection title="客户端集成"><IntegrationPane operations={operations} /></ManagementSection>
  </div>;
}
