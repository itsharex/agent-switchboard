import { beforeEach, expect, it, vi } from "vitest";
import {
  cancelProviderRequest,
  executeProviderRequest,
  fetchProviderRequestModels,
  prepareProviderRequest,
} from "./client";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { invoke } from "@tauri-apps/api/core";

const invokeMock = vi.mocked(invoke);

beforeEach(() => { invokeMock.mockReset(); });

it("prepares the persisted provider using only its stable id", async () => {
  const preparation = {
    requestId: "request-token",
    endpoint: "https://provider.example/v1/responses",
    upstreamProtocol: "responses",
    defaultModel: "provider-model",
    prompt: "请只回复：连接成功。",
  };
  invokeMock.mockResolvedValue(preparation);

  expect(await prepareProviderRequest({ kind: "saved", profileId: "provider-id" })).toBe(preparation);
  expect(invokeMock.mock.calls).toEqual([
    ["prepare_provider_request", { target: { kind: "saved", profileId: "provider-id" } }],
  ]);
});

it("lists the prepared connection's models without resending a target or credential", async () => {
  const models = [{ id: "loopback-model", ownedBy: null }];
  invokeMock.mockResolvedValue(models);

  expect(await fetchProviderRequestModels("request-token")).toBe(models);
  expect(invokeMock.mock.calls).toEqual([
    ["fetch_provider_request_models", { requestId: "request-token" }],
  ]);
});

it("executes the prepared request without resending an endpoint, prompt, or credential", async () => {
  const result = {
    outcome: "success",
    status: 200,
    latencyMs: 317,
    model: "returned-model",
    reply: "实际模型回复",
    error: null,
    diagnostic: null,
    at: "2026-09-07T12:00:00Z",
  };
  invokeMock.mockResolvedValue(result);

  expect(await executeProviderRequest("request-token", "temporary-model")).toBe(result);
  expect(invokeMock.mock.calls).toEqual([
    ["execute_provider_request", {
      request: { requestId: "request-token", model: "temporary-model" },
    }],
  ]);
});

it.each([true, false])("preserves the backend cancellation acknowledgement %s", async (acknowledged) => {
  invokeMock.mockResolvedValue(acknowledged);

  expect(await cancelProviderRequest("request-token")).toBe(acknowledged);
  expect(invokeMock.mock.calls).toEqual([
    ["cancel_provider_request", { requestId: "request-token" }],
  ]);
});

it.each([
  ["prepare", () => prepareProviderRequest({ kind: "saved", profileId: "provider-id" })],
  ["models", () => fetchProviderRequestModels("request-token")],
  ["execute", () => executeProviderRequest("request-token", "temporary-model")],
  ["cancel", () => cancelProviderRequest("request-token")],
] as const)("propagates %s command failures without inventing an outcome", async (_operation, run) => {
  const failure = { code: "provider-request-failed", message: "本机请求执行器不可用" };
  invokeMock.mockRejectedValue(failure);

  await expect(run()).rejects.toBe(failure);
  expect(invokeMock).toHaveBeenCalledTimes(1);
});
