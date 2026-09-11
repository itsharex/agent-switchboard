import { act, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import * as client from "../api/client";
import { ProviderEditor } from "../test/provider-editor";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const props = { profile: null, initialApp: "claude" as const, busy: false, officialTakenApps: [],
  userConfigModel: null, onCancel: vi.fn(), onSave: vi.fn() };

async function selectResponses(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("combobox", { name: "API 格式" }));
  await user.click(await screen.findByRole("option", { name: /Responses/ }));
}

function deferred() {
  let resolve!: (value: client.ProviderEndpoints) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<client.ProviderEndpoints>((done, failed) => { resolve = done; reject = failed; });
  return { promise, resolve, reject };
}

it("resolves a nonempty API root through the backend without sending credentials", async () => {
  const user = userEvent.setup();
  const result = { requestUrl: "https://backend-selected.example/v2/responses", modelsUrl: "https://backend-selected.example/v2/models" };
  const resolve = vi.mocked(client.resolveProviderEndpoints).mockResolvedValue(result);
  render(<ProviderEditor {...props} />);
  await selectResponses(user);
  expect(resolve).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("API 密钥"), { target: { value: "private-test-key" } });
  expect(resolve).not.toHaveBeenCalled();
  fireEvent.change(screen.getByLabelText("服务地址"), { target: { value: " https://typed.example/custom " } });
  expect(await screen.findByText(result.requestUrl)).toBeInTheDocument();
  expect(resolve).toHaveBeenCalledWith("https://typed.example/custom", "responses");
  expect(JSON.stringify(resolve.mock.calls)).not.toContain("private-test-key");
  expect(screen.getByText("请求地址")).toBeInTheDocument();
});

it.each(["success", "failure"])("discards a late resolver %s after a newer address resolves", async (outcome) => {
  const user = userEvent.setup();
  const first = deferred();
  const second = deferred();
  vi.mocked(client.resolveProviderEndpoints).mockImplementationOnce(() => first.promise).mockImplementationOnce(() => second.promise);
  render(<ProviderEditor {...props} />);
  await selectResponses(user);
  const input = screen.getByLabelText("服务地址");
  fireEvent.change(input, { target: { value: "https://first.example" } });
  fireEvent.change(input, { target: { value: "https://second.example/v2" } });
  await act(async () => second.resolve({ requestUrl: "https://second.example/v2/responses", modelsUrl: "https://second.example/v2/models" }));
  await act(async () => outcome === "success"
    ? first.resolve({ requestUrl: "https://first.example/responses", modelsUrl: "https://first.example/models" })
    : first.reject({ message: "过期的地址错误" }));
  expect(screen.getByText("https://second.example/v2/responses")).toBeInTheDocument();
  expect(screen.queryByText("https://first.example/responses")).not.toBeInTheDocument();
  expect(screen.queryByText("过期的地址错误")).not.toBeInTheDocument();
});

it("uses the selected protocol to resolve the actual upstream address for a gateway route", async () => {
  const user = userEvent.setup();
  const resolve = vi.mocked(client.resolveProviderEndpoints)
    .mockResolvedValueOnce({ requestUrl: "https://upstream.example/responses", modelsUrl: "https://upstream.example/models" })
    .mockResolvedValueOnce({ requestUrl: "https://upstream.example/chat/completions", modelsUrl: "https://upstream.example/models" });
  render(<ProviderEditor {...props} />);
  await selectResponses(user);
  fireEvent.change(screen.getByLabelText("服务地址"), { target: { value: "https://upstream.example" } });
  await screen.findByText("https://upstream.example/responses");
  await user.click(screen.getByRole("combobox", { name: "API 格式" }));
  await user.click(screen.getByRole("option", { name: /Chat Completions/ }));
  expect(await screen.findByText("https://upstream.example/chat/completions")).toBeInTheDocument();
  expect(resolve).toHaveBeenLastCalledWith("https://upstream.example", "chatCompletions");
});

it("shows the backend path error beside the URL and clears the previous resolved address", async () => {
  const user = userEvent.setup();
  const resolve = vi.mocked(client.resolveProviderEndpoints)
    .mockResolvedValueOnce({ requestUrl: "https://upstream.example/responses", modelsUrl: "https://upstream.example/models" })
    .mockRejectedValueOnce({ code: "provider-endpoint-invalid", message: "请填写 API 根地址，不要包含 /responses" });
  render(<ProviderEditor {...props} />);
  await selectResponses(user);
  const input = screen.getByLabelText("服务地址");
  fireEvent.change(input, { target: { value: "https://upstream.example" } });
  await screen.findByText("https://upstream.example/responses");
  fireEvent.change(input, { target: { value: "https://upstream.example/responses" } });
  expect(await screen.findByRole("alert")).toHaveTextContent("不要包含 /responses");
  expect(input).toHaveAttribute("aria-invalid", "true");
  expect(screen.queryByText("https://upstream.example/responses")).not.toBeInTheDocument();
  expect(resolve).toHaveBeenCalledTimes(2);
  fireEvent.change(input, { target: { value: "" } });
  expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  expect(resolve).toHaveBeenCalledTimes(2);
});
