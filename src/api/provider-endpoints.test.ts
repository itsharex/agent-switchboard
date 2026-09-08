import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { resolveProviderEndpoints } from "./client";

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
