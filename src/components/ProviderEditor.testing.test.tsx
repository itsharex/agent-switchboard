import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { ProviderEditor } from "../test/provider-editor";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
import { invoke } from "@tauri-apps/api/core";
const invokeMock = vi.mocked(invoke);
const props = { profile: null, initialApp: "claude" as const, busy: false,
  officialTakenApps: [], userConfigModel: null, onSave: vi.fn(), onCancel: vi.fn() };
const commands = (name: string) => invokeMock.mock.calls.filter(([command]) => command === name);

async function fillConnection(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("combobox", { name: "API 格式" }));
  await user.click(await screen.findByRole("option", { name: /Responses/ }));
  fireEvent.change(screen.getByLabelText("服务地址"), { target: { value: "https://draft.example/v1" } });
  fireEvent.change(screen.getByLabelText("API 密钥"), { target: { value: "draft-test-key" } });
  fireEvent.change(screen.getByLabelText("主模型"), { target: { value: "draft-model" } });
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command, args) => {
    if (command === "prepare_provider_request") {
      const connection = (args as { target: { connection: { baseUrl: string; defaultModel: string } } }).target.connection;
      return { requestId: `token-${commands(command).length}`, endpoint: `${connection.baseUrl}/responses`,
        upstreamProtocol: "responses", defaultModel: connection.defaultModel, prompt: "连接测试" };
    }
    if (command === "execute_provider_request") return { outcome: "success", reply: "草稿连接成功",
      status: 200, latencyMs: 10, model: "draft-model", diagnostic: null, error: null, at: "2026-09-08T05:00:00Z" };
    if (command === "fetch_provider_request_models") return [{ id: "draft-model", ownedBy: null }];
    if (command === "cancel_provider_request") return true;
    throw new Error(`Unexpected command ${command}`);
  });
});

it("tests an unnamed unsaved draft using its current connection without saving", async () => {
  const user = userEvent.setup();
  render(<ProviderEditor {...props} />);
  await fillConnection(user);
  await user.click(screen.getByRole("button", { name: "测试供应商" }));
  expect(commands("prepare_provider_request")).toHaveLength(0);
  await user.click(screen.getByRole("radio", { name: "真实请求" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "发送请求" })).toBeEnabled());
  expect(commands("prepare_provider_request")).toEqual([["prepare_provider_request", { target: {
    kind: "draft", connection: { app: "claude", baseUrl: "https://draft.example/v1", apiKey: "draft-test-key", connection: {},
      upstreamProtocol: "responses", responsesOptions: { requestMode: "standard" }, defaultModel: "draft-model" },
  } }]]);
  expect(commands("execute_provider_request")).toHaveLength(0);
  await user.click(screen.getByRole("button", { name: "发送请求" }));
  await screen.findByText("草稿连接成功");
  expect(props.onSave).not.toHaveBeenCalled();
});

it("lists draft models through the preparation token instead of resending the key", async () => {
  const user = userEvent.setup();
  render(<ProviderEditor {...props} />);
  await fillConnection(user);
  await user.click(screen.getByRole("button", { name: "测试供应商" }));
  await user.click(screen.getByRole("radio", { name: "真实请求" }));
  const panel = within(screen.getByRole("region", { name: "当前草稿 真实请求" }));
  await waitFor(() => expect(panel.getByRole("button", { name: "获取模型" })).toBeEnabled());

  await user.click(panel.getByRole("button", { name: "获取模型" }));
  expect(commands("fetch_provider_request_models")).toEqual([
    ["fetch_provider_request_models", { requestId: "token-1" }],
  ]);
  expect(JSON.stringify(commands("fetch_provider_request_models"))).not.toContain("draft-test-key");
  await waitFor(() => expect(screen.getByRole("button", { name: "选择测试模型" })).toBeEnabled());
});

it("replaces a draft preparation after connection edits and releases it when the editor is hidden", async () => {
  const user = userEvent.setup();
  const { rerender } = render(<ProviderEditor {...props} active />);
  await fillConnection(user);
  await user.click(screen.getByRole("button", { name: "测试供应商" }));
  await user.click(screen.getByRole("radio", { name: "真实请求" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "发送请求" })).toBeEnabled());
  fireEvent.change(screen.getByLabelText("API 密钥"), { target: { value: "edited-test-key" } });
  await waitFor(() => expect(commands("prepare_provider_request")).toHaveLength(2));
  expect(commands("cancel_provider_request")).toContainEqual(["cancel_provider_request", { requestId: "token-1" }]);
  expect(commands("prepare_provider_request")[1]?.[1]).toMatchObject({ target: { connection: { apiKey: "edited-test-key" } } });
  rerender(<ProviderEditor {...props} active={false} />);
  await waitFor(() => expect(commands("cancel_provider_request")).toContainEqual(["cancel_provider_request", { requestId: "token-2" }]));
  expect(commands("execute_provider_request")).toHaveLength(0);
});
