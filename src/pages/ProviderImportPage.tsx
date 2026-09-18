import { useState } from "react";
import type { AppKind, CcSwitchImportOutcome, CcSwitchScan, DiscoveryReport } from "../api/client";
import { Tabs } from "../components/Tabs";
import { EditorFrame } from "../components/EditorFrame";
import { CcSwitchImport } from "./provider-import/CcSwitchImport";
import { LocalConfigImport } from "./provider-import/LocalConfigImport";
import "../styles/base/provider-workspace.css";

interface ProviderImportPageProps {
  appFilter: AppKind;
  discovery: DiscoveryReport | null;
  ccScan: CcSwitchScan | null;
  ccSelected: Record<string, boolean>;
  ccResult: CcSwitchImportOutcome | null;
  busy: boolean;
  onBack: () => void;
  onScanLocal: () => void;
  onImportLocal: () => Promise<boolean>;
  onScanCc: () => void;
  onSelectCc: (key: string, checked: boolean) => void;
  onImportCc: () => Promise<boolean>;
}

/** Local discovery uses the selected client; batch import stays cross-client.
 * Scan and import actions belong to each source panel, so the frame runs
 * without a persistent bottom action bar. */
export function ProviderImportPage(props: ProviderImportPageProps) {
  const [source, setSource] = useState<"local" | "ccswitch">("local");
  const importLocal = async () => {
    if (await props.onImportLocal()) props.onBack();
  };
  const importCc = async () => {
    if (await props.onImportCc()) props.onBack();
  };
  return (
    <EditorFrame title="导入供应商" backLabel="返回供应商" busy={props.busy} onBack={props.onBack}>
      <div className="asb-provider-import">
        <Tabs value={source} onChange={setSource} scope="provider-import" label="导入来源"
          tabs={[
            { value: "local", label: "本机配置", controls: "provider-import-local-panel", disabled: props.busy },
            { value: "ccswitch", label: "本机数据库", controls: "provider-import-ccswitch-panel", disabled: props.busy },
          ]} />
        {/* Both tabpanels stay mounted so each tab's aria-controls always
            resolves; inactive content unmounts inside its hidden panel. */}
        <div id="provider-import-local-panel" role="tabpanel" aria-labelledby="provider-import-local-tab"
          hidden={source !== "local"}>
          {source === "local" && <LocalConfigImport app={props.appFilter} discovery={props.discovery} busy={props.busy}
            onScan={props.onScanLocal} onImport={() => void importLocal()} />}
        </div>
        <div id="provider-import-ccswitch-panel" role="tabpanel" aria-labelledby="provider-import-ccswitch-tab"
          hidden={source !== "ccswitch"}>
          {source === "ccswitch" && <CcSwitchImport scan={props.ccScan} selected={props.ccSelected} result={props.ccResult}
            busy={props.busy} onScan={props.onScanCc} onSelect={props.onSelectCc} onImport={() => void importCc()} />}
        </div>
      </div>
    </EditorFrame>
  );
}
