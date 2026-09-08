import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { ProviderProfile } from "../api/client";
import { ProviderEditor, gatewayStatus } from "../test/provider-editor";
import { providerParameters } from "../test/provider-parameters";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const profile: ProviderProfile = {
  id: "responses-provider", app: "codex", routeMode: "custom", name: "Responses 供应商",
  model: "provider-model", baseUrl: "https://relay.example/openai", apiKey: "test-key",
  upstreamProtocol: "responses", responsesOptions: { requestMode: "standard" },
  maxOutputTokens: null, modelOptions: null, parameters: providerParameters("codex"), websiteUrl: null,
};
const props = { initialApp: "codex" as const, busy: false, officialTakenApps: [],
  userConfigModel: null, onCancel: vi.fn() };

beforeEach(() => vi.mocked(invoke).mockResolvedValue(gatewayStatus(31819)));


async function openRequests(user: ReturnType<typeof userEvent.setup>) {
  const summary = screen.getByText("Responses 能力", { selector: "summary > span" }).closest("summary")!;
  if (!summary.parentElement?.hasAttribute("open")) await user.click(summary);
}

it("creates request options with an always-on gateway and no login or WebSocket toggle", async () => {
  const onSave = vi.fn();
  const user = userEvent.setup();
  render(<ProviderEditor {...props} profile={null} onSave={onSave} />);
  await openRequests(user);
  expect(screen.queryByRole("checkbox", { name: /WebSocket|保留登录/ })).not.toBeInTheDocument();
  expect(screen.getByRole("combobox", { name: "请求模式" })).toHaveTextContent("标准请求");
  expect(await screen.findByText(/先完成官方登录/)).toHaveTextContent("provider 统一为 openai");
  expect(screen.getByText(/不会自动补 \/v1/)).toHaveTextContent("https://example.com/v1/responses");
  fireEvent.change(screen.getByLabelText("名称"), { target: { value: profile.name } });
  fireEvent.change(screen.getByLabelText("服务地址"), { target: { value: profile.baseUrl } });
  fireEvent.change(screen.getByLabelText("API 密钥"), { target: { value: profile.apiKey } });
  await waitFor(() => expect(screen.getByRole("button", { name: "保存供应商" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "保存供应商" }));
  expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
    baseUrl: "https://relay.example/openai", responsesOptions: { requestMode: "standard" },
  }));
});

it("keeps standard and minimal Responses on the gateway and persists only request mode", async () => {
  const onSave = vi.fn();
  const user = userEvent.setup();
  render(<ProviderEditor {...props} profile={profile} onSave={onSave} />);
  await openRequests(user);
  await user.click(screen.getByRole("combobox", { name: "请求模式" }));
  await user.click(screen.getByRole("option", { name: "最小请求" }));
  expect(await screen.findByText(/本机协议网关 http:\/\/127\.0\.0\.1:31819/)).toHaveTextContent("请保持本应用运行");
  expect(screen.getByText(/保留输入与工具语义/)).toHaveTextContent("previous_response_id");
  await user.click(screen.getByRole("button", { name: "保存供应商" }));
  expect(onSave).toHaveBeenLastCalledWith(expect.objectContaining({ responsesOptions: { requestMode: "minimal" } }));
  await user.click(screen.getByRole("combobox", { name: "请求模式" }));
  await user.click(screen.getByRole("option", { name: "标准请求" }));
  expect(screen.queryByText(/切换后客户端直连所填服务地址/)).not.toBeInTheDocument();
  expect(screen.queryByRole("checkbox", { name: /WebSocket/ })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "保存供应商" }));
  expect(onSave).toHaveBeenLastCalledWith(expect.objectContaining({ responsesOptions: { requestMode: "standard" } }));
});

it("clears request options on protocol changes and creates fresh standard options", async () => {
  const onSave = vi.fn();
  const user = userEvent.setup();
  render(<ProviderEditor {...props} profile={profile} onSave={onSave} />);
  await user.click(screen.getByRole("combobox", { name: "API 格式" }));
  await user.click(screen.getByRole("option", { name: /Chat Completions/ }));
  expect(screen.queryByRole("combobox", { name: "请求模式" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "保存供应商" }));
  expect(onSave).toHaveBeenLastCalledWith(expect.objectContaining({ upstreamProtocol: "chatCompletions", responsesOptions: null }));
  await user.click(screen.getByRole("combobox", { name: "API 格式" }));
  await user.click(screen.getByRole("option", { name: /Responses/ }));
  await user.click(screen.getByRole("button", { name: "保存供应商" }));
  expect(onSave).toHaveBeenLastCalledWith(expect.objectContaining({ responsesOptions: { requestMode: "standard" } }));
});

it("clears request choices on an official-login transition", async () => {
  const user = userEvent.setup();
  render(<ProviderEditor {...props} profile={null} onSave={vi.fn()} />);
  await openRequests(user);
  await user.click(screen.getByRole("combobox", { name: "请求模式" }));
  await user.click(screen.getByRole("option", { name: "最小请求" }));
  await user.click(screen.getByRole("radio", { name: "官方登录" }));
  expect(screen.queryByRole("combobox", { name: "请求模式" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("radio", { name: "自定义 API 中继" }));
  await openRequests(user);
  expect(screen.getByRole("combobox", { name: "请求模式" })).toHaveTextContent("标准请求");
});

it("does not fill missing request options or accept retired fields", () => {
  for (const responsesOptions of [undefined, { requestMode: "standard", supportsWebsockets: false }]) {
    const onSave = vi.fn();
    const rendered = render(<ProviderEditor {...props} profile={{ ...profile, responsesOptions } as ProviderProfile} onSave={onSave} />);
    expect(screen.getByRole("button", { name: "保存供应商" })).toBeDisabled();
    fireEvent.submit(screen.getByRole("form", { name: "编辑供应商" }));
    expect(onSave).not.toHaveBeenCalled();
    rendered.unmount();
  }
});
