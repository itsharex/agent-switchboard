import { useState } from "react";
import type { AppKind, CcSwitchImportOutcome, CcSwitchScan, DiscoveryReport } from "../api/client";
import { Button } from "../components/Button";
import { RadioOption } from "../components/RadioOption";
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
  onSeedCc: (key: string) => void;
}

/** Local discovery uses the selected client; batch import stays cross-client. */
export function ProviderImportPage(props: ProviderImportPageProps) {
  const localImportAvailable = props.appFilter === "claude";
  const [selectedSource, setSelectedSource] = useState<"local" | "ccswitch">("local");
  const source = localImportAvailable ? selectedSource : "ccswitch";
  const importLocal = async () => {
    if (await props.onImportLocal()) props.onBack();
  };
  const importCc = async () => {
    if (await props.onImportCc()) props.onBack();
  };
  return (
    <div className="asb-provider-import">
      <div className="asb-panel-heading">
        <div className="asb-panel-heading-main">
          <Button variant="back" disabled={props.busy} aria-label="返回供应商" onClick={props.onBack}>←</Button>
          <h2 className="asb-panel-title">导入供应商</h2>
        </div>
      </div>
      {localImportAvailable && <section className="asb-panel" aria-label="导入供应商">
        <div className="asb-segments" role="radiogroup" aria-label="导入来源">
          <RadioOption name="provider-import-source" checked={source === "local"} disabled={props.busy}
            label="本机配置" onChange={() => setSelectedSource("local")} />
          <RadioOption name="provider-import-source" checked={source === "ccswitch"} disabled={props.busy}
            label="CC Switch" onChange={() => setSelectedSource("ccswitch")} />
        </div>
      </section>}
      {source === "local" ? <LocalConfigImport app="claude" discovery={props.discovery} busy={props.busy}
        onScan={props.onScanLocal} onImport={() => void importLocal()} />
        : <CcSwitchImport scan={props.ccScan} selected={props.ccSelected} result={props.ccResult}
          busy={props.busy} onScan={props.onScanCc} onSelect={props.onSelectCc} onImport={() => void importCc()}
          onSeed={props.onSeedCc} />}
    </div>
  );
}
