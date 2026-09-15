import { invoke } from "./client";
export type OutboundProxyMode = "none" | "system" | "manual";
export interface OutboundProxySettings {
  version: number;
  mode: OutboundProxyMode;
  url?: string | null;
  revision: string;
}
export interface OutboundProxyTestResult {
  success: boolean;
  latencyMs: number;
  error: string | null;
}
export interface DetectedOutboundProxy {
  url: string;
  proxyType: string;
  port: number;
}
export const getOutboundProxy = (): Promise<OutboundProxySettings> => invoke("get_outbound_proxy");
export const setOutboundProxy = (settings: OutboundProxySettings, expectedRevision: string,
  confirmWrite: boolean): Promise<OutboundProxySettings> =>
  invoke("set_outbound_proxy", { settings, expectedRevision, confirmWrite });
export const testOutboundProxy = (url: string): Promise<OutboundProxyTestResult> =>
  invoke("test_outbound_proxy", { url });
export const scanLocalOutboundProxies = (): Promise<DetectedOutboundProxy[]> =>
  invoke("scan_local_outbound_proxies");
