import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, Copy } from "lucide-react";
import { useI18n } from "../i18n";
import { Button } from "./Button";
import { ModuleHeader } from "./WorkspaceHeader";

export function ConfirmGatewayRecoveryDiscard({ onConfirm }: { onConfirm: () => void }) {
  const { t } = useI18n();
  const [confirming, setConfirming] = useState(false);
  if (!confirming) {
    return (
      <div>
        <Button variant="secondary" onClick={() => setConfirming(true)}>
          {t("gateway.discardRecordButton")}
        </Button>
      </div>
    );
  }
  return (
    <div className="asb-gateway-row">
      <span className="asb-gateway-guidance-copy">
        {t("gateway.discardConfirmCopy")}
      </span>
      <Button variant="danger" onClick={onConfirm}>{t("gateway.discardConfirmButton")}</Button>
      <Button variant="secondary" onClick={() => setConfirming(false)}>{t("gateway.cancel")}</Button>
    </div>
  );
}

export function CopyGatewayAddressButton({ value }: { value: string }) {
  const { t } = useI18n();
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | null>(null);
  useEffect(() => () => {
    if (timer.current !== null) window.clearTimeout(timer.current);
  }, []);
  return (
    <Button
      variant="icon"
      aria-label={copied ? t("gateway.copiedAddressAria") : t("gateway.copyAddressAria")}
      title={copied ? t("gateway.copiedTitle") : t("gateway.copyTitle")}
      onClick={() => {
        void navigator.clipboard?.writeText(value).then(() => {
          setCopied(true);
          if (timer.current !== null) window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => setCopied(false), 1500);
        });
      }}
    >
      {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
    </Button>
  );
}

/** 网关页骨架的唯一拥有者：模块标题加「拓扑主图—图表带—最近请求」内容栈。 */
export function GatewayPanel({ children }: { children: ReactNode }) {
  const { t } = useI18n();
  return (
    <section className="asb-panel asb-gateway-panel" aria-label={t("gateway.panelAriaLabel")}>
      <ModuleHeader title={t("gateway.title")} primary={<p className="asb-gateway-intro">{t("gateway.intro")}</p>} />
      <div className="asb-gateway-stack">{children}</div>
    </section>
  );
}
