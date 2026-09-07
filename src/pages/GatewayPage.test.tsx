import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import type { GatewayStatus, ProviderProfile } from "../api/client";
import { GatewayPage } from "./GatewayPage";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = vi.mocked(invoke);

function profileRecord(id: string, name: string): ProviderProfile {
  return {
    id,
    app: "codex",
    routeMode: "custom",
    name,
    model: null,
    baseUrl: "http://127.0.0.1:9",
    apiKey: "fixture-key",
    upstreamProtocol: "chatCompletions",
    maxOutputTokens: null,
    modelOptions: null,
    notes: null,
    websiteUrl: null,
    usageQuery: null,
  };
}

function gatewayStatus(): GatewayStatus {
  const now = Date.now();
  return {
    port: 47821,
    baseUrl: "http://127.0.0.1:47821",
    routes: [{ app: "codex", profileId: "p1", upstreamProtocol: "chatCompletions" }],
    metrics: {
      startedAtMs: now - 120_000,
      totalRequests: 3,
      failedRequests: 1,
      samples: [
        {
          atMs: now - 60_000,
          app: "codex",
          profileId: "p1",
          clientProtocol: "responses",
          upstreamProtocol: "chatCompletions",
          status: 200,
          durationMs: 120,
          requestBytes: 100,
          responseBytes: 400,
        },
        {
          atMs: now - 30_000,
          app: "codex",
          profileId: "p1",
          clientProtocol: "responses",
          upstreamProtocol: "chatCompletions",
          status: 500,
          durationMs: 80,
          requestBytes: 100,
          responseBytes: 0,
        },
        {
          atMs: now - 10_000,
          app: "claude",
          profileId: null,
          clientProtocol: "anthropicMessages",
          upstreamProtocol: null,
          status: null,
          durationMs: 50,
          requestBytes: 0,
          responseBytes: 0,
        },
      ],
    },
  };
}

describe("GatewayPage", () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it("渲染状态瓦片、活动路由与最近请求表", async () => {
    invokeMock.mockResolvedValue(gatewayStatus());

    render(<GatewayPage active profiles={[profileRecord("p1", "沙箱供应商")]} />);

    await screen.findByRole("table", { name: "最近经本机协议网关转换的请求" });
    expect(invokeMock).toHaveBeenCalledWith("gateway_status");

    expect(
      within(screen.getByRole("group", { name: "累计请求" })).getByText("3"),
    ).toBeInTheDocument();
    expect(
      within(screen.getByRole("group", { name: "失败请求" })).getByText("1"),
    ).toBeInTheDocument();
    expect(
      within(screen.getByRole("group", { name: "活动路由" })).getByText("1"),
    ).toBeInTheDocument();
    expect(
      within(screen.getByRole("group", { name: "监听端口" })).getByText("47821"),
    ).toBeInTheDocument();

    const routes = screen.getByRole("region", { name: "活动路由" });
    expect(within(routes).getByText("沙箱供应商")).toBeInTheDocument();
    expect(within(routes).getByText(/Chat Completions/)).toBeInTheDocument();

    const table = screen.getByRole("table", { name: "最近经本机协议网关转换的请求" });
    expect(within(table).getByText("HTTP 500")).toBeInTheDocument();
    expect(within(table).getByText("中断")).toBeInTheDocument();
    expect(within(table).getByText("未匹配路由")).toBeInTheDocument();
    expect(within(table).getAllByText("—")).toHaveLength(1);

    expect(screen.getByRole("figure", { name: "近 60 分钟请求趋势" })).toBeInTheDocument();
  });

  it("无路由与无请求时显示空状态", async () => {
    const empty = gatewayStatus();
    empty.routes = [];
    empty.metrics.samples = [];
    empty.metrics.totalRequests = 0;
    empty.metrics.failedRequests = 0;
    invokeMock.mockResolvedValue(empty);

    render(<GatewayPage active profiles={[]} />);

    expect(
      await screen.findByText("当前没有经本机协议网关转换的供应商；客户端均在直连或官方登录。"),
    ).toBeInTheDocument();
    expect(screen.getAllByText("暂无网关请求记录。")).toHaveLength(2);
  });

  it("读取失败时给出重试入口并在重试后恢复", async () => {
    const user = userEvent.setup();
    invokeMock.mockRejectedValueOnce(new Error("gateway unavailable"));

    render(<GatewayPage active profiles={[]} />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "无法读取网关状态：gateway unavailable",
    );

    invokeMock.mockResolvedValue(gatewayStatus());
    await user.click(screen.getByRole("button", { name: "重试" }));

    await waitFor(() => {
      expect(screen.getByRole("table", { name: "最近经本机协议网关转换的请求" })).toBeInTheDocument();
    });
  });

  it("非激活页不发起轮询", () => {
    render(<GatewayPage active={false} profiles={[]} />);
    expect(invokeMock).not.toHaveBeenCalled();
  });
});
