import { useState } from "react";
import type { CodexUpstream } from "../../api/client";
import type { ProviderConnectionOptions } from "../../api/providers";
import { clientName } from "../../lib/client-name";
import { requiresGateway } from "../../lib/protocol";
import { Input } from "../Input";
import { Select } from "../Select";
import { Textarea } from "../Textarea";
import { ProviderCredentialField } from "../provider-editor/ProviderCredentialField";
import { ProviderEndpointField } from "../provider-editor/ProviderEndpointField";
import { reconcileCodexUpstream } from "./draft";
import type { CodexEditorState } from "./useCodexProviderEditor";

interface Props { editor: CodexEditorState; busy: boolean }

function ProtocolField({ editor, busy }: Props) {
  const { draft, setDraft } = editor;
  return (
    <label className="asb-field">
      <span>API 格式</span>
      <Select ariaLabel="API 格式" value={draft.upstream} disabled={busy}
        options={[
          { value: "responses", label: "Responses (/responses)" },
          { value: "chatCompletions", label: "Chat Completions (/chat/completions)" },
          { value: "anthropicMessages", label: "Anthropic Messages (/v1/messages)" },
        ]}
        onChange={(value) => setDraft((current) =>
          reconcileCodexUpstream(current, value as CodexUpstream))} />
    </label>
  );
}

function OverrideFields({ editor, busy }: Props) {
  const { draft, setDraft } = editor;
  const overrides = draft.connection.localProxyRequestOverrides ?? null;
  const [headers, setHeaders] = useState(() =>
    JSON.stringify(overrides?.headers ?? {}, null, 2));
  const [body, setBody] = useState(() =>
    JSON.stringify(overrides?.body ?? {}, null, 2));
  const [problem, setProblem] = useState<string | null>(null);
  const update = (patch: Partial<ProviderConnectionOptions>) =>
    setDraft((current) => ({ ...current, connection: { ...current.connection, ...patch } }));
  const applyOverrides = (parsedHeaders: Record<string, string>, parsedBody: Record<string, unknown>) => {
    const emptyHeaders = Object.keys(parsedHeaders).length === 0;
    const emptyBody = Object.keys(parsedBody).length === 0;
    update({ localProxyRequestOverrides: emptyHeaders && emptyBody ? null : {
      headers: parsedHeaders,
      body: emptyBody ? null : parsedBody,
    } });
  };
  return (
    <div className="asb-provider-route-note">
      <label className="asb-field"><span>自定义 User-Agent</span>
        <Input value={draft.connection.customUserAgent ?? ""} disabled={busy}
          onChange={(e) => update({ customUserAgent: e.target.value || null })} /></label>
      <label className="asb-field"><span>请求头覆盖（JSON）</span>
        <Textarea code value={headers} disabled={busy}
          onChange={(e) => {
            setHeaders(e.target.value);
            try {
              const parsed = JSON.parse(e.target.value);
              if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)
                || Object.values(parsed).some((value) => typeof value !== "string")) {
                throw new Error("请求头必须是字符串到字符串的对象");
              }
              setProblem(null);
              applyOverrides(parsed, parseBody(body, setProblem));
            } catch (error) {
              setProblem(error instanceof Error ? error.message : String(error));
            }
          }} /></label>
      <label className="asb-field"><span>请求体覆盖（JSON 对象，顶层 stream 不可覆盖）</span>
        <Textarea code value={body} disabled={busy}
          onChange={(e) => {
            setBody(e.target.value);
            try {
              const parsed = parseBody(e.target.value, setProblem);
              setProblem(null);
              applyOverrides(parseHeaders(headers, setProblem), parsed);
            } catch (error) {
              setProblem(error instanceof Error ? error.message : String(error));
            }
          }} /></label>
      {problem && <p className="asb-scope-note asb-warn-text">{problem}</p>}
      <p className="asb-scope-note">
        覆盖仅在本地网关接管时生效；凭据、会话与追踪头始终由网关重新生成，保存时会拒绝替换它们的覆盖。
      </p>
    </div>
  );
}

function parseHeaders(raw: string, warn: (message: string) => void): Record<string, string> {
  const parsed = JSON.parse(raw);
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)
    || Object.values(parsed).some((value) => typeof value !== "string")) {
    const message = "请求头必须是字符串到字符串的对象";
    warn(message);
    throw new Error(message);
  }
  return parsed;
}

function parseBody(raw: string, warn: (message: string) => void): Record<string, unknown> {
  const text = raw.trim();
  if (text === "" || text === "{}" || text === "null") return {};
  const parsed: unknown = JSON.parse(text);
  if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
    const message = "请求体覆盖必须是 JSON 对象";
    warn(message);
    throw new Error(message);
  }
  return parsed as Record<string, unknown>;
}

export function CodexConnectionFields({ editor, busy }: Props) {
  const { draft, setDraft, connection } = editor;
  const gatewayRoute = requiresGateway({
    app: "codex",
    routeMode: "custom",
    upstreamProtocol: draft.upstream,
    responsesOptions: draft.upstream === "responses" ? { requestMode: draft.requestMode } : null,
    connection: draft.connection,
    authentication: draft.authentication,
  });
  return (
    <section className="asb-provider-section" aria-label="连接配置">
      <h3 className="asb-section-title">连接配置</h3>
      <div className="asb-provider-section-fields">
        <div className="asb-provider-connection-choice">
          <ProtocolField editor={editor} busy={busy} />
        </div>
        <ProviderEndpointField busy={busy} baseUrl={draft.endpoint} protocol={draft.upstream}
          endpoints={connection.endpoints} endpointError={connection.endpointError}
          resolvingEndpoint={connection.resolvingEndpoint}
          onChange={(value) => setDraft((current) => ({ ...current, endpoint: value }))} />
        <ProviderCredentialField busy={busy} value={draft.apiKey} protocol={draft.upstream}
          authentication={draft.authentication}
          onChange={(value) => setDraft((current) => ({ ...current, apiKey: value }))} />
        <label className="asb-field">
          <span>模型列表 URL</span>
          <Input type="url" value={draft.connection.modelsUrl ?? ""} disabled={busy}
            placeholder={draft.connection.isFullUrl ? "完整请求 URL 必填" : "（可选）"}
            onChange={(event) => setDraft((current) => ({
              ...current,
              connection: { ...current.connection, modelsUrl: event.target.value.trim() || null },
            }))} />
          <p className="asb-scope-note">仅用于“获取模型”。完整请求 URL 必须填写；不会改变实际请求地址。</p>
        </label>
        <div className="asb-provider-route-note">
          {gatewayRoute
            ? <p className="asb-scope-note asb-warn-text">{connection.gatewayRouteWarning}</p>
            : <p className="asb-scope-note">
              与 {clientName("codex")} 原生协议一致，切换后客户端直连所填服务地址。
            </p>}
          {draft.upstream !== "responses" && (
            <p className="asb-scope-note asb-warn-text">
              此路由下 Codex 的网页搜索会关闭，client_metadata、prompt_cache_key、reasoning.summary=auto 与 reasoning.encrypted_content 也不会转发到上游；需要这些能力请使用 Responses 上游。
            </p>
          )}
        </div>
        {gatewayRoute && <OverrideFields editor={editor} busy={busy} />}
      </div>
    </section>
  );
}
