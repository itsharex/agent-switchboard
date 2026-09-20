import { useEffect, useState } from "react";
import { SessionManager } from "../components/SessionManager";
import { ClientConfigurationWorkspace } from "./ClientConfigurationWorkspace";
import { ExtensionsWorkspace } from "./ExtensionsWorkspace";
import { ProvidersWorkspace } from "./ProvidersWorkspace";
import { SettingsWorkspace } from "./SettingsWorkspace";
import { UsageWorkspace } from "./UsageWorkspace";
import type { Page } from "./navigation";
import type { SwitchboardModel } from "./useSwitchboardModel";

export function AppWorkspace({ model }: { model: SwitchboardModel }) {
  const { page } = model;
  const [opened, setOpened] = useState<ReadonlySet<Page>>(() => new Set([page]));
  useEffect(() => {
    setOpened((current) => current.has(page) ? current : new Set([...current, page]));
  }, [page]);
  const visited = (target: Page) => page === target || opened.has(target);
  return (
    <main className="asb-main" aria-label={page}>
      <div hidden={page !== "供应商切换"} className="asb-page-stack">
        <ProvidersWorkspace model={model} active={page === "供应商切换"} />
      </div>
      {visited("客户端配置") && <div hidden={page !== "客户端配置"} className="asb-page-stack">
        <ClientConfigurationWorkspace model={model} />
      </div>}
      {visited("扩展") && <div hidden={page !== "扩展"} className="asb-page-stack">
        <ExtensionsWorkspace model={model} />
      </div>}
      {visited("会话记录") && <section className="asb-panel" hidden={page !== "会话记录"} aria-label="会话管理">
        <SessionManager active={page === "会话记录"} />
      </section>}
      {visited("用量监控") && <div hidden={page !== "用量监控"} className="asb-page-stack">
        <UsageWorkspace active={page === "用量监控"} section={model.usageSection} onSectionChange={model.setUsageSection} />
      </div>}
      {visited("设置") && <div hidden={page !== "设置"} className="asb-page-stack">
        <SettingsWorkspace model={model} active={page === "设置"} />
      </div>}
    </main>
  );
}
