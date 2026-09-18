import { useState } from "react";
import type { AppKind, CcSwitchImportOutcome, CcSwitchScan, DiscoveryReport } from "../api/client";
import { RadioOption } from "../components/RadioOption";
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
  const localImportAvailable = true;
  const [selectedSource, setSelectedSource] = useState<"local" | "ccswitch">("local");
  const source = localImportAvailable ? selectedSource : "ccswitch";
  const importLocal = async () => {
    if (await props.onImportLocal()) props.onBack();
  };
  const importCc = async () => {
    if (await props.onImportCc()) props.onBack();
  };
  return (
    <EditorFrame title="导入供应商" backLabel="返回供应商" busy={props.busy} onBack={props.onBack}>
      <div className="asb-provider-import">
        {localImportAvailable && <div className="asb-segments" role="radiogroup" aria-label="导入来源">
          <RadioOption name="provider-import-source" checked={source === "local"} disabled={props.busy}
            label="本机配置" onChange={() => setSelectedSource("local")} />
          <RadioOption name="provider-import-source" checked={source === "ccswitch"} disabled={props.busy}
            label="本机数据库" onChange={() => setSelectedSource("ccswitch")} />
        </div>}
        {source === "local" ? <LocalConfigImport app={props.appFilter} discovery={props.discovery} busy={props.busy}
          onScan={props.onScanLocal} onImport={() => void importLocal()} />
          : <CcSwitchImport scan={props.ccScan} selected={props.ccSelected} result={props.ccResult}
            busy={props.busy} onScan={props.onScanCc} onSelect={props.onSelectCc} onImport={() => void importCc()} />}
      </div>
    </EditorFrame>
  );
}
