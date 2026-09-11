import { providerParameters } from "../test/provider-parameters";
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
    responsesOptions: null,
    maxOutputTokens: null,
    parameters: providerParameters("codex"),
    modelOptions: null,
    notes: null,
    websiteUrl: null,
    usageQuery: null,
  };
}

function gatewayStatus(): GatewayStatus {
  const now = Date.now();
  return {
    configuredPort: 47821,
    listeningPort: 47821,
    baseUrl: "http://127.0.0.1:47821",
    status: "running",
    failure: null,
    repairReason: null,
    blockedRecovery: null,
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
          routeRevision: "route-a",
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
          routeRevision: "route-b",
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
          routeRevision: null,
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

function conflictStatus(): GatewayStatus {
  const status = gatewayStatus();
  status.listeningPort = null;
  status.baseUrl = null;
  status.status = "portConflict";
  status.failure = {
    port: 47821,
    kind: "portInUse",
    osCode: 10048,
    message: "无法监听 127.0.0.1:47821：端口已被占用（占用进程未知）。",
    process: null,
  };
  return status;
}

describe("GatewayPage", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(gatewayStatus());
  });

  it("渲染状态徽标、监听信息、状态瓦片、活动路由与最近请求表", async () => {
    render(<GatewayPage active profiles={[profileRecord("p1", "沙箱供应商")]} />);

    await screen.findByRole("table", { name: "最近经本机协议网关转换的请求" });
    expect(invokeMock).toHaveBeenCalledWith("gateway_status");

    expect(screen.getByText("运行中", { selector: "[role=status]" })).toBeInTheDocument();
    const listenInfo = screen.getByLabelText("监听信息");
    expect(within(listenInfo).getByText("http://127.0.0.1:47821")).toBeInTheDocument();
    expect(within(listenInfo).getByRole("button", { name: "复制" })).toBeInTheDocument();
    expect(within(listenInfo).getByText("47821")).toBeInTheDocument();
    expect(within(listenInfo).getByRole("button", { name: "修改" })).toBeInTheDocument();

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
      within(screen.getByRole("group", { name: "实际监听" })).getByText("47821"),
    ).toBeInTheDocument();

    const routes = screen.getByRole("region", { name: "活动路由" });
    expect(within(routes).getByText("沙箱供应商")).toBeInTheDocument();
    expect(within(routes).getByText(/Chat Completions/)).toBeInTheDocument();

    const table = screen.getByRole("table", { name: "最近经本机协议网关转换的请求" });
    expect(within(table).getByText("HTTP 500")).toBeInTheDocument();
    expect(within(table).getByText("中断")).toBeInTheDocument();
    expect(within(table).getByText("未匹配路由")).toBeInTheDocument();
    expect(within(table).getByText("route-a")).toBeInTheDocument();
    expect(within(table).getByText("route-b")).toBeInTheDocument();
    expect(within(table).getAllByText("—")).toHaveLength(2);

    expect(screen.getByRole("figure", { name: "近 60 分钟请求趋势" })).toBeInTheDocument();
  });

  it("端口冲突时显示结构化错误并提供重试与修改端口", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) =>
      command === "gateway_retry_bind"
        ? Promise.resolve(gatewayStatus())
        : Promise.resolve(conflictStatus()),
    );

    render(<GatewayPage active profiles={[]} />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "无法监听 127.0.0.1:47821：端口已被占用（占用进程未知）。",
    );
    expect(screen.getByText("端口冲突", { selector: "[role=status]" })).toBeInTheDocument();
    expect(screen.getByText("未在监听", { selector: "span" })).toBeInTheDocument();
    expect(
      within(screen.getByRole("group", { name: "实际监听" })).getByText("未监听"),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "重试" }));
    await waitFor(() => {
      expect(screen.getByText("运行中", { selector: "[role=status]" })).toBeInTheDocument();
    });
    expect(invokeMock).toHaveBeenCalledWith("gateway_retry_bind");
  });

  it("修改端口走预览并确认后应用，提示重启客户端", async () => {
    const user = userEvent.setup();
    const plan = {
      preparationId: "prep-1",
      fromPort: 47821,
      toPort: 47822,
      clients: [
        {
          app: "codex" as const,
          profileId: "p1",
          profileName: "沙箱供应商",
          currentBaseUrl: "http://127.0.0.1:47821/v1",
          newBaseUrl: "http://127.0.0.1:47822/v1",
        },
      ],
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "gateway_prepare_port_change") return Promise.resolve(plan);
      if (command === "gateway_commit_port_change")
        return Promise.resolve({ fromPort: 47821, toPort: 47822, clients: plan.clients, warnings: [] });
      return Promise.resolve(gatewayStatus());
    });

    render(<GatewayPage active profiles={[profileRecord("p1", "沙箱供应商")]} />);
    await user.click(await screen.findByRole("button", { name: "修改" }));

    const portInput = await screen.findByLabelText(/新监听端口/);
    await user.clear(portInput);
    await user.type(portInput, "47822");
    await user.click(screen.getByRole("button", { name: "下一步：预览变更" }));

    const dialog = await screen.findByRole("dialog", { name: "修改监听端口" });
    expect(invokeMock).toHaveBeenCalledWith("gateway_prepare_port_change", { newPort: 47822 });
    expect(within(dialog).getByText("监听端口：47821 → 47822")).toBeInTheDocument();
    expect(within(dialog).getByText("http://127.0.0.1:47821/v1")).toBeInTheDocument();
    expect(within(dialog).getByText("http://127.0.0.1:47822/v1")).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "确认修改并应用" }));

    expect(invokeMock).toHaveBeenCalledWith("gateway_commit_port_change", {
      preparationId: "prep-1",
      confirmWrite: true,
    });
    expect(
      await within(dialog).findByText(/监听端口已改为 47822/),
    ).toBeInTheDocument();
    expect(within(dialog).getByText(/请重新启动相关客户端或会话/)).toBeInTheDocument();
  });

  it("取消端口预览会释放后端保留的监听套接字", async () => {
    const user = userEvent.setup();
    const plan = {
      preparationId: "prep-cancel",
      fromPort: 47821,
      toPort: 47822,
      clients: [],
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "gateway_prepare_port_change") return Promise.resolve(plan);
      return Promise.resolve(gatewayStatus());
    });
    render(<GatewayPage active profiles={[]} />);
    await user.click(await screen.findByRole("button", { name: "修改" }));
    const portInput = await screen.findByLabelText(/新监听端口/);
    await user.clear(portInput);
    await user.type(portInput, "47822");
    await user.click(screen.getByRole("button", { name: "下一步：预览变更" }));
    const dialog = await screen.findByRole("dialog", { name: "修改监听端口" });

    await user.click(within(dialog).getByRole("button", { name: "取消" }));

    await waitFor(() => {
      expect(invokeMock).toHaveBeenCalledWith("gateway_cancel_port_change", {
        preparationId: "prep-cancel",
      });
    });
  });

  it("显示端口准备命令返回的结构化错误", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((command: string) => {
      if (command === "gateway_prepare_port_change") {
        return Promise.reject({
          code: "gateway-port-change-invalid",
          message: "端口已被占用，请选择其他端口",
        });
      }
      return Promise.resolve(gatewayStatus());
    });
    render(<GatewayPage active profiles={[]} />);
    await user.click(await screen.findByRole("button", { name: "修改" }));
    const portInput = await screen.findByLabelText(/新监听端口/);
    await user.clear(portInput);
    await user.type(portInput, "47822");
    await user.click(screen.getByRole("button", { name: "下一步：预览变更" }));

    expect(await screen.findByRole("alert")).toHaveTextContent("端口已被占用，请选择其他端口");
  });

  it("端口提交失败后回到输入，避免重用已失效的预览", async () => {
    const user = userEvent.setup();
    const plan = {
      preparationId: "prep-expired",
      fromPort: 47821,
      toPort: 47822,
      clients: [],
    };
    invokeMock.mockImplementation((command: string) => {
      if (command === "gateway_prepare_port_change") return Promise.resolve(plan);
      if (command === "gateway_commit_port_change") {
        return Promise.reject({
          code: "gateway-port-change-failed",
          message: "客户端配置已在预览后变化，请重新发起修改",
        });
      }
      return Promise.resolve(gatewayStatus());
    });
    render(<GatewayPage active profiles={[]} />);
    await user.click(await screen.findByRole("button", { name: "修改" }));
    const portInput = await screen.findByLabelText(/新监听端口/);
    await user.clear(portInput);
    await user.type(portInput, "47822");
    await user.click(screen.getByRole("button", { name: "下一步：预览变更" }));
    await user.click(screen.getByRole("button", { name: "确认修改并应用" }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "客户端配置已在预览后变化，请重新发起修改",
    );
    expect(screen.getByRole("button", { name: "下一步：预览变更" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "确认修改并应用" })).not.toBeInTheDocument();
  });

  it("需要修复与需要恢复状态给出对应指引与放弃入口", async () => {
    const user = userEvent.setup();
    const broken = gatewayStatus();
    broken.status = "recoveryBlocked";
    broken.blockedRecovery = {
      fromPort: 47821,
      toPort: 47822,
      apps: ["codex"],
      reason: "客户端配置在事务外被修改，拒绝覆盖",
    };
    invokeMock.mockImplementation((command: string) =>
      command === "gateway_discard_port_change"
        ? Promise.resolve(gatewayStatus())
        : Promise.resolve(broken),
    );

    render(<GatewayPage active profiles={[]} />);

    expect(await screen.findByText("需要恢复", { selector: "[role=status]" })).toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent(/上次端口修改（47821 → 47822）需要处理/);
    expect(screen.getByRole("alert")).toHaveTextContent("客户端配置在事务外被修改，拒绝覆盖");

    await user.click(screen.getByRole("button", { name: "保留当前配置并清除恢复记录" }));
    await user.click(screen.getByRole("button", { name: "确认保留并清除" }));

    await waitFor(() => {
      expect(screen.getByText("运行中", { selector: "[role=status]" })).toBeInTheDocument();
    });
    expect(invokeMock).toHaveBeenCalledWith("gateway_discard_port_change", { confirmWrite: true });
  });

  it("显示后端提供的旧 Codex 路由修复原因", async () => {
    const broken = gatewayStatus();
    broken.status = "needsRepair";
    broken.repairReason = "旧版本机协议网关状态已被隔离；旧 Codex 路由不会恢复。请重新创建并应用 Codex 档案。";
    invokeMock.mockResolvedValue(broken);

    render(<GatewayPage active profiles={[]} />);

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "旧版本机协议网关状态已被隔离；旧 Codex 路由不会恢复。请重新创建并应用 Codex 档案。",
    );
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
    // 上游协议分布、最近请求与近 60 分钟趋势三处空态；趋势空态替代空坐标轴。
    expect(screen.getAllByText("暂无网关请求记录。")).toHaveLength(3);
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
