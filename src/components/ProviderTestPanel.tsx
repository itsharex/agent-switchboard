import { useState } from "react";
import type { ProviderRequestTarget } from "../api/client";
import { Button } from "./Button";
import { CloseIcon } from "./icons";
import { ProbeFeedback, useEndpointProbe } from "./EndpointProbe";
import { ProviderRequestPanel } from "./ProviderRequestPanel";
import { RadioOption } from "./RadioOption";

function ConnectivityTest({ url }: { url: string | null }) {
  const probe = useEndpointProbe(url);
  return <div className="asb-connectivity-test">
    <p className="asb-test-address">{url || "请先填写服务地址。"}</p>
    <p className="asb-request-note">仅检测服务地址是否可达，不携带密钥，不发送模型请求。</p>
    <div><Button variant="secondary" disabled={!url || probe.busy} onClick={() => void probe.run()}>
      {probe.busy ? "检测中…" : "开始检测"}
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
  const [mode, setMode] = useState<"connectivity" | "request">("connectivity");
  return <section id={id} className="asb-provider-tests" aria-label={`${name} 供应商测试`}>
    <header className="asb-provider-tests-heading">
      <h3 className="asb-section-title">供应商测试</h3>
      <Button variant="icon" aria-label="收起供应商测试" onClick={onClose}><CloseIcon /></Button>
    </header>
    <div className="asb-segments" role="radiogroup" aria-label="测试类型">
      <RadioOption name={`${id}-mode`} checked={mode === "connectivity"} disabled={false}
        label="连通性测试" onChange={() => setMode("connectivity")} />
      <RadioOption name={`${id}-mode`} checked={mode === "request"} disabled={false}
        label="真实请求" onChange={() => setMode("request")} />
    </div>
    {mode === "connectivity" ? <ConnectivityTest url={url} /> : target ?
      <ProviderRequestPanel target={target} name={name} /> :
      <p className="asb-request-note">请填写服务地址、API 密钥，并选择有效的 API 格式和请求模式。</p>}
  </section>;
}
