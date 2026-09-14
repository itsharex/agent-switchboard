import { useEffect, useRef, useState } from "react";
import { fetchProviderModels, getGatewayStatus, resolveProviderEndpoints,
  type AppKind, type ProviderEndpoints, type ProviderModel,
  type ProviderConnectionOptions, type ResponsesOptions, type UpstreamProtocol } from "../../api/client";
import { clientName } from "../../lib/client-name";
import { NATIVE_PROTOCOL, PROTOCOL_LABELS, requiresGateway } from "../../lib/protocol";

/** The routing facts every editor needs; both provider contracts project onto it. */
export interface ProviderConnectionInput {
  app: AppKind;
  routeMode: "official" | "custom";
  baseUrl: string | null;
  connection?: ProviderConnectionOptions | null;
  apiKey: string;
  authentication?: import("../../api/shared").AuthenticationScheme | null;
  upstreamProtocol: UpstreamProtocol | null;
  responsesOptions: ResponsesOptions | null;
}

function useGatewayWarning(input: ProviderConnectionInput) {
  const [address, setAddress] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);
  const routed = requiresGateway(input);
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
  const minimal = input.upstreamProtocol === "responses" && input.responsesOptions?.requestMode === "minimal";
  const lead = input.app === "codex" ? "先完成官方登录；切换时保留登录，provider 统一为 openai。" : minimal ? "Responses 最小请求模式："
    : `与 ${clientName(input.app)} 原生协议（${PROTOCOL_LABELS[NATIVE_PROTOCOL[input.app]]}）不同：`;
  const action = minimal ? "按最小字段集" : input.upstreamProtocol === NATIVE_PROTOCOL[input.app] ? "通过 HTTP/SSE" : "转换为该协议后";
  return address
    ? `${lead}切换到该供应商时，客户端的服务地址会被改写为本机协议网关 ${address}（仅监听本机），请求由网关${action}转发到所填服务地址；请保持本应用运行，退出后第三方请求会停止，重新打开本应用可恢复网关。`
    : failed
      ? `${lead}此路径需要本机协议网关处理，但网关当前未在监听。请在“网关”页重试监听或修改端口，再切换到该供应商。`
      : `${lead}此路径需要本机协议网关处理，正在读取实际监听地址。`;
}

function useResolvedEndpoints(
  baseUrl: string,
  upstreamProtocol: UpstreamProtocol | null,
  connection: ProviderConnectionOptions | null | undefined,
) {
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
      void resolveProviderEndpoints(baseUrl, upstreamProtocol, connection).then((endpoints) => {
        if (active) setResolution({ baseUrl, upstreamProtocol, endpoints, error: null });
      }).catch((caught: unknown) => {
        const error = typeof caught === "object" && caught !== null && "message" in caught
          && typeof caught.message === "string" ? caught.message : "无法解析服务地址，请检查地址与 API 格式。";
        if (active) setResolution({ baseUrl, upstreamProtocol, endpoints: null, error });
      });
    }
    return () => { active = false; };
  }, [baseUrl, upstreamProtocol, connection]);
  const current = resolution?.baseUrl === baseUrl && resolution.upstreamProtocol === upstreamProtocol
    ? resolution : null;
  return { endpoints: current?.endpoints ?? null, endpointError: current?.error ?? null,
    resolvingEndpoint: Boolean(baseUrl && upstreamProtocol && !current) };
}

export function useProviderConnection(input: ProviderConnectionInput) {
  const [models, setModels] = useState<ProviderModel[] | null>(null);
  const [modelsBusy, setModelsBusy] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const modelsVersion = useRef(0);
  const baseUrl = input.baseUrl?.trim() ?? "";
  const endpoints = useResolvedEndpoints(baseUrl, input.upstreamProtocol, input.connection);
  const gatewayRouteWarning = useGatewayWarning(input);
  const modelAuthentication = input.authentication ?? undefined;
  const modelConnection = input.connection && Object.keys(input.connection).length > 0
    ? input.connection : undefined;
  useEffect(() => {
    modelsVersion.current += 1;
    setModels(null);
    setModelsError(null);
    setModelsBusy(false);
  }, [baseUrl, input.app, input.upstreamProtocol, input.apiKey, input.authentication, input.connection]);

  /** Fetches the upstream model list; resolves with the fetched models, or
   * null when the request was skipped or superseded. */
  const fetchModels = async (): Promise<ProviderModel[] | null> => {
    if (modelsBusy || !baseUrl || !input.upstreamProtocol) return null;
    const version = modelsVersion.current;
    setModelsBusy(true);
    setModelsError(null);
    try {
      const fetched = await fetchProviderModels(
        input.app,
        baseUrl,
        input.apiKey,
        input.upstreamProtocol,
        modelAuthentication,
        modelConnection,
      );
      if (modelsVersion.current === version) setModels(fetched);
      return modelsVersion.current === version ? fetched : null;
    } catch (caught) {
      if (modelsVersion.current === version) {
        setModelsError((caught as { message?: string }).message ?? "无法获取模型列表");
      }
      return null;
    } finally {
      if (modelsVersion.current === version) setModelsBusy(false);
    }
  };
  return { models, modelsBusy, modelsError, baseUrl, gatewayRouteWarning, fetchModels, ...endpoints };
}
