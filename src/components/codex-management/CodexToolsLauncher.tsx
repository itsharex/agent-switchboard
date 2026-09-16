import { AccountsPane } from "./AccountsPane";
import { ProvidersPane } from "./ProvidersPane";
import { CommonPane } from "./CommonPane";
import { GatewayPane } from "./GatewayPane";
import { MeteringPane } from "./MeteringPane";
import { PromptsPane } from "./PromptsPane";
import { EnvConflictsPane } from "./EnvConflictsPane";
import { ProjectPlansPane } from "./ProjectPlansPane";
import { HistoryUnifyPane } from "./HistoryUnifyPane";
import { ManagementSection } from "../local-management/ManagementSection";
type Group = "connection" | "routing" | "workspace";

/** Codex keeps its own business panes; the shared launcher only owns navigation. */
export function CodexManagementGroup({ group, operations }: {
  group: Group;
  operations: Parameters<typeof ProvidersPane>[0]["operations"];
}) {
  if (group === "connection") return <div className="asb-management-group">
    <ManagementSection title="供应商" initiallyOpen><ProvidersPane operations={operations} /></ManagementSection>
    <ManagementSection title="认证"><AccountsPane operations={operations} /></ManagementSection>
  </div>;
  if (group === "routing") return <div className="asb-management-group">
    <ManagementSection title="网关与 Failover" initiallyOpen><GatewayPane operations={operations} /></ManagementSection>
    <ManagementSection title="计量"><MeteringPane operations={operations} /></ManagementSection>
  </div>;
  return <div className="asb-management-group">
    <ManagementSection title="通用配置" initiallyOpen><CommonPane operations={operations} /></ManagementSection>
    <ManagementSection title="指令预设"><PromptsPane operations={operations} /></ManagementSection>
    <ManagementSection title="项目方案"><ProjectPlansPane operations={operations} /></ManagementSection>
    <ManagementSection title="统一历史"><HistoryUnifyPane operations={operations} /></ManagementSection>
    <ManagementSection title="环境变量"><EnvConflictsPane operations={operations} /></ManagementSection>
  </div>;
}
