import { useEffect, useRef, useState, type ReactNode } from "react";
import { Button } from "./Button";
import { ModuleHeader } from "./WorkspaceHeader";

export function ConfirmGatewayRecoveryDiscard({ onConfirm }: { onConfirm: () => void }) {
  const [confirming, setConfirming] = useState(false);
  if (!confirming) {
    return (
      <div>
        <Button variant="secondary" onClick={() => setConfirming(true)}>
          保留当前配置并清除恢复记录
        </Button>
      </div>
    );
  }
  return (
    <div className="asb-gateway-row">
      <span className="asb-gateway-alert-copy">
        不会覆盖当前客户端配置（包括外部修改）；会清除本次恢复记录与可清理的备份。
      </span>
      <Button variant="danger" onClick={onConfirm}>确认保留并清除</Button>
      <Button variant="secondary" onClick={() => setConfirming(false)}>取消</Button>
    </div>
  );
}

export function CopyGatewayAddressButton({ value }: { value: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | null>(null);
  useEffect(() => () => {
    if (timer.current !== null) window.clearTimeout(timer.current);
  }, []);
  return (
    <Button
      variant="secondary"
      onClick={() => {
        void navigator.clipboard?.writeText(value).then(() => {
          setCopied(true);
          if (timer.current !== null) window.clearTimeout(timer.current);
          timer.current = window.setTimeout(() => setCopied(false), 1500);
        });
      }}
    >
      {copied ? "已复制" : "复制"}
    </Button>
  );
}

export function GatewayPanel({ children, aside }: { children: ReactNode; aside?: ReactNode }) {
  return (
    <section className="asb-panel" aria-label="协议网关">
      <ModuleHeader title="本机协议网关" primaryActions={aside} />
      <div className="asb-gateway-stack">{children}</div>
    </section>
  );
}

export function GatewayStatTile({ label, value }: { label: string; value: number | string }) {
  return (
    <div role="group" aria-label={label} className="asb-gateway-tile">
      <p className="asb-gateway-tile-label">{label}</p>
      <p className="asb-gateway-tile-value">{value}</p>
    </div>
  );
}
