import { useEffect, useRef, useState } from "react";
import { fetchProviderModels, getGatewayStatus, resolveProviderEndpoints,
  type ProviderEndpoints, type ProviderModel, type UpstreamProtocol } from "../../api/client";
import { clientName } from "../../lib/client-name";
import { NATIVE_PROTOCOL, PROTOCOL_LABELS, requiresGateway } from "../../lib/protocol";
import type { ProviderEditorDraft } from "./draft";

function useGatewayWarning(draft: ProviderEditorDraft) {
  const [address, setAddress] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const routed = requiresGateway(draft);
  useEffect(() => {
    let active = true;
    setAddress(null);
    setFailed(false);
    if (routed) {
      void getGatewayStatus().then((status) => {
        if (!active) return;
        setAddress(status.baseUrl);
        setFailed(status.baseUrl === null);
      }).catch(() => { if (active) setFailed(true); });
    }
    return () => { active = false; };
  }, [routed]);
  const minimal = draft.upstreamProtocol === "responses" && draft.responsesOptions?.requestMode === "minimal";
  const lead = draft.app === "codex" ? "先完成官方登录；切换时保留登录，provider 统一为 openai。" : minimal ? "Responses 最小请求模式："
    : `与 ${clientName(draft.app)} 原生协议（${PROTOCOL_LABELS[NATIVE_PROTOCOL[draft.app]]}）不同：`;
  const action = minimal ? "按最小字段集" : draft.upstreamProtocol === NATIVE_PROTOCOL[draft.app] ? "通过 HTTP/SSE" : "转换为该协议后";
  return address
    ? `${lead}切换到该供应商时，客户端的服务地址会被改写为本机协议网关 ${address}（仅监听本机），请求由网关${action}转发到所填服务地址；请保持本应用运行，退出后第三方请求会停止，重新打开本应用可恢复网关。`
    : failed
      ? `${lead}此路径需要本机协议网关处理，但网关当前未在监听。请在“网关”页重试监听或修改端口，再切换到该供应商。`
      : `${lead}此路径需要本机协议网关处理，正在读取实际监听地址。`;
}

function useResolvedEndpoints(baseUrl: string, upstreamProtocol: UpstreamProtocol | null) {
  const [resolution, setResolution] = useState<{
    baseUrl: string;
    upstreamProtocol: UpstreamProtocol;
    endpoints: ProviderEndpoints | null;
    error: string | null;
  } | null>(null);
  useEffect(() => {
    let active = true;
    setResolution(null);
    if (baseUrl && upstreamProtocol) {
      void resolveProviderEndpoints(baseUrl, upstreamProtocol).then((endpoints) => {
        if (active) setResolution({ baseUrl, upstreamProtocol, endpoints, error: null });
      }).catch((caught: unknown) => {
        const error = typeof caught === "object" && caught !== null && "message" in caught
          && typeof caught.message === "string" ? caught.message : "无法解析服务地址，请检查地址与 API 格式。";
        if (active) setResolution({ baseUrl, upstreamProtocol, endpoints: null, error });
      });
    }
    return () => { active = false; };
  }, [baseUrl, upstreamProtocol]);
  const current = resolution?.baseUrl === baseUrl && resolution.upstreamProtocol === upstreamProtocol
    ? resolution : null;
  return { endpoints: current?.endpoints ?? null, endpointError: current?.error ?? null,
    resolvingEndpoint: Boolean(baseUrl && upstreamProtocol && !current) };
}

export function useProviderConnection(draft: ProviderEditorDraft) {
  const [models, setModels] = useState<ProviderModel[] | null>(null);
  const [modelsBusy, setModelsBusy] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const modelsVersion = useRef(0);
  const baseUrl = draft.baseUrl?.trim() ?? "";
  const endpoints = useResolvedEndpoints(baseUrl, draft.upstreamProtocol);
  const gatewayRouteWarning = useGatewayWarning(draft);
  useEffect(() => {
    modelsVersion.current += 1;
    setModels(null);
    setModelsError(null);
    setModelsBusy(false);
  }, [baseUrl, draft.upstreamProtocol]);

  const fetchModels = async () => {
    if (modelsBusy || !baseUrl || !draft.upstreamProtocol) return;
    const version = modelsVersion.current;
    setModelsBusy(true);
    setModelsError(null);
    try {
      const fetched = await fetchProviderModels(baseUrl, draft.apiKey, draft.upstreamProtocol);
      if (modelsVersion.current === version) setModels(fetched);
    } catch (caught) {
      if (modelsVersion.current === version) {
        setModelsError((caught as { message?: string }).message ?? "无法获取模型列表");
      }
    } finally {
      if (modelsVersion.current === version) setModelsBusy(false);
    }
  };
  return { models, modelsBusy, modelsError, baseUrl, gatewayRouteWarning, fetchModels, ...endpoints };
}
