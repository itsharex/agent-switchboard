import { useMemo, useState } from "react";
import type { ProviderRequestTarget } from "../../api/client";
import type { ProviderDiagnosticsReport } from "../../api/provider-diagnostics";
import { useI18n } from "../../i18n";
import { AppDialog } from "../AppDialog";
import { Button } from "../Button";
import { ProbeFeedback, useEndpointProbe } from "../EndpointProbe";
import { ProviderRequestPanel } from "../ProviderRequestPanel";
import { Tabs } from "../Tabs";
import { ProviderDiagnosticReport } from "./ProviderDiagnosticReport";
import { ProviderRepairPane } from "./ProviderRepairPane";
import { useProviderDiagnosis } from "./use-provider-diagnosis";
import { useProviderRepair } from "./use-provider-repair";
import "../../styles/base/provider-diagnostics.css";

function NetworkChecks({ report }: { report: ProviderDiagnosticsReport }) {
  const { t } = useI18n();
  const probe = useEndpointProbe(report.endpoint);
  const target = useMemo<ProviderRequestTarget>(() => ({ kind: "saved", profileId: report.profileId }), [report.profileId]);
  if (report.official) return <p>{t("doctor.officialTest")}</p>;
  return <div className="asb-doctor-network">
    <p className="asb-scope-note">{t("doctor.networkNote")}</p>
    {report.endpoint && <section>
      <p className="asb-code">{report.endpoint}</p>
      <Button variant="secondary" disabled={probe.busy} onClick={() => void probe.run()}>{t("doctor.connectivity")}</Button>
      <ProbeFeedback result={probe.result} error={probe.error} />
    </section>}
    <h3 className="asb-section-title">{t("doctor.request")}</h3>
    <ProviderRequestPanel target={target} name={report.profileName} />
  </div>;
}

export function ProviderDiagnosticsDialog({ profileId, name, onClose }: {
  profileId: string; name: string; onClose: () => void;
}) {
  const { t } = useI18n();
  const [view, setView] = useState<"local" | "network">("local");
  const diagnosis = useProviderDiagnosis(profileId);
  const repair = useProviderRepair(profileId, diagnosis.refresh);
  const busy = diagnosis.busy || repair.busy;
  return <AppDialog title={`${name} · ${t("doctor.title")}`} busy={repair.busy} onClose={onClose} wide>
    <div className="asb-doctor-toolbar">
      <Tabs value={view} onChange={setView} scope="provider-doctor" label={t("doctor.tabs")} tabs={[
        { value: "local", label: t("doctor.local"), controls: "provider-doctor-local", disabled: repair.busy || !!repair.pending },
        { value: "network", label: t("doctor.network"), controls: "provider-doctor-network", disabled: busy || !!repair.pending || !diagnosis.report },
      ]} />
      {view === "local" && <Button variant="secondary" disabled={busy || !!repair.pending}
        onClick={() => void diagnosis.refresh()}>{t("doctor.refresh")}</Button>}
    </div>
    {diagnosis.busy && <p role="status">{t("doctor.loading")}</p>}
    {diagnosis.error && <p className="asb-warn-text" role="alert">{diagnosis.error}</p>}
    <div id="provider-doctor-local" role="tabpanel" aria-labelledby="provider-doctor-local-tab" hidden={view !== "local"}>
      {diagnosis.report && <ProviderDiagnosticReport report={diagnosis.report} />}
      <ProviderRepairPane repair={repair} canRepair={diagnosis.report?.canRepair ?? false} disabled={busy} />
    </div>
    <div id="provider-doctor-network" role="tabpanel" aria-labelledby="provider-doctor-network-tab" hidden={view !== "network"}>
      {view === "network" && diagnosis.report && <NetworkChecks report={diagnosis.report} />}
    </div>
  </AppDialog>;
}
