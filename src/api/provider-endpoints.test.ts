import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  fetchProviderModels,
  resolveProviderEndpoints,
  testUsageQuery,
  type AuthenticationScheme,
  type UpstreamProtocol,
  type UsageQuery,
} from "./client";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);
beforeEach(() => { invokeMock.mockReset(); });

it("delegates the API root unchanged to the backend and preserves its resolved endpoints", async () => {
  const result = { requestUrl: "https://upstream.example/openai/responses", modelsUrl: "https://upstream.example/openai/models" };
  invokeMock.mockResolvedValue(result);
  expect(await resolveProviderEndpoints("https://upstream.example/openai", "responses")).toBe(result);
  expect(invokeMock.mock.calls).toEqual([["resolve_provider_endpoints", {
    request: { baseUrl: "https://upstream.example/openai", upstreamProtocol: "responses" },
  }]]);
});

it("preserves resolver failures without inventing another URL", async () => {
  const error = { code: "provider-endpoint-invalid", message: "请填写 API 根地址，而非完整请求地址" };
  invokeMock.mockRejectedValue(error);
  await expect(resolveProviderEndpoints("https://upstream.example/openai/responses", "responses")).rejects.toBe(error);
  expect(invokeMock).toHaveBeenCalledTimes(1);
});

const protocols: UpstreamProtocol[] = ["anthropicMessages", "chatCompletions", "responses"];
const authSchemes: AuthenticationScheme[] = ["bearer", "xApiKey"];
const usageQuery: UsageQuery = {
  kind: "declarative", url: "{{baseUrl}}/usage", remainingPath: "/remaining",
  refreshIntervalMinutes: 0,
};
const connection = {
  customUserAgent: "Usage Fixture",
  endpointAutoSelect: false,
};

it.each(protocols.flatMap((protocol) => authSchemes.map((authentication) => ({ protocol, authentication }))))(
  "forwards explicit $authentication independently from $protocol for model and usage queries",
  async ({ protocol, authentication }) => {
    const baseUrl = "http://127.0.0.1:9/provider";
    const apiKey = "query-auth-fixture-key";
    const models = [{ id: "fixture-model", ownedBy: null }];
    const usage = { readings: [], at: "2026-09-12T00:00:00Z" };
    invokeMock.mockResolvedValueOnce(models).mockResolvedValueOnce(usage);

    expect(await fetchProviderModels("claude", baseUrl, apiKey, protocol, authentication)).toBe(models);
    expect(await testUsageQuery(usageQuery, apiKey, baseUrl, protocol, authentication, connection)).toBe(usage);
    expect(invokeMock.mock.calls).toStrictEqual([
      ["fetch_provider_models", { request: { app: "claude", url: baseUrl, apiKey, upstreamProtocol: protocol, authentication } }],
      ["test_usage_query", { request: { query: usageQuery, apiKey, baseUrl, upstreamProtocol: protocol, authentication, connection } }],
    ]);
  },
);

it.each(protocols.flatMap((protocol) => [null, undefined].map((authentication) => ({ protocol, authentication }))))(
  "keeps $protocol query defaults when authentication is $authentication",
  async ({ protocol, authentication }) => {
    const baseUrl = "http://127.0.0.1:9/v1";
    const apiKey = "default-auth-fixture-key";
    invokeMock.mockResolvedValue({});
    await fetchProviderModels("claude", baseUrl, apiKey, protocol, authentication);
    await testUsageQuery(usageQuery, apiKey, baseUrl, protocol, authentication);

    expect(invokeMock.mock.calls).toStrictEqual([
      ["fetch_provider_models", { request: { app: "claude", url: baseUrl, apiKey, upstreamProtocol: protocol } }],
      ["test_usage_query", { request: { query: usageQuery, apiKey, baseUrl, upstreamProtocol: protocol } }],
    ]);
  },
);

it("keeps existing callers without an authentication argument compatible", async () => {
  invokeMock.mockResolvedValue({});
  await fetchProviderModels("claude", "http://127.0.0.1:9", "fixture-key", "responses");
  await testUsageQuery(usageQuery, "fixture-key", null, "anthropicMessages");
  expect(invokeMock.mock.calls[0]).toStrictEqual(["fetch_provider_models", {
    request: { app: "claude", url: "http://127.0.0.1:9", apiKey: "fixture-key", upstreamProtocol: "responses" },
  }]);
  expect(invokeMock.mock.calls[1]).toStrictEqual(["test_usage_query", {
    request: { query: usageQuery, apiKey: "fixture-key", baseUrl: null, upstreamProtocol: "anthropicMessages" },
  }]);
});
