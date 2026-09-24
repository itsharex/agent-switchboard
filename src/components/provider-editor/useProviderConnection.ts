import { errorText, uiMessage } from "../../i18n/errors";
import { useMessageState } from "../../i18n/use-message-state";
import { useEffect, useRef, useState } from "react";
import { fetchProviderModels, getGatewayStatus, resolveProviderEndpoints,
  type AppKind, type ProviderEndpoints, type ProviderModel,
  type ProviderConnectionOptions, type ResponsesOptions, type UpstreamProtocol } from "../../api/client";
import { useI18n } from "../../i18n";
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
  const { t } = useI18n();
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
  const lead = input.app === "codex" ? t("providers.gateway.lead.official") : minimal ? t("providers.gateway.lead.minimal")
    : t("providers.gateway.lead.protocol", { client: clientName(input.app), protocol: PROTOCOL_LABELS[NATIVE_PROTOCOL[input.app]] });
  const action = minimal ? t("providers.gateway.actionMinimal")
    : input.upstreamProtocol === NATIVE_PROTOCOL[input.app] ? t("providers.gateway.actionNative") : t("providers.gateway.actionConverted");
  const tail = address
    ? t("providers.gateway.tail.routed", { address, action })
    : failed
      ? t("providers.gateway.tail.failed")
      : t("providers.gateway.tail.pending");
  return `${lead}${tail}`;
}

function useResolvedEndpoints(
  baseUrl: string,
  upstreamProtocol: UpstreamProtocol | null,
  connection: ProviderConnectionOptions | null | undefined,
) {
  const { t } = useI18n();
  const [resolution, setResolution] = useState<{
    baseUrl: string;
    upstreamProtocol: UpstreamProtocol;
    endpoints: ProviderEndpoints | null;
    error: unknown;
  } | null>(null);
  useEffect(() => {
    let active = true;
    setResolution(null);
    if (baseUrl && upstreamProtocol) {
      void resolveProviderEndpoints(baseUrl, upstreamProtocol, connection).then((endpoints) => {
        if (active) setResolution({ baseUrl, upstreamProtocol, endpoints, error: null });
      }).catch((caught: unknown) => {
        if (active) setResolution({ baseUrl, upstreamProtocol, endpoints: null, error: caught });
      });
    }
    return () => { active = false; };
  }, [baseUrl, upstreamProtocol, connection]);
  const current = resolution?.baseUrl === baseUrl && resolution.upstreamProtocol === upstreamProtocol
    ? resolution : null;
  return { endpoints: current?.endpoints ?? null, endpointError: current?.error == null ? null : errorText(current.error, t),
    resolvingEndpoint: Boolean(baseUrl && upstreamProtocol && !current) };
}

export function useProviderConnection(input: ProviderConnectionInput) {
  const { t } = useI18n();
  const [models, setModels] = useState<ProviderModel[] | null>(null);
  const [modelsBusy, setModelsBusy] = useState(false);
  const [modelsError, setModelsError] = useMessageState();
  const modelsVersion = useRef(0);
  const baseUrl = input.baseUrl?.trim() ?? "";
  const endpoints = useResolvedEndpoints(baseUrl, input.upstreamProtocol, input.connection);
  const gatewayRouteWarning = useGatewayWarning(input);
  const modelsEndpointError = input.connection?.isFullUrl && !input.connection.modelsUrl?.trim()
    ? t("providers.gateway.modelsUrlMissing")
    : endpoints.endpoints?.modelsError ?? null;
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
    if (modelsEndpointError) {
      setModelsError(input.connection?.isFullUrl && !input.connection.modelsUrl?.trim()
        ? uiMessage("providers.gateway.modelsUrlMissing") : modelsEndpointError);
      return null;
    }
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
        setModelsError(caught);
      }
      return null;
    } finally {
      if (modelsVersion.current === version) setModelsBusy(false);
    }
  };
  return { models, modelsBusy, modelsError, modelsEndpointError, baseUrl, gatewayRouteWarning, fetchModels, ...endpoints };
}
