import { useState } from "react";
import type { ProviderRequestTarget } from "../api/client";
import { useI18n } from "../i18n";
import { Button } from "./Button";
import { CloseIcon } from "./icons";
import { ProbeFeedback, useEndpointProbe } from "./EndpointProbe";
import { ProviderRequestPanel } from "./ProviderRequestPanel";
import { RadioOption } from "./RadioOption";

function ConnectivityTest({ url }: { url: string | null }) {
  const { t } = useI18n();
  const probe = useEndpointProbe(url);
  return <div className="asb-connectivity-test">
    <p className="asb-test-address">{url || t("providers.test.needUrl")}</p>
    <p className="asb-request-note">{t("providers.test.connectivityNote")}</p>
    <div><Button variant="secondary" disabled={!url || probe.busy} onClick={() => void probe.run()}>
      {probe.busy ? t("providers.test.probing") : t("providers.test.start")}
    </Button></div>
    <ProbeFeedback result={probe.result} error={probe.error} />
  </div>;
}

interface Props {
  id: string;
  name: string;
  url: string | null;
  target: ProviderRequestTarget | null;
  onClose: () => void;
}

export function ProviderTestPanel({ id, name, url, target, onClose }: Props) {
  const { t } = useI18n();
  const [mode, setMode] = useState<"connectivity" | "request">("connectivity");
  return <section id={id} className="asb-provider-tests" aria-label={t("providers.test.aria", { name })}>
    <header className="asb-provider-tests-heading">
      <h3 className="asb-section-title">{t("providers.test.title")}</h3>
      <Button variant="icon" aria-label={t("providers.test.collapseAria")} onClick={onClose}><CloseIcon /></Button>
    </header>
    <div className="asb-segments" role="radiogroup" aria-label={t("providers.test.typeAria")}>
      <RadioOption name={`${id}-mode`} checked={mode === "connectivity"} disabled={false}
        label={t("providers.test.connectivity")} onChange={() => setMode("connectivity")} />
      <RadioOption name={`${id}-mode`} checked={mode === "request"} disabled={false}
        label={t("providers.test.liveRequest")} onChange={() => setMode("request")} />
    </div>
    {mode === "connectivity" ? <ConnectivityTest url={url} /> : target ?
      <ProviderRequestPanel target={target} name={name} /> :
      <p className="asb-request-note">{t("providers.test.needFields")}</p>}
  </section>;
}
