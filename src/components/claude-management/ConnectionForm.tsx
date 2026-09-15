import { useState } from "react";
import { prepareProfileSave, commitProfileSave, type ProviderRecord, type ProviderDraft, type ProfileSavePreparation } from "../../api/providers";
import { type ClaudeAccountView, usesClaudeManagedAuth } from "../../api/claude-accounts";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Input } from "../Input";
import { Select } from "../Select";
import { Textarea } from "../Textarea";
import { ProfileSave } from "./ProfileSave";
import { NativeFields } from "./NativeFields";
import { BillingFields } from "./BillingFields";
import type { ClaudeOperations } from "./operations";
type Change = (draft: ProviderDraft) => void;
function draftOf(record: ProviderRecord): ProviderDraft { const { id: _id, ...draft } = record.profile; return draft; }
function object(text: string, label: string): Record<string, unknown> {
  const parsed: unknown = JSON.parse(text);
  if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error(label + "必须是 JSON 对象");
  return parsed as Record<string, unknown>;
}
function ConnectionFields({ draft, change, busy, accounts }: { draft: ProviderDraft; change: Change; busy: boolean; accounts: ClaudeAccountView[] }) {
  const connection = draft.connection ?? {};
  const managed = usesClaudeManagedAuth(connection);
  const update = (patch: Partial<typeof connection>) => change({ ...draft, connection: { ...connection, ...patch } });
  const service = connection.authBinding?.authProvider ?? connection.providerType;
  return <>
    {managed ? <Select ariaLabel="Claude 档案账号绑定" value={connection.authBinding?.accountId ?? "default"} disabled={busy}
      options={[{ value: "default", label: "跟随此服务的默认账号" }, ...accounts.filter((a) => a.provider === service).map((a) => ({ value: a.id, label: a.label }))]}
      onChange={(value) => update({ authBinding: { source: "managed_account", authProvider: service, accountId: value === "default" ? null : value } })} />
      : <Select ariaLabel="Claude 上游认证方式" value={draft.authentication ?? "default"} disabled={busy}
        options={[{ value: "default", label: "按协议默认认证" }, { value: "bearer", label: "Bearer / ANTHROPIC_AUTH_TOKEN" },
          ...(draft.upstreamProtocol === "geminiGenerateContent" ? [{ value: "xGoogApiKey", label: "Google x-goog-api-key" }] : [{ value: "xApiKey", label: "x-api-key / ANTHROPIC_API_KEY" }])]}
        onChange={(value) => change({ ...draft, authentication: value === "default" ? null : value as ProviderDraft["authentication"] })} />}
    {!managed && <>
      <Checkbox label="服务地址为完整请求 URL" checked={connection.isFullUrl ?? false} disabled={busy} onChange={(value) => update({ isFullUrl: value })} />
      <label className="asb-field"><span>Claude 模型列表 URL</span><Input value={connection.claudeModelsUrl ?? ""} disabled={busy} placeholder="留空时按上游协议解析" onChange={(e) => update({ claudeModelsUrl: e.target.value || null })} /></label>
      <Checkbox label="自动选择已保存的候选端点" checked={connection.endpointAutoSelect !== false} disabled={busy} onChange={(value) => update({ endpointAutoSelect: value })} />
    </>}
    <label className="asb-field"><span>自定义 User-Agent</span><Input value={connection.customUserAgent ?? ""} disabled={busy} onChange={(e) => update({ customUserAgent: e.target.value || null })} /></label>
    <label className="asb-field"><span>Claude 提示缓存键</span><Input value={connection.claudePromptCacheKey ?? ""} disabled={busy || !["responses", "chatCompletions"].includes(draft.upstreamProtocol ?? "")} onChange={(e) => update({ claudePromptCacheKey: e.target.value || null })} /></label>
  </>;
}
export function ConnectionForm({ record, accounts, operations: op, onSaved }: {
  record: ProviderRecord; accounts: ClaudeAccountView[]; operations: ClaudeOperations; onSaved: () => Promise<void>;
}) {
  const [draft, setDraft] = useState(() => draftOf(record));
  const [headers, setHeaders] = useState(() => JSON.stringify(record.profile.connection?.localProxyRequestOverrides?.headers ?? {}, null, 2));
  const [body, setBody] = useState(() => JSON.stringify(record.profile.connection?.localProxyRequestOverrides?.body ?? {}, null, 2));
  const [fragment, setFragment] = useState(() => JSON.stringify(record.profile.claudeFragment ?? {}, null, 2));
  const [preparation, setPreparation] = useState<ProfileSavePreparation | null>(null);
  const busy = op.busy || !!preparation;
  if (draft.routeMode === "official") return <p className="asb-scope-note">官方 Claude 登录由 Claude Code 自己管理。本功能不会读写其原生凭据缓存。</p>;
  const changeNative = (next: ProviderDraft) => {
    if (next.connection?.claudeNative?.kind !== draft.connection?.claudeNative?.kind) { setHeaders("{}"); setBody("{}"); }
    setDraft(next);
  };
  const prepare = () => void op.run(async () => {
    const parsedFragment = object(fragment, "附加配置片段");
    const next = { ...draft, claudeFragment: parsedFragment };
    if (draft.connection?.claudeNative) { setPreparation(await prepareProfileSave(record.profile.id, next, record.fileHash)); return; }
    const parsedHeaders = object(headers, "请求头");
    if (Object.values(parsedHeaders).some((value) => typeof value !== "string")) throw new Error("请求头的所有值必须是字符串");
    const parsedBody = object(body, "请求体覆盖");
    setPreparation(await prepareProfileSave(record.profile.id, { ...next, connection: { ...next.connection, localProxyRequestOverrides: {
      headers: parsedHeaders as Record<string, string>, body: Object.keys(parsedBody).length ? parsedBody : null,
    } } }, record.fileHash));
  });
  return <div className="asb-provider-section-fields">
    <NativeFields draft={draft} change={changeNative} disabled={busy} />
    {!draft.connection?.claudeNative && <>
    <label className="asb-field"><span>HTTP 服务根地址</span><Input type="url" value={draft.baseUrl ?? ""} disabled={busy} onChange={(e) => setDraft({ ...draft, baseUrl: e.target.value || null })} /></label>
    {!usesClaudeManagedAuth(draft.connection) && <label className="asb-field"><span>HTTP API 密钥</span><Input type="password" value={draft.apiKey} disabled={busy} onChange={(e) => setDraft({ ...draft, apiKey: e.target.value })} /></label>}
    <ConnectionFields draft={draft} change={setDraft} busy={busy} accounts={accounts} />
    <BillingFields value={draft.connection?.claudeBilling ?? null} disabled={busy} onChange={(claudeBilling) => setDraft({ ...draft, connection: { ...draft.connection, claudeBilling } })} />
      <label className="asb-field"><span>请求头覆盖（JSON）</span><Textarea code value={headers} disabled={busy} onChange={(e) => setHeaders(e.target.value)} /></label>
      <label className="asb-field"><span>请求体覆盖（JSON）</span><Textarea code value={body} disabled={busy} onChange={(e) => setBody(e.target.value)} /></label>
      <p className="asb-scope-note">覆盖只属于此 Claude 供应商，不进入通用配置。认证头受保护；托管账号的服务端点不能被覆盖。</p>
      </>}
    <label className="asb-field"><span>Claude 设置附加片段（JSON）</span><Textarea code value={fragment} disabled={busy} onChange={(e) => setFragment(e.target.value)} /></label>
    <p className="asb-scope-note">片段只属于此档案：启用时叠加进 Claude 全局设置，切换或停用该档案时自动整段撤下；可视化参数、客户端偏好与扩展管理的键不能放进片段。</p>
    <Button variant="primary" disabled={busy} onClick={prepare}>预览保存 Claude 连接设置</Button>
    {preparation && <ProfileSave preparation={preparation} busy={op.busy} onCancel={() => setPreparation(null)} onConfirm={() => void op.run(async () => {
      await commitProfileSave(preparation.preparationId, true); setPreparation(null); await onSaved();
    })} />}
  </div>;
}
