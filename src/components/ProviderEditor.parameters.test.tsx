import { useState } from "react";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import * as client from "../api/client";
import { ProviderEditor } from "../test/provider-editor";
import { providerParameters } from "../test/provider-parameters";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function profile(id = "a", effort: "low" | "high" = "low"): client.ProviderProfile {
  const parameters = providerParameters("claude");
  parameters.settings.effortLevel = { mode: "explicit", value: effort };
  return { id, app: "claude", routeMode: "custom", name: `供应商 ${id}`, model: "claude-opus-4-1",
    baseUrl: "https://relay.example", apiKey: "test-key", upstreamProtocol: "anthropicMessages",
    responsesOptions: null, maxOutputTokens: null, modelOptions: null, parameters, websiteUrl: null };
}

const baseProps = { initialApp: "claude" as const, busy: false, officialTakenApps: [],
  userConfigModel: null, onCancel: vi.fn() };

async function openParameters(user: ReturnType<typeof userEvent.setup>) {
  await user.click(screen.getByRole("button", { name: "配置运行参数" }));
  return screen.findByRole("slider", { name: "推理强度" });
}

function fillRequiredFields() {
  fireEvent.change(screen.getByLabelText("名称"), { target: { value: "新供应商" } });
  fireEvent.change(screen.getByLabelText("服务地址"), { target: { value: "https://relay.example" } });
  fireEvent.change(screen.getByLabelText("API 密钥"), { target: { value: "test-key" } });
}

describe("provider-owned parameter draft", () => {
  it("opens parameters only through the button and returns with the same unsaved draft", async () => {
    const user = userEvent.setup();
    const savedProfile = profile();
    const onSave = vi.fn();
    const saveClient = vi.spyOn(client, "saveClientSettings");
    render(<ProviderEditor {...baseProps} profile={savedProfile} onSave={onSave} />);
    expect(screen.queryByRole("slider")).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("名称"), { target: { value: "更新名称" } });
    const effort = await openParameters(user);
    expect(screen.getByRole("heading", { name: "运行参数" })).toHaveFocus();
    expect(screen.queryByRole("button", { name: "返回供应商列表" })).not.toBeInTheDocument();
    expect(screen.queryByRole("textbox", { name: "名称" })).not.toBeInTheDocument();
    fireEvent.change(effort, { target: { value: "3" } });
    await user.click(screen.getByRole("button", { name: "返回供应商编辑" }));
    expect(screen.getByLabelText("名称")).toHaveValue("更新名称");
    expect(screen.getByRole("button", { name: "配置运行参数" })).toHaveFocus();
    expect(await openParameters(user)).toHaveValue("3");
    expect(onSave).not.toHaveBeenCalled();
    expect(saveClient).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "返回供应商编辑" }));
    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ name: "更新名称",
      parameters: { settings: { ...savedProfile.parameters.settings,
        effortLevel: { mode: "explicit", value: "high" } } },
      modelOptions: null }));
    expect(savedProfile.parameters.settings.effortLevel).toEqual({ mode: "explicit", value: "low" });
  });

  it("discards parameter edits when the whole supplier editor is cancelled", async () => {
    const user = userEvent.setup();
    const savedProfile = profile();
    const onSave = vi.fn();
    function Harness() {
      const [open, setOpen] = useState(true);
      return open ? <ProviderEditor {...baseProps} profile={savedProfile} onSave={onSave} onCancel={() => setOpen(false)} />
        : <button onClick={() => setOpen(true)}>重新编辑</button>;
    }
    render(<Harness />);
    fireEvent.change(await openParameters(user), { target: { value: "3" } });
    await user.click(screen.getByRole("button", { name: "返回供应商编辑" }));
    await user.click(screen.getByRole("button", { name: "取消" }));
    await user.click(screen.getByRole("button", { name: "重新编辑" }));
    expect(await openParameters(user)).toHaveValue("1");
    expect(onSave).not.toHaveBeenCalled();
  });

  it("keeps two providers independent when the selected profile changes", async () => {
    const user = userEvent.setup();
    const first = profile("a", "low");
    const second = profile("b", "high");
    const onSave = vi.fn();
    const { rerender } = render(<ProviderEditor {...baseProps} profile={first} onSave={onSave} />);
    fireEvent.change(await openParameters(user), { target: { value: "2" } });
    rerender(<ProviderEditor {...baseProps} profile={second} onSave={onSave} />);
    expect(await openParameters(user)).toHaveValue("3");
    await user.click(within(screen.getByRole("radiogroup", { name: "扩展思考" })).getByRole("radio", { name: "开启" }));
    await user.click(screen.getByRole("button", { name: "返回供应商编辑" }));
    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    expect(onSave.mock.calls[0][0].parameters.settings.effortLevel).toEqual({ mode: "explicit", value: "high" });
    rerender(<ProviderEditor {...baseProps} profile={first} onSave={onSave} />);
    expect(await openParameters(user)).toHaveValue("1");
    expect(within(screen.getByRole("radiogroup", { name: "扩展思考" })).getByRole("radio", { name: "自动" })).toBeChecked();
  });

  it("blocks a new profile until a failed catalog load is successfully retried", async () => {
    vi.mocked(client.getProviderParametersCatalog).mockRejectedValueOnce({ message: "目录不可用" });
    const user = userEvent.setup();
    const onSave = vi.fn();
    render(<ProviderEditor {...baseProps} profile={null} onSave={onSave} />);
    fillRequiredFields();
    expect(await screen.findByRole("alert")).toHaveTextContent("目录不可用");
    expect(screen.getByRole("button", { name: "保存供应商" })).toBeDisabled();
    fireEvent.submit(screen.getByRole("form", { name: "新建供应商" }));
    expect(onSave).not.toHaveBeenCalled();
    await user.click(screen.getByRole("button", { name: "配置运行参数" }));
    expect(screen.queryByRole("slider")).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "重新读取运行参数" }));
    expect(await screen.findByRole("slider", { name: "推理强度" })).toHaveValue("0");
    await user.click(screen.getByRole("button", { name: "返回供应商编辑" }));
    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    expect(onSave.mock.calls[0][0].parameters).toEqual(providerParameters("claude"));
  });

  it("allows official profiles to own parameters without custom model overrides", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const official: client.ProviderProfile = { ...profile(), routeMode: "official", model: null,
      baseUrl: null, apiKey: "", upstreamProtocol: null, responsesOptions: null, modelOptions: null };
    render(<ProviderEditor {...baseProps} profile={official} onSave={onSave} />);
    fireEvent.change(await openParameters(user), { target: { value: "3" } });
    await user.click(screen.getByRole("button", { name: "返回供应商编辑" }));
    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ routeMode: "official", modelOptions: null,
      parameters: { settings: { ...official.parameters.settings,
        effortLevel: { mode: "explicit", value: "high" } } } }));
  });

  it("keeps an in-progress official login alive while navigating the parameter level", async () => {
    const user = userEvent.setup();
    const start = vi.spyOn(client, "startOfficialLogin").mockResolvedValue({
      userCode: "ABCD", verificationUrl: "https://auth.example/verify" });
    const cancel = vi.spyOn(client, "cancelOfficialLogin").mockResolvedValue();
    render(<ProviderEditor {...baseProps} profile={null} onSave={vi.fn()} />);
    await user.click(screen.getByRole("radio", { name: "官方登录" }));
    await user.click(screen.getByRole("button", { name: "开始官方登录" }));
    expect(await screen.findByText("ABCD")).toBeInTheDocument();
    await openParameters(user);
    await user.click(screen.getByRole("button", { name: "返回供应商编辑" }));
    expect(start).toHaveBeenCalledTimes(1);
    expect(cancel).not.toHaveBeenCalled();
    expect(screen.getByText("ABCD")).toBeInTheDocument();
  });

  it("disables both levels during a save and resets using catalog defaults", async () => {
    const user = userEvent.setup();
    const onSave = vi.fn();
    const savedProfile = profile("a", "high");
    const { rerender } = render(<ProviderEditor {...baseProps} profile={savedProfile} onSave={onSave} />);
    await openParameters(user);
    rerender(<ProviderEditor {...baseProps} busy profile={savedProfile} onSave={onSave} />);
    expect(screen.getByRole("button", { name: "返回供应商编辑" })).toBeDisabled();
    expect(screen.getByRole("slider", { name: "推理强度" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "全部恢复默认值" })).toBeDisabled();
    rerender(<ProviderEditor {...baseProps} profile={savedProfile} onSave={onSave} />);
    await user.click(screen.getByRole("button", { name: "全部恢复默认值" }));
    await user.click(screen.getByRole("button", { name: "返回供应商编辑" }));
    await waitFor(() => expect(screen.getByRole("button", { name: "保存供应商" })).toBeEnabled());
    await user.click(screen.getByRole("button", { name: "保存供应商" }));
    expect(onSave.mock.calls[0][0].parameters).toEqual(providerParameters("claude"));
  });
});
