import { expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import * as client from "../../api/client";
import type { ProviderProfile } from "../../api/client";
import { gatewayStatus, ProviderEditor } from "../../test/provider-editor";
import { providerParameters } from "../../test/provider-parameters";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

function profile(managed: boolean): ProviderProfile {
  return { id: "claude-editor-fixture", app: "claude", routeMode: "custom", name: "Claude 供应商",
    model: "claude-sonnet-5", baseUrl: "https://api.example.test", apiKey: managed ? "" : "fake-provider-key",
    authentication: "bearer", upstreamProtocol: "anthropicMessages", responsesOptions: null, maxOutputTokens: null,
    modelOptions: null, parameters: providerParameters("claude"), websiteUrl: null,
    connection: managed ? { providerType: "github_copilot", authBinding: { source: "managed_account", authProvider: "github_copilot", accountId: "42" } } : {} };
}

function controls(container: HTMLElement) {
  return [...container.querySelectorAll("input,textarea,button")].map((node) => [
    node.tagName, node.getAttribute("type"), node.getAttribute("aria-label"), node.className,
  ]);
}

it("saves a keyless managed Claude draft through the same controls without introducing account UI", async () => {
  vi.spyOn(client, "getGatewayStatus").mockResolvedValue({ configuredPort: 47821, listeningPort: 47821, baseUrl: "http://127.0.0.1:47821",
    status: "standby", failure: null, repairReason: null, blockedRecovery: null, routes: [], metrics: gatewayStatus(47821).metrics });
  const user = userEvent.setup();
  const onSave = vi.fn();
  const managed = render(<ProviderEditor profile={profile(true)} initialApp="claude" busy={false}
    officialTakenApps={[]} userConfigModel={null} onSave={onSave} onCancel={() => {}} />);
  await waitFor(() => expect(screen.getByRole("button", { name: "保存供应商" })).toBeEnabled());
  expect(screen.getByLabelText("API 密钥")).not.toBeRequired();
  const originalControls = controls(managed.container);
  await user.click(screen.getByRole("button", { name: "保存供应商" }));
  expect(onSave).toHaveBeenCalledWith(expect.objectContaining({ app: "claude", apiKey: "", connection: profile(true).connection }));
  managed.unmount();
  const direct = render(<ProviderEditor profile={profile(false)} initialApp="claude" busy={false}
    officialTakenApps={[]} userConfigModel={null} onSave={vi.fn()} onCancel={() => {}} />);
  await waitFor(() => expect(screen.getByRole("button", { name: "保存供应商" })).toBeEnabled());
  expect(screen.getByLabelText("API 密钥")).toBeRequired();
  expect(controls(direct.container)).toEqual(originalControls);
});
