import { useState } from "react";
import type { AppKind, CcSwitchImportOutcome, CcSwitchScan, DiscoveryReport } from "../api/client";
import { Button } from "../components/Button";
import { ClientPicker } from "../components/ClientPicker";
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
  onSelectApp: (app: AppKind) => void;
  onBack: () => void;
  onScanLocal: () => void;
  onImportLocal: (app: AppKind) => Promise<boolean>;
  onScanCc: () => void;
  onSelectCc: (key: string, checked: boolean) => void;
  onImportCc: () => Promise<boolean>;
}

/** Local discovery uses the selected client; CC Switch retains cross-client batch import. */
export function ProviderImportPage(props: ProviderImportPageProps) {
  const [source, setSource] = useState<"local" | "ccswitch">("local");
  const importLocal = async (app: AppKind) => {
    if (await props.onImportLocal(app)) props.onBack();
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
        {source === "local" && <ClientPicker app={props.appFilter} onChange={props.onSelectApp}
          disabled={props.busy} label="导入客户端" />}
      </div>
      <section className="asb-panel" aria-label="导入供应商">
        <div className="asb-provider-import-sources" role="group" aria-label="导入来源">
          <Button variant="secondary" className="asb-provider-import-source" aria-pressed={source === "local"}
            disabled={props.busy} onClick={() => setSource("local")}>本机配置</Button>
          <Button variant="secondary" className="asb-provider-import-source" aria-pressed={source === "ccswitch"}
            disabled={props.busy} onClick={() => setSource("ccswitch")}>CC Switch</Button>
        </div>
      </section>
      {source === "local" ? <LocalConfigImport app={props.appFilter} discovery={props.discovery} busy={props.busy}
        onScan={props.onScanLocal} onImport={(app) => void importLocal(app)} />
        : <CcSwitchImport scan={props.ccScan} selected={props.ccSelected} result={props.ccResult}
          busy={props.busy} onScan={props.onScanCc} onSelect={props.onSelectCc} onImport={() => void importCc()} />}
    </div>
  );
}
