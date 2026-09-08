import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import type { ProviderProfile, ResponsesOptions } from "../api/client";
import { ProviderEditor } from "../test/provider-editor";
import { providerParameters } from "../test/provider-parameters";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const profile: ProviderProfile = {
  id: "layout-provider", app: "codex", routeMode: "custom", name: "供应商", model: "model-id",
  baseUrl: "https://relay.example/v1", apiKey: "test-key", upstreamProtocol: "responses",
  responsesOptions: { requestMode: "standard" },
  maxOutputTokens: null, modelOptions: null, parameters: providerParameters("codex"), websiteUrl: null,
};
const props = { initialApp: "codex" as const, busy: false, officialTakenApps: [],
  userConfigModel: null, onCancel: vi.fn() };

function summary(label: string) {
  return screen.getByText(label, { selector: "summary > span" }).closest("summary")!;
}

it("groups the always-visible fields and keeps one form and save action", async () => {
  const onSave = vi.fn();
  render(<ProviderEditor {...props} profile={profile} onSave={onSave} />);
  const basic = within(screen.getByRole("region", { name: "基本资料" }));
  const connection = within(screen.getByRole("region", { name: "连接配置" }));
  const models = within(screen.getByRole("region", { name: "模型" }));
  expect(basic.getByRole("textbox", { name: "名称" })).toBeVisible();
  expect(basic.getByRole("textbox", { name: "官网地址" })).toBeVisible();
  expect(connection.getByRole("combobox", { name: "API 格式" })).toBeVisible();
  expect(connection.getByLabelText("服务地址")).toBeVisible();
  expect(connection.getByLabelText("API 密钥")).toBeVisible();
  expect(models.getByRole("textbox", { name: "主模型" })).toBeVisible();
  expect(models.getByRole("button", { name: "获取模型" })).toBeVisible();
  expect(models.queryByRole("button", { name: "测试供应商" })).not.toBeInTheDocument();
  expect(screen.getByRole("button", { name: "测试供应商" })).toBeVisible();
  expect(screen.getAllByRole("form")).toHaveLength(1);
  expect(screen.getAllByRole("button", { name: "保存供应商" })).toHaveLength(1);
  await waitFor(() => expect(screen.getByRole("button", { name: "保存供应商" })).toBeEnabled());
  fireEvent.submit(screen.getByRole("form", { name: "编辑供应商" }));
  expect(onSave).toHaveBeenCalledTimes(1);
});

it("toggles advanced options without losing an edit after collapsing", async () => {
  const onSave = vi.fn();
  const user = userEvent.setup();
  render(<ProviderEditor {...props} profile={profile} onSave={onSave} />);
  const advanced = summary("Responses 能力");
  expect(advanced).toHaveTextContent("HTTP/SSE · 标准请求");
  expect(advanced.parentElement).not.toHaveAttribute("open");
  await user.click(advanced);
  expect(advanced.parentElement).toHaveAttribute("open");
  await user.click(screen.getByRole("combobox", { name: "请求模式" }));
  await user.click(screen.getByRole("option", { name: "最小请求" }));
  await user.click(advanced);
  expect(advanced.parentElement).not.toHaveAttribute("open");
  expect(advanced).toHaveTextContent("HTTP/SSE · 最小请求");
  await user.click(screen.getByRole("button", { name: "保存供应商" }));
  expect(onSave).toHaveBeenCalledWith(expect.objectContaining({
    responsesOptions: { requestMode: "minimal" },
  }));
});

it.each<ResponsesOptions>([
  { requestMode: "minimal" },
])("expands existing nondefault Responses options: $requestMode", (responsesOptions) => {
  render(<ProviderEditor {...props} profile={{ ...profile, responsesOptions }} onSave={vi.fn()} />);
  expect(summary("Responses 能力").parentElement).toHaveAttribute("open");
  expect(screen.getByRole("combobox", { name: "请求模式" })).toBeVisible();
});

it("initializes disclosure state from the newly selected profile", () => {
  const { rerender } = render(<ProviderEditor {...props} profile={profile} onSave={vi.fn()} />);
  expect(summary("Responses 能力").parentElement).not.toHaveAttribute("open");
  rerender(<ProviderEditor {...props} profile={{ ...profile, id: "another-provider",
    responsesOptions: { requestMode: "minimal" } }} onSave={vi.fn()} />);
  expect(summary("Responses 能力").parentElement).toHaveAttribute("open");
  expect(screen.getByRole("combobox", { name: "请求模式" })).toHaveTextContent("最小请求");
});

it("keeps existing notes visible initially and saves edits after collapsing them", async () => {
  const onSave = vi.fn();
  const user = userEvent.setup();
  render(<ProviderEditor {...props} profile={{ ...profile, notes: "团队共用" }} onSave={onSave} />);
  expect(summary("备注").parentElement).toHaveAttribute("open");
  fireEvent.change(screen.getByRole("textbox", { name: "备注" }), { target: { value: "更新的备注" } });
  await user.click(summary("备注"));
  expect(summary("备注").parentElement).not.toHaveAttribute("open");
  await user.click(screen.getByRole("button", { name: "保存供应商" }));
  expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ notes: "更新的备注" }));
});

it("opens existing Claude mappings and keeps empty mappings optional", () => {
  const claude: ProviderProfile = { ...profile, app: "claude", upstreamProtocol: "anthropicMessages", responsesOptions: null,
    parameters: providerParameters("claude"), modelOptions: { kind: "claude", primaryOneM: false,
      haikuModel: null, sonnetModel: "sonnet-model", sonnetOneM: false, opusModel: null, opusOneM: false, availableModels: null } };
  const { rerender } = render(<ProviderEditor {...props} profile={claude} onSave={vi.fn()} />);
  expect(summary("模型映射").parentElement).toHaveAttribute("open");
  expect(screen.getByRole("textbox", { name: "Sonnet 档" })).toHaveValue("sonnet-model");
  rerender(<ProviderEditor {...props} profile={{ ...claude, id: "empty-claude", modelOptions: null }} onSave={vi.fn()} />);
  expect(summary("模型映射").parentElement).not.toHaveAttribute("open");
  expect(screen.getByRole("textbox", { name: "Sonnet 档" })).not.toBeVisible();
});
