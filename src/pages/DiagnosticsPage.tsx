import { useEffect, useState } from "react";
import type { RuntimeLogLevel } from "../api/client";
import { DIAGNOSTIC_SECTIONS, type DiagnosticSection } from "../app/navigation";
import { ConfigStatusPanel, type ConfigStatusPanelProps } from "../components/ConfigStatusPanel";
import { Button } from "../components/Button";
import { RadioOption } from "../components/RadioOption";
import { Select } from "../components/Select";
import { ModuleHeader } from "../components/WorkspaceHeader";
import { RuntimeOverviewPanel } from "../components/RuntimeOverviewPanel";
import { Tabs } from "../components/Tabs";
import { LogsPage } from "./LogsPage";
import { LEVEL_FILTERS, LOG_LEVEL_OPTIONS, useRuntimeLogs } from "./use-runtime-logs";

interface DiagnosticsPageProps extends ConfigStatusPanelProps {
  active: boolean;
  section: DiagnosticSection;
  onSectionChange: (section: DiagnosticSection) => void;
  onRefresh: () => void;
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
  const logsOpened = opened("logs");
  const logs = useRuntimeLogs(logsOpened);
  return (
    <section className="asb-diagnostics" aria-label="诊断" hidden={!active}>
      {/* 设置内容区的子页：模块级 h3 标题独占第一行；第二行是分类页签与
      页签级动作，视图控制行只挂当前页签自己的控制项。 */}
      <ModuleHeader
        title="诊断"
        primary={
          <Tabs value={section} onChange={onSectionChange} scope="diagnostics" label="诊断内容"
            tabs={DIAGNOSTIC_SECTIONS.map((tab) => ({ ...tab, controls: `diagnostics-${tab.value}-panel` }))} />
        }
        primaryActions={section === "configuration" ? (
          <Button variant="secondary" disabled={busy} onClick={props.onRefresh}>刷新状态</Button>
        ) : undefined}
        secondary={section === "logs" ? (
          <>
            <div className="asb-runtime-log-level-control">
              <span className="asb-runtime-log-level-label">记录级别</span>
              <Select
                value={props.logLevel}
                options={LOG_LEVEL_OPTIONS}
                ariaLabel="记录级别"
                placeholder="加载中"
                disabled={busy || props.logLevel === null}
                onChange={(level) => props.onLogLevelChange(level as RuntimeLogLevel)}
              />
            </div>
            <div className="asb-segments" role="radiogroup" aria-label="日志级别筛选">
              {LEVEL_FILTERS.map((option) => (
                <RadioOption
                  key={option.value}
                  name="runtime-log-level-filter"
                  checked={logs.filter === option.value}
                  disabled={false}
                  label={option.label}
                  onChange={() => logs.setFilter(option.value)}
                />
              ))}
            </div>
            <Button variant="secondary" disabled={logs.loading} onClick={() => void logs.refresh()}>
              {logs.loading ? "刷新中" : "刷新"}
            </Button>
            <Button
              variant="secondary"
              disabled={logs.openingFolder}
              onClick={() => void logs.openLogDirectory()}
            >
              {logs.openingFolder ? "打开中" : "打开日志文件夹"}
            </Button>
          </>
        ) : undefined}
      />
      <div id="diagnostics-configuration-panel" role="tabpanel" aria-labelledby="diagnostics-configuration-tab" hidden={section !== "configuration"}>
        {opened("configuration") && <div className="asb-diagnostics-configuration">
          <ConfigStatusPanel statuses={props.statuses} profiles={profiles} locks={props.locks}
            busy={busy} onRecoverLock={props.onRecoverLock} />
          <RuntimeOverviewPanel />
        </div>}
      </div>
      <div id="diagnostics-logs-panel" role="tabpanel" aria-labelledby="diagnostics-logs-tab" hidden={section !== "logs"}>
        {logsOpened && <LogsPage logs={logs} />}
      </div>
    </section>
  );
}
