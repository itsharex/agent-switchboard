import { useState } from "react";
import type { ClaudeNativeConfiguration, ProviderDraft } from "../../api/providers";
import { Button } from "../Button";
import { Input } from "../Input";
import { Select } from "../Select";
const FIELDS = {
  bedrock: ["AWS_REGION", "AWS_PROFILE", "AWS_ACCESS_KEY_ID", "AWS_SECRET_ACCESS_KEY", "AWS_SESSION_TOKEN", "AWS_BEARER_TOKEN_BEDROCK"],
  vertex: ["CLOUD_ML_REGION", "ANTHROPIC_VERTEX_PROJECT_ID", "GOOGLE_APPLICATION_CREDENTIALS"],
  foundry: ["ANTHROPIC_FOUNDRY_RESOURCE", "ANTHROPIC_FOUNDRY_API_KEY", "ANTHROPIC_FOUNDRY_AUTH_TOKEN"],
};
export function NativeFields({ draft, change, disabled }: { draft: ProviderDraft; change: (draft: ProviderDraft) => void; disabled: boolean }) {
  const [extra, setExtra] = useState("");
  const native = draft.connection?.claudeNative;
  const environment = native?.environment ?? {};
  const update = (environment: Record<string, string>) => {
    if (native) change({ ...draft, connection: { ...draft.connection, claudeNative: { ...native, environment } } });
  };
  const choose = (kind: string) => {
    if (kind === (native?.kind ?? "http")) return;
    change({ ...draft, baseUrl: null, apiKey: "", authentication: null, upstreamProtocol: "anthropicMessages", responsesOptions: null, maxOutputTokens: null,
      connection: kind === "http" ? {} : { claudeNative: { kind: kind as ClaudeNativeConfiguration["kind"], environment: {} } } });
  };
  const keys = native ? [...new Set([...FIELDS[native.kind], ...Object.keys(environment)])] : [];
  return <section aria-label="Claude 原生云模式" className="asb-provider-section-fields">
    <Select ariaLabel="Claude 请求执行方式" value={native?.kind ?? "http"} disabled={disabled} onChange={choose}
      options={[{ value: "http", label: "HTTP 供应商 / 本机协议网关" }, { value: "bedrock", label: "原生 Amazon Bedrock SDK" }, { value: "vertex", label: "原生 Google Vertex SDK" }, { value: "foundry", label: "原生 Microsoft Foundry SDK" }]} />
    <p className="asb-scope-note">切换原生云模式会清除普通 HTTP 密钥、请求覆盖及计费参数；仅在确认保存预览后生效。云请求由 Claude 自己的 SDK 执行，不会伪装成本机 HTTP 代理。</p>
    {native && <>
      <label className="asb-field"><span>原生云服务根地址（可选）</span><Input type="url" value={draft.baseUrl ?? ""} disabled={disabled} onChange={(e) => change({ ...draft, baseUrl: e.target.value || null })} /></label>
      {keys.map((key) => <label className="asb-field" key={key}><span>{key}</span><Input value={environment[key] ?? ""} disabled={disabled} type={/TOKEN|SECRET|KEY_ID|API_KEY|PASSWORD/.test(key) ? "password" : "text"}
        onChange={(e) => { const env = { ...environment }; if (e.target.value) env[key] = e.target.value; else delete env[key]; update(env); }} /></label>)}
      <div className="asb-form-actions"><Input aria-label="其它原生云环境键" value={extra} disabled={disabled} onChange={(e) => setExtra(e.target.value)} />
        <Button variant="secondary" disabled={disabled || !extra.trim()} onClick={() => { update({ ...environment, [extra.trim()]: "" }); setExtra(""); }}>添加原生云字段</Button></div>
      <p className="asb-scope-note">留空的认证字段使用 SDK 默认凭据链，不读取或复制系统凭据缓存。其它字段保存时会按此云模式的支持列表校验。</p>
    </>}
  </section>;
}
