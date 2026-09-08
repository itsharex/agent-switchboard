import { useEffect, useId, useState } from "react";
import { CodexOfficialResetPanel } from "../components/CodexOfficialResetPanel";
import { CodexResetPanel } from "../components/CodexResetPanel";
import { Tabs } from "../components/Tabs";
import { UsagePage } from "../pages/UsagePage";
import { USAGE_SECTIONS, type UsageSection } from "./navigation";

export function UsageWorkspace({ active, section, onSectionChange }: {
  active: boolean;
  section: UsageSection;
  onSectionChange: (section: UsageSection) => void;
}) {
  const id = useId();
  const [quotaOpened, setQuotaOpened] = useState(section === "quota");
  useEffect(() => { if (section === "quota") setQuotaOpened(true); }, [section]);
  return (
    <section className="asb-page-stack" aria-label="用量工作区">
      <div className="asb-panel-heading">
        <div className="asb-panel-heading-main">
          <h2 className="asb-panel-title">用量</h2>
          <Tabs value={section} onChange={onSectionChange} scope={id} label="用量分类"
            tabs={USAGE_SECTIONS.map((tab) => ({ ...tab, controls: `${id}-${tab.value}-panel` }))} />
        </div>
      </div>
      <div id={`${id}-consumption-panel`} role="tabpanel" aria-labelledby={`${id}-consumption-tab`} hidden={section !== "consumption"}>
        <UsagePage active={active && section === "consumption"} />
      </div>
      <div id={`${id}-quota-panel`} role="tabpanel" aria-labelledby={`${id}-quota-tab`} hidden={section !== "quota"}>
        {(quotaOpened || section === "quota") && <div className="asb-page-stack">
          <CodexOfficialResetPanel />
          <CodexResetPanel />
        </div>}
      </div>
    </section>
  );
}
