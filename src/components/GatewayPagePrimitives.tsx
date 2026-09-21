import { useEffect, useRef, useState, type ReactNode } from "react";
import { Check, Copy } from "lucide-react";
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
      <span className="asb-gateway-guidance-copy">
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
      variant="icon"
      aria-label={copied ? "已复制网关地址" : "复制网关地址"}
      title={copied ? "已复制" : "复制地址"}
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
  return (
    <section className="asb-panel asb-gateway-panel" aria-label="协议网关">
      <ModuleHeader title="本机协议网关" primary={<p className="asb-gateway-intro">查看客户端的本机转发路径与请求状态。</p>} />
      <div className="asb-gateway-stack">{children}</div>
    </section>
  );
}
