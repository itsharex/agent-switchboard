import { useState, type ComponentProps } from "react";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { GatewayStatus, RuntimeOverview } from "../api/client";
import type { DiagnosticSection } from "../app/navigation";
import { DiagnosticsPage } from "./DiagnosticsPage";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = vi.mocked(invoke);

const gateway: GatewayStatus = {
  configuredPort: 47821, listeningPort: 47821, baseUrl: "http://127.0.0.1:47821", status: "standby",
  failure: null, blockedRecovery: null, routes: [],
  metrics: { startedAtMs: 0, totalRequests: 0, failedRequests: 0, samples: [] },
};
const runtime: RuntimeOverview = {
  appVersion: "0.1.5", buildMode: "release", platform: "windows", architecture: "x86_64",
  transport: { kind: "desktopProtocol" }, appDataPath: "/isolated/agent-switchboard",
};
const defaults: Omit<ComponentProps<typeof DiagnosticsPage>, "active" | "section"> = {
  onSectionChange: vi.fn(), statuses: [], profiles: [], locks: {}, busy: false,
  onRefresh: vi.fn(), onRecoverLock: vi.fn(), logLevel: "info", onLogLevelChange: vi.fn(),
};

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (command) => {
    if (command === "gateway_status" || command === "gateway_discard_port_change") return gateway;
    if (command === "runtime_overview") return runtime;
    if (command === "list_runtime_logs") return [];
    throw new Error(`Unexpected command: ${command}`);
  });
});
afterEach(() => vi.useRealTimers());

function callsFor(command: string) {
  return invokeMock.mock.calls.filter(([name]) => name === command).length;
}

it("loads configuration environment and logs only when opened and preserves the completed reads", async () => {
  const { rerender } = render(<DiagnosticsPage {...defaults} active={false} section="configuration" />);
  expect(invokeMock).not.toHaveBeenCalled();

  rerender(<DiagnosticsPage {...defaults} active section="configuration" />);
  expect(await screen.findByText("无 TCP 端口")).toBeVisible();
  expect(callsFor("runtime_overview")).toBe(1);
  expect(callsFor("list_runtime_logs")).toBe(0);
  expect(callsFor("gateway_status")).toBe(0);

  rerender(<DiagnosticsPage {...defaults} active section="logs" />);
  expect(await screen.findByText("暂无应用运行日志")).toBeVisible();
  expect(screen.getByText("无 TCP 端口")).not.toBeVisible();
  rerender(<DiagnosticsPage {...defaults} active section="configuration" />);
  expect(screen.getByText("无 TCP 端口")).toBeVisible();
  rerender(<DiagnosticsPage {...defaults} active section="logs" />);
  expect(screen.getByText("暂无应用运行日志")).toBeVisible();
  expect(callsFor("runtime_overview")).toBe(1);
  expect(callsFor("list_runtime_logs")).toBe(1);
});

it("polls the gateway only while visible and keeps its last status across navigation", async () => {
  vi.useFakeTimers();
  const { rerender } = render(<DiagnosticsPage {...defaults} active section="gateway" />);
  await act(async () => {});
  const standby = screen.getByLabelText("网关运行状态");
  expect(standby).toHaveTextContent("待命");
  expect(callsFor("gateway_status")).toBe(1);
  await act(async () => { vi.advanceTimersByTime(5000); });
  expect(callsFor("gateway_status")).toBe(2);

  rerender(<DiagnosticsPage {...defaults} active section="logs" />);
  await act(async () => { vi.advanceTimersByTime(15000); });
  expect(callsFor("gateway_status")).toBe(2);
  expect(standby).not.toBeVisible();
  invokeMock.mockImplementation(() => new Promise(() => {}));
  rerender(<DiagnosticsPage {...defaults} active section="gateway" />);
  expect(standby).toBeVisible();
  expect(standby).toHaveTextContent("待命");
  expect(callsFor("gateway_status")).toBe(3);

  rerender(<DiagnosticsPage {...defaults} active={false} section="gateway" />);
  await act(async () => { vi.advanceTimersByTime(15000); });
  expect(callsFor("gateway_status")).toBe(3);
  expect(standby).not.toBeVisible();
});

it("keeps stale-lock and blocked gateway recovery reachable through the diagnostic selectors", async () => {
  const user = userEvent.setup();
  const onRecoverLock = vi.fn();
  const blocked: GatewayStatus = { ...gateway, status: "recoveryBlocked", blockedRecovery: {
    fromPort: 47821, toPort: 47822, apps: ["codex"], reason: "配置已被外部修改",
  } };
  invokeMock.mockImplementation(async (command) => {
    if (command === "gateway_status") return blocked;
    if (command === "gateway_discard_port_change") return gateway;
    if (command === "runtime_overview") return runtime;
    throw new Error(`Unexpected command: ${command}`);
  });
  function Harness() {
    const [section, setSection] = useState<DiagnosticSection>("configuration");
    return <DiagnosticsPage {...defaults} active section={section} onSectionChange={setSection} onRecoverLock={onRecoverLock}
      locks={{ codex: { state: "stale" } }} statuses={[{ app: "codex", path: "/isolated/config.toml", exists: true,
        syntaxOk: true, route: null, readError: null, activeProfileId: null, lastSwitch: null, matchStatus: { kind: "unmanaged" } }]} />;
  }
  render(<Harness />);

  await user.click(screen.getByRole("button", { name: "清理遗留锁" }));
  expect(onRecoverLock).toHaveBeenCalledExactlyOnceWith("codex");
  await user.click(screen.getByRole("tab", { name: "本机网关" }));
  expect(await screen.findByLabelText("网关运行状态")).toHaveTextContent("需要恢复");
  expect(screen.queryByRole("button", { name: "清理遗留锁" })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "保留当前配置并清除恢复记录" }));
  expect(callsFor("gateway_discard_port_change")).toBe(0);
  await user.click(screen.getByRole("button", { name: "确认保留并清除" }));
  await waitFor(() => expect(screen.getByLabelText("网关运行状态")).toHaveTextContent("待命"));
  expect(invokeMock).toHaveBeenCalledWith("gateway_discard_port_change", { confirmWrite: true });
});
