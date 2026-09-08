import { afterEach, beforeEach, vi } from "vitest";
import type { ComponentProps } from "react";
import * as client from "../api/client";
import { ProviderEditor as ProviderEditorComponent } from "../components/ProviderEditor";
import { providerParametersCatalog } from "./provider-parameters";

beforeEach(() => {
  vi.spyOn(client, "getProviderParametersCatalog").mockImplementation(async (app) => providerParametersCatalog(app));
  vi.spyOn(client, "resolveProviderEndpoints").mockResolvedValue({
    requestUrl: "https://resolved-provider.example/api/responses", modelsUrl: "https://resolved-provider.example/api/models",
  });
});
afterEach(() => vi.restoreAllMocks());

export function gatewayStatus(port: number) {
  return { port, baseUrl: `http://127.0.0.1:${port}`, routes: [],
    metrics: { startedAtMs: 0, totalRequests: 0, failedRequests: 0, samples: [] } };
}

export function ProviderEditor({ active = true, userConfigWarnings = [], onOpenOfficial = vi.fn(), ...props }:
  Omit<ComponentProps<typeof ProviderEditorComponent>, "active" | "userConfigWarnings" | "onOpenOfficial"> & {
    active?: boolean;
    userConfigWarnings?: string[];
    onOpenOfficial?: (app: "codex" | "claude") => void;
  }) {
  return <ProviderEditorComponent {...props} active={active} userConfigWarnings={userConfigWarnings} onOpenOfficial={onOpenOfficial} />;
}
