import type { ProviderDiagnostic, ProviderDiagnosticKind, ProviderRequestTarget, ProviderRequestOutcome } from "../api/client";
import { PROTOCOL_LABELS } from "../lib/protocol";
import { Button } from "./Button";
import { Input } from "./Input";
import { ModelPicker } from "./ModelPicker";
import { Time } from "./Time";
import { CheckIcon, CloseIcon, RequestIcon } from "./icons";
import { useProviderRequest, type RequestView } from "./use-provider-request";

const OUTCOME_TITLES: Record<ProviderRequestOutcome, string> = {
  success: "已收到模型回复",
  authenticationFailed: "认证失败",
  rateLimited: "请求受限",
  httpError: "供应商拒绝了请求",
  networkError: "网络连接失败",
  timeout: "等待回复超时",
  invalidResponse: "未收到有效模型回复",
  cancelled: "请求已取消",
};

const DIAGNOSTIC_TITLES: Record<ProviderDiagnosticKind, string> = {
  dns: "DNS 解析失败",
  tls: "TLS 连接失败",
  network: "网络连接失败",
  timeout: "请求超时",
  websocketUnsupported: "供应商不支持 Responses WebSocket",
  endpoint: "API 路径错误",
  authentication: "认证失败",
  requestParameters: "请求参数错误",
  modelNotFound: "模型不存在",
  rateLimit: "请求受限",
  upstream: "供应商上游错误",
  responseParse: "响应解析失败",
  streamParse: "流式响应解析失败",
};

const PHASE_TITLES: Record<RequestView["phase"], string> = {
  preparing: "正在读取请求目标…",
  ready: "等待发送",
  sending: "正在等待模型回复…",
  cancelling: "正在取消请求…",
  complete: "请求结束",
  failed: "请求未完成",
  cancelled: "请求已取消",
};

function DiagnosticDetails({ diagnostic }: { diagnostic: ProviderDiagnostic }) {
  return (
    <div className="asb-request-diagnostic">
      <p>{diagnostic.message}</p>
      <dl>
        <div><dt>请求地址</dt><dd>{diagnostic.endpoint}</dd></div>
        <div><dt>传输方式</dt><dd>{diagnostic.transport === "websocket" ? "WebSocket" : "HTTP/SSE"}</dd></div>
        <div><dt>HTTP 状态</dt><dd>{diagnostic.status === null ? "未收到 HTTP 响应" : `HTTP ${diagnostic.status}`}</dd></div>
        <div><dt>Request ID</dt><dd>{diagnostic.requestId ?? "未返回"}</dd></div>
      </dl>
      {diagnostic.body !== null && <details>
        <summary>上游响应正文{diagnostic.bodyTruncated ? "（已截断）" : ""}</summary>
        <pre>{diagnostic.body || "（响应正文为空）"}</pre>
      </details>}
    </div>
  );
}

function RequestReceipt({ view }: { view: RequestView }) {
  const { phase, result, error } = view;
  const diagnostic = result?.diagnostic;
  const success = result?.outcome === "success";
  const cancelled = result?.outcome === "cancelled" || phase === "cancelled";
  const failure = phase === "failed" || (result !== null && !success && !cancelled);
  return (
    <div className="asb-request-receipt" data-phase={success ? "success" : failure ? "failure" : phase}
      role="status" aria-label="请求结果" aria-live="polite">
      <h4 className="asb-section-title">
        {success ? <CheckIcon /> : failure ? <CloseIcon /> : <RequestIcon />}
        {diagnostic ? DIAGNOSTIC_TITLES[diagnostic.kind] : result ? OUTCOME_TITLES[result.outcome] : PHASE_TITLES[phase]}
      </h4>
      {phase === "ready" && <p>收到有效模型回复后，才能确认本次请求可用。</p>}
      {phase === "sending" && <p>正在向供应商发送测试内容并等待回复。</p>}
      {phase === "cancelling" && <p>正在停止本次请求。</p>}
      {phase === "cancelled" && <p>已取消本次请求，供应商可能已产生用量。</p>}
      {success && <blockquote>{result.reply}</blockquote>}
      {diagnostic ? <DiagnosticDetails diagnostic={diagnostic} /> : result?.error && <p>{result.error}</p>}
      {result && <>
        <p className="asb-request-metrics">
          {!diagnostic && result.status !== null && <>HTTP {result.status} · </>}{result.latencyMs} ms
          {result.model !== null && <> · 模型 {result.model}</>}
        </p>
        <p><Time iso={result.at} /></p>
      </>}
      {error && <p role="alert">{error}</p>}
    </div>
  );
}

export function ProviderRequestPanel({ target, name }: { target: ProviderRequestTarget; name: string }) {
  const request = useProviderRequest(target);
  const { view, model, busy } = request;
  const preparation = view.preparation;
  const preparing = view.phase === "preparing";
  // A listing in flight owns the model field: the stale list must not stay
  // selectable while the backend is resolving the current connection.
  const modelsLocked = busy || request.modelsBusy;
  return (
    <section className="asb-request-panel" aria-label={`${name} 真实请求`}>
      {preparation && <dl className="asb-request-target">
        <div><dt>直达地址</dt><dd>{preparation.endpoint}</dd></div>
        <div><dt>API 格式</dt><dd>{PROTOCOL_LABELS[preparation.upstreamProtocol]}</dd></div>
      </dl>}
      <div className="asb-request-body">
        <div className="asb-request-composer">
          <div className="asb-request-field">
            <span>测试模型</span>
            <div className="asb-model-control">
              <Input code aria-label="测试模型" value={model} placeholder="填写模型 ID" disabled={modelsLocked || !preparation}
                onChange={(event) => request.setModel(event.target.value)} />
              {request.models && <ModelPicker models={request.models} current={model} ariaLabel="选择测试模型"
                disabled={modelsLocked || !preparation} onSelect={request.setModel} />}
              <div className="asb-model-actions">
                <Button variant="secondary" disabled={modelsLocked || !preparation} onClick={() => void request.fetchModels()}>
                  {request.modelsBusy ? "获取中…" : "获取模型"}
                </Button>
              </div>
            </div>
            {request.modelsError && <span className="asb-warn-text">{request.modelsError}</span>}
          </div>
          {preparation && <div className="asb-request-message"><span>测试内容</span><p>{preparation.prompt}</p></div>}
          <p className="asb-request-note">使用{target.kind === "draft" ? "当前草稿" : "已保存档案"}的地址与密钥直达供应商；本次验证不包含客户端切换或本机协议转换。</p>
        </div>
        <RequestReceipt view={view} />
      </div>
      <footer className="asb-request-footer">
        <p className="asb-request-note">单次模型请求可能产生少量费用。</p>
        {!preparation && !preparing ? (
          <Button variant="primary" onClick={request.reload}>重新读取</Button>
        ) : busy ? (
          <Button variant="secondary" disabled={view.phase === "cancelling"} onClick={request.cancel}>
            <CloseIcon />{view.phase === "cancelling" ? "正在取消…" : "取消请求"}
          </Button>
        ) : (
          <Button variant="primary" disabled={preparing || request.modelsBusy || !preparation || !model.trim()} onClick={request.send}>
            <RequestIcon />发送请求
          </Button>
        )}
      </footer>
    </section>
  );
}
