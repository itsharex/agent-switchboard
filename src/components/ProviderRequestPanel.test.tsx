import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { executeProviderRequest, prepareProviderRequest, ProviderDiagnosticKind, ProviderProfile, ProviderRequestTarget } from "../api/client";
import { providerParameters } from "../test/provider-parameters";
import { ProviderList } from "./ProviderList";
import { ProviderRequestPanel } from "./ProviderRequestPanel";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { invoke } from "@tauri-apps/api/core";

type Preparation = Awaited<ReturnType<typeof prepareProviderRequest>>;
type RequestResult = Awaited<ReturnType<typeof executeProviderRequest>>;
const invokeMock = vi.mocked(invoke);
const profile: ProviderProfile = {
  id: "request-provider", app: "codex", routeMode: "custom", name: "请求供应商",
  baseUrl: "https://saved-provider.example/v1", model: "saved-model", apiKey: "test-private-key",
  websiteUrl: null, upstreamProtocol: "responses", responsesOptions: { requestMode: "standard" as const }, maxOutputTokens: null, modelOptions: null,
  parameters: providerParameters("codex"),
};
const preparation: Preparation = {
  requestId: "request-1", endpoint: "https://resolved-provider.example/v1/responses",
  upstreamProtocol: "responses", defaultModel: "backend-model", prompt: "测试上游收到后请回复当前状态。",
};
const success: RequestResult = {
  outcome: "success", status: 200, latencyMs: 317, model: "returned-model",
  reply: "模型确认收到本次请求。", error: null, diagnostic: null, at: "2026-09-07T12:00:00Z",
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((complete, fail) => { resolve = complete; reject = fail; });
  return { promise, resolve, reject };
}

function mockCommands(replies: {
  prepare?: () => Promise<Preparation>;
  execute?: () => Promise<RequestResult>;
  cancel?: () => Promise<boolean>;
  models?: () => Promise<{ id: string; ownedBy: string | null }[]>;
} = {}) {
  invokeMock.mockImplementation((command) => {
    if (command === "prepare_provider_request") return (replies.prepare?.() ?? Promise.resolve(preparation)) as never;
    if (command === "execute_provider_request") return (replies.execute?.() ?? Promise.resolve(success)) as never;
    if (command === "cancel_provider_request") return (replies.cancel?.() ?? Promise.resolve(true)) as never;
    if (command === "fetch_provider_request_models") {
      return (replies.models?.() ?? Promise.resolve([])) as never;
    }
    return Promise.reject(new Error(`Unexpected command: ${command}`)) as never;
  });
}

function callsFor(command: string) {
  return invokeMock.mock.calls.filter(([called]) => called === command);
}

function result() {
  return screen.getByRole("status", { name: "请求结果" });
}

async function ready() {
  await waitFor(() => expect(screen.getByRole("button", { name: "发送请求" })).toBeEnabled());
}

beforeEach(() => { invokeMock.mockReset(); });

it("opens the backend-resolved target beside connectivity and only sends on explicit action", async () => {
  mockCommands();
  const user = userEvent.setup();
  const select = vi.fn();
  render(<ProviderList profiles={[profile]} activeProfileId={null} userConfigModel={null}
    selectedId={null} onSelect={select} onSaveQuotaInterval={async () => true} />);
  expect(invokeMock).not.toHaveBeenCalled();
  expect(screen.getByRole("button", { name: "测试 请求供应商 供应商" })).toBeInTheDocument();

  const trigger = screen.getByRole("button", { name: "测试 请求供应商 供应商" });
  await user.click(trigger);
  expect(invokeMock).not.toHaveBeenCalled();
  await user.click(screen.getByRole("radio", { name: "真实请求" }));
  await ready();

  expect(trigger).toHaveAttribute("aria-expanded", "true");
  expect(screen.getByText(preparation.endpoint)).toBeInTheDocument();
  expect(screen.getByText(preparation.prompt)).toBeInTheDocument();
  expect(screen.getByRole("textbox", { name: "测试模型" })).toHaveValue(preparation.defaultModel);
  expect(screen.queryByText(/设计演示|演示结果|示例/)).not.toBeInTheDocument();
  expect(screen.queryByText(profile.apiKey)).not.toBeInTheDocument();
  expect(invokeMock.mock.calls).toEqual([["prepare_provider_request", { target: { kind: "saved", profileId: profile.id } }]]);
  expect(select).not.toHaveBeenCalled();

  await user.click(screen.getByRole("button", { name: "收起供应商测试" }));
  expect(trigger).toHaveFocus();
  expect(callsFor("cancel_provider_request")).toEqual([["cancel_provider_request", { requestId: "request-1" }]]);
  expect(callsFor("execute_provider_request")).toHaveLength(0);
});

it("renders actual response values and prepares a fresh token for a second send", async () => {
  const prepare = vi.fn().mockResolvedValueOnce(preparation)
    .mockResolvedValueOnce({ ...preparation, requestId: "request-2" });
  mockCommands({ prepare });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  fireEvent.change(screen.getByRole("textbox", { name: "测试模型" }), { target: { value: "temporary-model" } });
  await user.click(screen.getByRole("button", { name: "发送请求" }));

  await waitFor(() => expect(result()).toHaveTextContent(success.reply!));
  expect(result()).toHaveTextContent("200");
  expect(result()).toHaveTextContent(/317\s*(?:ms|毫秒)/);
  expect(result()).toHaveTextContent("returned-model");
  expect(callsFor("execute_provider_request")).toEqual([["execute_provider_request", {
    request: { requestId: "request-1", model: "temporary-model" },
  }]]);
  expect(JSON.stringify(invokeMock.mock.calls)).not.toContain(profile.apiKey);

  await user.click(screen.getByRole("button", { name: "发送请求" }));
  await waitFor(() => expect(callsFor("execute_provider_request")).toHaveLength(2));
  expect(callsFor("execute_provider_request")[1]).toEqual(["execute_provider_request", {
    request: { requestId: "request-2", model: "temporary-model" },
  }]);
  expect(prepare).toHaveBeenCalledTimes(2);
});

it("requires a nonblank temporary model when the backend has no default", async () => {
  mockCommands({ prepare: async () => ({ ...preparation, defaultModel: null }) });
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await screen.findByText(preparation.endpoint);
  const send = screen.getByRole("button", { name: "发送请求" });
  expect(send).toBeDisabled();
  fireEvent.change(screen.getByRole("textbox", { name: "测试模型" }), { target: { value: "   " } });
  expect(send).toBeDisabled();
  fireEvent.change(screen.getByRole("textbox", { name: "测试模型" }), { target: { value: "temporary-model" } });
  expect(send).toBeEnabled();
  expect(callsFor("execute_provider_request")).toHaveLength(0);
});

it("fetches the model list on demand and fills the field from the picker selection", async () => {
  const models = [
    { id: "loopback-model", ownedBy: "fixture-vendor" },
    { id: "bare-model", ownedBy: null },
  ];
  mockCommands({ models: async () => models });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  expect(screen.queryByRole("button", { name: "选择测试模型" })).not.toBeInTheDocument();

  await user.click(screen.getByRole("button", { name: "获取模型" }));
  expect(callsFor("fetch_provider_request_models")).toEqual([
    ["fetch_provider_request_models", { requestId: "request-1" }],
  ]);
  const picker = await screen.findByRole("button", { name: "选择测试模型" });
  await user.click(picker);
  expect(screen.getByRole("group", { name: "fixture-vendor" })).toBeInTheDocument();
  await user.click(screen.getByRole("option", { name: "loopback-model" }));

  expect(screen.getByRole("textbox", { name: "测试模型" })).toHaveValue("loopback-model");
  expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  await waitFor(() => expect(callsFor("execute_provider_request")).toEqual([["execute_provider_request", {
    request: { requestId: "request-1", model: "loopback-model" },
  }]]));
});

it("uses only an opaque token when listing models for an unsaved draft", async () => {
  const target: ProviderRequestTarget = {
    kind: "draft",
    connection: {
      baseUrl: "https://draft-provider.example/v1",
      apiKey: "draft-only-private-key",
      upstreamProtocol: "responses",
      responsesOptions: { requestMode: "standard" },
      defaultModel: "draft-model",
    },
  };
  mockCommands({ models: async () => [{ id: "draft-model", ownedBy: null }] });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={target} name="当前草稿" />);
  await ready();
  await user.click(screen.getByRole("button", { name: "获取模型" }));

  const listing = callsFor("fetch_provider_request_models");
  expect(listing).toEqual([["fetch_provider_request_models", { requestId: "request-1" }]]);
  expect(JSON.stringify(listing)).not.toContain(target.connection.apiKey);
});

it("locks the model controls while a listing is in flight and replaces the stale list only when it lands", async () => {
  const pending = deferred<{ id: string; ownedBy: string | null }[]>();
  const models = vi.fn()
    .mockResolvedValueOnce([{ id: "stale-model", ownedBy: null }])
    .mockImplementationOnce(() => pending.promise)
    .mockResolvedValueOnce([{ id: "refreshed-model", ownedBy: null }]);
  mockCommands({ models });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  await user.click(screen.getByRole("button", { name: "获取模型" }));
  expect(await screen.findByRole("button", { name: "选择测试模型" })).toBeEnabled();

  await user.click(screen.getByRole("button", { name: "获取模型" }));
  expect(screen.getByRole("button", { name: "获取中…" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "选择测试模型" })).toBeDisabled();
  expect(screen.getByRole("textbox", { name: "测试模型" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "发送请求" })).toBeDisabled();
  expect(callsFor("fetch_provider_request_models")).toEqual([
    ["fetch_provider_request_models", { requestId: "request-1" }],
    ["fetch_provider_request_models", { requestId: "request-1" }],
  ]);

  await act(async () => pending.resolve([{ id: "refreshed-model", ownedBy: null }]));
  await waitFor(() => expect(screen.getByRole("button", { name: "获取模型" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "选择测试模型" }));
  expect(screen.getByRole("option", { name: "refreshed-model" })).toBeInTheDocument();
  expect(screen.queryByRole("option", { name: "stale-model" })).not.toBeInTheDocument();
});

it("discards a model list that arrives after the request target changed", async () => {
  const pending = deferred<{ id: string; ownedBy: string | null }[]>();
  const next = { ...preparation, requestId: "request-2", endpoint: "https://new-target.example/v1/responses" };
  const prepare = vi.fn().mockResolvedValueOnce(preparation).mockResolvedValueOnce(next);
  mockCommands({ prepare, models: () => pending.promise });
  const user = userEvent.setup();
  const { rerender } = render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  await user.click(screen.getByRole("button", { name: "获取模型" }));
  expect(screen.getByRole("button", { name: "获取中…" })).toBeDisabled();

  rerender(<ProviderRequestPanel target={{ kind: "saved", profileId: "other-provider" }} name={profile.name} />);
  await screen.findByText(next.endpoint);
  await act(async () => pending.resolve([{ id: "old-target-model", ownedBy: null }]));

  await waitFor(() => expect(screen.getByRole("button", { name: "获取模型" })).toBeEnabled());
  expect(screen.queryByRole("button", { name: "选择测试模型" })).not.toBeInTheDocument();
  expect(callsFor("cancel_provider_request")).toContainEqual(["cancel_provider_request", { requestId: "request-1" }]);
});

it("prepares a fresh token for a listing after the previous request consumed it", async () => {
  const prepare = vi.fn().mockResolvedValueOnce(preparation)
    .mockResolvedValueOnce({ ...preparation, requestId: "request-2" });
  mockCommands({ prepare, models: async () => [{ id: "loopback-model", ownedBy: null }] });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  await waitFor(() => expect(result()).toHaveTextContent(success.reply!));

  await user.click(screen.getByRole("button", { name: "获取模型" }));
  await waitFor(() => expect(callsFor("fetch_provider_request_models")).toEqual([
    ["fetch_provider_request_models", { requestId: "request-2" }],
  ]));
  expect(prepare).toHaveBeenCalledTimes(2);
  expect(result()).toHaveTextContent(success.reply!);
});

it("keeps the request usable when the model list fails to load", async () => {
  mockCommands({ models: async () => { throw { code: "models-fetch-failed", message: "模型列表响应缺少 data 数组" }; } });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  await user.click(screen.getByRole("button", { name: "获取模型" }));

  expect(await screen.findByText("模型列表响应缺少 data 数组")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "选择测试模型" })).not.toBeInTheDocument();
  expect(screen.getByRole("textbox", { name: "测试模型" })).toHaveValue(preparation.defaultModel);
  expect(screen.getByRole("button", { name: "发送请求" })).toBeEnabled();
});

it("shows preparation failures and retries the backend lookup without executing a request", async () => {
  const prepare = vi.fn().mockRejectedValueOnce({ code: "profile-unavailable", message: "暂时无法读取该供应商档案" })
    .mockResolvedValueOnce(preparation);
  mockCommands({ prepare });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  expect(await screen.findByRole("alert")).toHaveTextContent("暂时无法读取该供应商档案");
  expect(screen.queryByText(profile.baseUrl!)).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "重新读取" }));

  await ready();
  expect(screen.getByText(preparation.endpoint)).toBeInTheDocument();
  expect(prepare).toHaveBeenCalledTimes(2);
  expect(callsFor("execute_provider_request")).toHaveLength(0);
});

it("requires another explicit send after a fresh preparation resolves a different endpoint", async () => {
  const next = { ...preparation, requestId: "request-2", endpoint: "https://new-target.example/v1/responses" };
  const prepare = vi.fn().mockResolvedValueOnce(preparation).mockResolvedValueOnce(next);
  mockCommands({ prepare });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  await waitFor(() => expect(result()).toHaveTextContent(success.reply!));
  await user.click(screen.getByRole("button", { name: "发送请求" }));

  await screen.findByText(next.endpoint);
  expect(callsFor("execute_provider_request")).toHaveLength(1);
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  expect(callsFor("execute_provider_request").at(-1)).toEqual(["execute_provider_request", {
    request: { requestId: "request-2", model: preparation.defaultModel },
  }]);
  expect(prepare).toHaveBeenCalledTimes(2);
});

it.each([
  { outcome: "authenticationFailed", status: 401, error: "API 密钥无效或已过期" },
  { outcome: "invalidResponse", status: 200, error: "上游响应没有模型回复" },
] as const)("shows $outcome as a failed request even when the endpoint answered", async (failure) => {
  mockCommands({ execute: async () => ({ ...success, ...failure, model: null, reply: null }) });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  await user.click(screen.getByRole("button", { name: "发送请求" }));

  await waitFor(() => expect(result()).toHaveTextContent(failure.error));
  expect(result()).toHaveTextContent(String(failure.status));
  expect(result()).not.toHaveTextContent(success.reply!);
  expect(screen.getByRole("button", { name: "发送请求" })).toBeEnabled();
});

it.each<[ProviderDiagnosticKind, string, number | null]>([
  ["dns", "DNS 解析失败", null], ["tls", "TLS 连接失败", null],
  ["network", "网络连接失败", null], ["timeout", "请求超时", null],
  ["websocketUnsupported", "供应商不支持 Responses WebSocket", 200],
  ["endpoint", "API 路径错误", 404], ["authentication", "认证失败", 401],
  ["requestParameters", "请求参数错误", 400], ["modelNotFound", "模型不存在", 404],
  ["rateLimit", "请求受限", 429], ["upstream", "供应商上游错误", 502],
  ["responseParse", "响应解析失败", 200], ["streamParse", "流式响应解析失败", 200],
])("shows the %s diagnosis and preserves the upstream receipt as plain text", async (kind, title, status) => {
  const diagnostic = {
    kind, endpoint: "https://provider.example/openai/responses",
    transport: kind === "websocketUnsupported" ? "websocket" as const : "http" as const,
    status, requestId: status === null ? null : "upstream-request-123",
    message: `具体原因：${title}`, bodyTruncated: status !== null,
    body: status === null ? null : '<html><script>unsafe()</script>Provider error: unsupported field</html>',
  };
  mockCommands({ execute: async () => ({
    ...success, outcome: "httpError", status, model: null, reply: null,
    error: "Unknown upstream error", diagnostic,
  }) });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  await screen.findByRole("heading", { name: title });
  expect(result()).toHaveTextContent(diagnostic.endpoint);
  expect(result()).toHaveTextContent(diagnostic.message);
  expect(result()).not.toHaveTextContent("Unknown upstream error");
  expect(result()).toHaveTextContent(status === null ? "未收到 HTTP 响应" : `HTTP ${status}`);
  expect(result()).toHaveTextContent(diagnostic.requestId ?? "未返回");
  if (diagnostic.body !== null) {
    await user.click(screen.getByText("上游响应正文（已截断）"));
    expect(result().querySelector("pre")?.textContent).toBe(diagnostic.body);
    expect(result().querySelector("script")).toBeNull();
  }
});

it("keeps cancellation retryable when the cancellation command fails", async () => {
  const pending = deferred<RequestResult>();
  const cancel = vi.fn().mockRejectedValueOnce({ code: "cancel-failed", message: "取消请求未送达执行器" })
    .mockResolvedValueOnce(true);
  mockCommands({ execute: () => pending.promise, cancel });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  await user.click(screen.getByRole("button", { name: "取消请求" }));

  expect(await screen.findByText(/取消请求未送达执行器/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "取消请求" })).toBeEnabled();
  await user.click(screen.getByRole("button", { name: "取消请求" }));
  await ready();
  expect(cancel).toHaveBeenCalledTimes(2);
  expect(callsFor("execute_provider_request")).toHaveLength(1);
  await act(async () => pending.resolve(success));
  expect(result()).not.toHaveTextContent(success.reply!);
});

it.each<RequestResult>([
  success,
  { ...success, outcome: "authenticationFailed", status: 401, model: null, reply: null, error: "供应商拒绝了当前密钥" },
])("waits for the actual $outcome receipt when cancellation returns false first", async (receipt) => {
  const pending = deferred<RequestResult>();
  mockCommands({ execute: () => pending.promise, cancel: async () => false });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  await user.click(screen.getByRole("button", { name: "取消请求" }));

  expect(result()).toHaveTextContent("正在等待模型回复");
  expect(result()).not.toHaveTextContent("请求已取消");
  expect(screen.getByRole("button", { name: "取消请求" })).toBeEnabled();
  expect(screen.queryByRole("button", { name: "发送请求" })).not.toBeInTheDocument();
  await act(async () => pending.resolve(receipt));
  expect(result()).toHaveTextContent(receipt.reply ?? receipt.error!);
  expect(result()).toHaveTextContent(String(receipt.status));
  expect(result()).not.toHaveTextContent("请求已取消");
  expect(screen.getByRole("button", { name: "发送请求" })).toBeEnabled();
});

it.each([true, false])("preserves a completed result when cancellation acknowledges %s afterwards", async (accepted) => {
  const pending = deferred<RequestResult>();
  const acknowledgement = deferred<boolean>();
  mockCommands({ execute: () => pending.promise, cancel: () => acknowledgement.promise });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  await user.click(screen.getByRole("button", { name: "取消请求" }));
  await act(async () => pending.resolve(success));

  expect(result()).toHaveTextContent(success.reply!);
  expect(screen.getByRole("button", { name: "发送请求" })).toBeEnabled();
  await act(async () => acknowledgement.resolve(accepted));
  expect(result()).toHaveTextContent(success.reply!);
  expect(result()).toHaveTextContent("returned-model");
  expect(result()).not.toHaveTextContent("请求已取消");
  expect(screen.getByRole("button", { name: "发送请求" })).toBeEnabled();
  expect(screen.queryByRole("button", { name: "取消请求" })).not.toBeInTheDocument();
});

it("ignores a cancelled request's late success after the next request completes", async () => {
  const pending = deferred<RequestResult>();
  const execute = vi.fn().mockImplementationOnce(() => pending.promise)
    .mockResolvedValueOnce({ ...success, reply: "第二次请求的新回复" });
  const prepare = vi.fn().mockResolvedValueOnce(preparation)
    .mockResolvedValueOnce({ ...preparation, requestId: "request-2" });
  mockCommands({ prepare, execute });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  await user.click(screen.getByRole("button", { name: "取消请求" }));
  await ready();
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  await waitFor(() => expect(result()).toHaveTextContent("第二次请求的新回复"));

  await act(async () => pending.resolve(success));
  expect(result()).toHaveTextContent("第二次请求的新回复");
  expect(result()).not.toHaveTextContent(success.reply!);
  expect(callsFor("cancel_provider_request")).toContainEqual(["cancel_provider_request", { requestId: "request-1" }]);
});

it("keeps the next request running and cancellable after an old cancelled request rejects", async () => {
  const first = deferred<RequestResult>();
  const second = deferred<RequestResult>();
  const execute = vi.fn().mockImplementationOnce(() => first.promise).mockImplementationOnce(() => second.promise);
  const prepare = vi.fn().mockResolvedValueOnce(preparation)
    .mockResolvedValueOnce({ ...preparation, requestId: "request-2" });
  mockCommands({ prepare, execute });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  await user.click(screen.getByRole("button", { name: "取消请求" }));
  await ready();
  await user.click(screen.getByRole("button", { name: "发送请求" }));

  await act(async () => first.reject({ code: "old-execution-failed", message: "上一轮请求迟到的失败" }));
  expect(result()).not.toHaveTextContent("上一轮请求迟到的失败");
  expect(screen.getByRole("button", { name: "取消请求" })).toBeEnabled();
  expect(callsFor("cancel_provider_request")).toHaveLength(1);
  await user.click(screen.getByRole("button", { name: "取消请求" }));
  expect(callsFor("cancel_provider_request").at(-1)).toEqual(["cancel_provider_request", { requestId: "request-2" }]);
  await ready();
  await act(async () => second.resolve(success));
  expect(result()).not.toHaveTextContent(success.reply!);
});

it.each<[string, Partial<ProviderProfile>]>([
  ["profile", { id: "other-provider" }],
  ["endpoint", { baseUrl: "https://changed-provider.example/v1" }],
  ["credential", { apiKey: "changed-test-private-key" }],
  ["protocol", { upstreamProtocol: "chatCompletions" }],
  ["Responses mode", { responsesOptions: { requestMode: "minimal" } }],
  ["model", { model: "changed-model" }],
])("cancels the old request when the %s changes and discards its late result", async (_field, change) => {
  const pending = deferred<RequestResult>();
  const next = { ...preparation, requestId: "request-2", endpoint: "https://changed-resolved.example/v1/messages" };
  const prepare = vi.fn().mockResolvedValueOnce(preparation).mockResolvedValueOnce(next);
  mockCommands({ prepare, execute: () => pending.promise });
  const user = userEvent.setup();
  const list = (value: ProviderProfile) => <ProviderList profiles={[value]} activeProfileId={null}
    userConfigModel={null} selectedId={null} onSelect={vi.fn()} onSaveQuotaInterval={async () => true} />;
  const { rerender } = render(list(profile));
  await user.click(screen.getByRole("button", { name: "测试 请求供应商 供应商" }));
  await user.click(screen.getByRole("radio", { name: "真实请求" }));
  await ready();
  await user.click(screen.getByRole("button", { name: "发送请求" }));

  rerender(list({ ...profile, ...change }));
  if (change.id) {
    await user.click(screen.getByRole("button", { name: "测试 请求供应商 供应商" }));
    await user.click(screen.getByRole("radio", { name: "真实请求" }));
  }
  await screen.findByText(next.endpoint);
  expect(callsFor("cancel_provider_request")).toContainEqual(["cancel_provider_request", { requestId: "request-1" }]);
  expect(callsFor("prepare_provider_request").at(-1)).toEqual(["prepare_provider_request", { target: { kind: "saved", profileId: change.id ?? profile.id } }]);
  await act(async () => pending.resolve(success));
  expect(result()).not.toHaveTextContent(success.reply!);
  expect(screen.getByText(next.endpoint)).toBeInTheDocument();
  expect(callsFor("execute_provider_request")).toHaveLength(1);
});

it("cancels the running backend request when its panel unmounts", async () => {
  const pending = deferred<RequestResult>();
  mockCommands({ execute: () => pending.promise });
  const user = userEvent.setup();
  const { unmount } = render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  unmount();

  expect(callsFor("cancel_provider_request")).toContainEqual(["cancel_provider_request", { requestId: "request-1" }]);
  await act(async () => pending.resolve(success));
  expect(screen.queryByRole("status", { name: "请求结果" })).not.toBeInTheDocument();
});

it("cancels a preparation that resolves after unmount without executing it", async () => {
  const pending = deferred<Preparation>();
  mockCommands({ prepare: () => pending.promise });
  const { unmount } = render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  unmount();
  expect(callsFor("cancel_provider_request")).toHaveLength(0);

  await act(async () => pending.resolve(preparation));
  expect(callsFor("cancel_provider_request")).toEqual([["cancel_provider_request", { requestId: "request-1" }]]);
  expect(callsFor("execute_provider_request")).toHaveLength(0);
});

it("discards a late preparation when another profile has already been prepared", async () => {
  const pending = deferred<Preparation>();
  const next = { ...preparation, requestId: "request-2", endpoint: "https://new-target.example/v1/responses" };
  const prepare = vi.fn().mockImplementationOnce(() => pending.promise).mockResolvedValueOnce(next);
  mockCommands({ prepare });
  const { rerender } = render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  rerender(<ProviderRequestPanel target={{ kind: "saved", profileId: "other-provider" }} name={profile.name} />);
  await screen.findByText(next.endpoint);

  await act(async () => pending.resolve(preparation));
  expect(callsFor("cancel_provider_request")).toContainEqual(["cancel_provider_request", { requestId: "request-1" }]);
  expect(screen.getByText(next.endpoint)).toBeInTheDocument();
  expect(screen.queryByText(preparation.endpoint)).not.toBeInTheDocument();
  expect(callsFor("execute_provider_request")).toHaveLength(0);
});

it("cancels a repeated send during preparation before any second execution starts", async () => {
  const pending = deferred<Preparation>();
  const prepare = vi.fn().mockResolvedValueOnce(preparation).mockImplementationOnce(() => pending.promise);
  mockCommands({ prepare });
  const user = userEvent.setup();
  render(<ProviderRequestPanel target={{ kind: "saved", profileId: profile.id }} name={profile.name} />);
  await ready();
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  await waitFor(() => expect(result()).toHaveTextContent(success.reply!));
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  await user.click(screen.getByRole("button", { name: "取消请求" }));

  await act(async () => pending.resolve({ ...preparation, requestId: "request-2" }));
  expect(callsFor("cancel_provider_request")).toContainEqual(["cancel_provider_request", { requestId: "request-2" }]);
  expect(callsFor("execute_provider_request")).toHaveLength(1);
  await ready();
});
