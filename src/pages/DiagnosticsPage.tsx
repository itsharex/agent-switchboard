import { useEffect, useState } from "react";
import type { RuntimeLogLevel } from "../api/client";
import { DIAGNOSTIC_SECTIONS, type DiagnosticSection } from "../app/navigation";
import { ConfigStatusPanel, type ConfigStatusPanelProps } from "../components/ConfigStatusPanel";
import { ModuleHeader } from "../components/WorkspaceHeader";
import { RuntimeOverviewPanel } from "../components/RuntimeOverviewPanel";
import { Tabs } from "../components/Tabs";
import { LogsPage } from "./LogsPage";

interface DiagnosticsPageProps extends ConfigStatusPanelProps {
  active: boolean;
  section: DiagnosticSection;
  onSectionChange: (section: DiagnosticSection) => void;
  logLevel: RuntimeLogLevel | null;
  onLogLevelChange: (level: RuntimeLogLevel) => void;
}

export function DiagnosticsPage(props: DiagnosticsPageProps) {
  const { active, section, onSectionChange, profiles, busy } = props;
  const [visited, setVisited] = useState<DiagnosticSection[]>([]);
  useEffect(() => {
    if (active) setVisited((current) => current.includes(section) ? current : [...current, section]);
  }, [active, section]);
  const opened = (target: DiagnosticSection) => visited.includes(target) || (active && section === target);
  return (
    <section className="asb-diagnostics" aria-label="诊断" hidden={!active}>
      {/* 设置内容区的子页：模块级 h3 标题独占第一行，分类页签在第二行。 */}
      <ModuleHeader
        title="诊断"
        primary={
          <Tabs value={section} onChange={onSectionChange} scope="diagnostics" label="诊断内容"
            tabs={DIAGNOSTIC_SECTIONS.map((tab) => ({ ...tab, controls: `diagnostics-${tab.value}-panel` }))} />
        }
      />
      <div id="diagnostics-configuration-panel" role="tabpanel" aria-labelledby="diagnostics-configuration-tab" hidden={section !== "configuration"}>
        {opened("configuration") && <div className="asb-diagnostics-configuration">
          <ConfigStatusPanel statuses={props.statuses} profiles={profiles} locks={props.locks}
            busy={busy} onRefresh={props.onRefresh} onRecoverLock={props.onRecoverLock} />
          <RuntimeOverviewPanel />
        </div>}
      </div>
      <div id="diagnostics-logs-panel" role="tabpanel" aria-labelledby="diagnostics-logs-tab" hidden={section !== "logs"}>
        {opened("logs") && <LogsPage logLevel={props.logLevel} busy={busy} onLogLevelChange={props.onLogLevelChange} />}
      </div>
    </section>
  );
}
