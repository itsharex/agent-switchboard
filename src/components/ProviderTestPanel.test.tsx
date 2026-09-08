import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { ProviderTestPanel } from "./ProviderTestPanel";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { invoke } from "@tauri-apps/api/core";
const invokeMock = vi.mocked(invoke);
const props = { id: "test", name: "测试档案", url: "https://relay.example/v1",
  target: { kind: "saved" as const, profileId: "saved" }, onClose: vi.fn() };
const reachable = { grade: "ok", status: 204, latencyMs: 320, error: null, at: "2026-09-08T05:00:00Z" };

beforeEach(() => invokeMock.mockReset());

it("opens without requests and disables probing until an address is available", () => {
  render(<ProviderTestPanel {...props} url={null} />);
  expect(screen.getByRole("button", { name: "开始检测" })).toBeDisabled();
  expect(invokeMock).not.toHaveBeenCalled();
});

it.each([
  [reachable, "连通正常 · HTTP 204 · 320 毫秒"],
  [{ ...reachable, grade: "slow", latencyMs: 7400 }, "连通但较慢 · HTTP 204 · 7400 毫秒"],
  [{ ...reachable, grade: "unreachable", error: "连接超时" }, "无法连通 · 连接超时"],
])("reports the actual reachability result without credentials", async (reply, text) => {
  invokeMock.mockResolvedValue(reply);
  const user = userEvent.setup();
  render(<ProviderTestPanel {...props} />);
  await user.click(screen.getByRole("button", { name: "开始检测" }));
  expect(await screen.findByText(new RegExp(text))).toBeInTheDocument();
  expect(invokeMock.mock.calls).toEqual([["probe_endpoint", { url: props.url }]]);
});

it("discards a late probe after the edited address changes", async () => {
  let resolve!: (value: unknown) => void;
  invokeMock.mockReturnValue(new Promise((done) => { resolve = done; }));
  const user = userEvent.setup();
  const { rerender } = render(<ProviderTestPanel {...props} />);
  await user.click(screen.getByRole("button", { name: "开始检测" }));
  expect(screen.getByRole("button", { name: "检测中…" })).toBeDisabled();
  rerender(<ProviderTestPanel {...props} url="https://new.example" />);
  await act(async () => resolve(reachable));
  expect(screen.queryByText(/连通正常/)).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "开始检测" })).toBeEnabled();
});

it("releases a real request when switching back to connectivity", async () => {
  invokeMock.mockImplementation(async (command) => command === "prepare_provider_request" ? {
    requestId: "prepared", endpoint: "https://relay.example/v1/responses", upstreamProtocol: "responses",
    defaultModel: "test-model", prompt: "测试内容",
  } : true);
  const user = userEvent.setup();
  render(<ProviderTestPanel {...props} />);
  await user.click(screen.getByRole("radio", { name: "真实请求" }));
  await screen.findByDisplayValue("test-model");
  await user.click(screen.getByRole("radio", { name: "连通性测试" }));
  expect(invokeMock.mock.calls).toEqual([
    ["prepare_provider_request", { target: props.target }],
    ["cancel_provider_request", { requestId: "prepared" }],
  ]);
  expect(screen.queryByRole("textbox", { name: "测试模型" })).not.toBeInTheDocument();
});
