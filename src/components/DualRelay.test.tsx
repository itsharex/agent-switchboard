import { describe, expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ConfigFileStatus, LockStatus, ProviderProfile } from "../api/client";
import { providerParameters } from "../test/provider-parameters";
import { DualRelay } from "./DualRelay";

const status: ConfigFileStatus = {
  app: "codex", path: "test/config.toml", exists: true, syntaxOk: true, readError: null,
  activeProfileId: null, matchStatus: { kind: "externallyModified", at: "2026-09-08T01:00:00Z" },
  lastSwitch: null,
  route: { app: "codex", routeMode: "custom", providerName: "当前服务", model: "current-model",
    baseUrl: "https://example.test/v1", apiKey: "TEST_KEY", wireApi: "responses",
    codexModelOptions: null, haikuModel: null, sonnetModel: null, opusModel: null,
    availableModels: null, scopeWarnings: [],
  },
};

type Props = Omit<Parameters<typeof DualRelay>[0], "statuses" | "locks"> & { status: ConfigFileStatus; lock: LockStatus };
function renderCards({ status: current = status, lock = { state: "free" }, ...overrides }: Partial<Props> = {}) {
  return render(<DualRelay statuses={[current]} profiles={[]} locks={{ codex: lock }}
    onOpenDiagnostics={() => {}} onOpenQuota={() => {}} {...overrides} />);
}

function codexCard() { return within(screen.getByRole("region", { name: "Codex 当前连接" })); }

describe("DualRelay", () => {
  it("shows live connection facts with configuration drift and a reachable stale-lock diagnostic", async () => {
    const onOpenDiagnostics = vi.fn();
    renderCards({ lock: { state: "stale" }, onOpenDiagnostics });
    expect(screen.getByText("当前服务")).toBeInTheDocument();
    expect(screen.getByText("current-model")).toBeInTheDocument();
    expect(screen.getByText("自定义服务")).toBeInTheDocument();
    expect(screen.getByText(/配置已被外部修改/)).toBeInTheDocument();
    expect(screen.getByText(/发现遗留写入锁/)).toBeInTheDocument();
    expect(screen.queryByText("配置正常")).not.toBeInTheDocument();
    await userEvent.click(codexCard().getByRole("button", { name: "查看诊断" }));
    expect(onOpenDiagnostics).toHaveBeenCalledWith("configuration");
  });

  it("reports saved changes that have not been applied without claiming the connection is healthy", () => {
    renderCards({ status: { ...status, matchStatus: { kind: "profileChanged", profileName: "已改档案" } } });
    expect(screen.getByText("档案或客户端偏好已更改，尚未应用")).toBeInTheDocument();
    expect(screen.queryByText("配置正常")).not.toBeInTheDocument();
  });

  it("does not display stale route facts after a read failure", async () => {
    const onOpenDiagnostics = vi.fn();
    renderCards({ status: { ...status, readError: "访问被拒绝" }, onOpenDiagnostics });
    expect(screen.getByText("读取失败：访问被拒绝")).toBeInTheDocument();
    expect(screen.queryByText("当前服务")).not.toBeInTheDocument();
    expect(screen.queryByText("current-model")).not.toBeInTheDocument();
    await userEvent.click(codexCard().getByRole("button", { name: "查看诊断" }));
    expect(onOpenDiagnostics).toHaveBeenCalledWith("configuration");
  });

  it("keeps scope and indeterminate lock warnings visible", () => {
    renderCards({ status: { ...status, route: { ...status.route!, scopeWarnings: ["启动参数可能覆盖当前模型"] } },
      lock: { state: "indeterminate", reason: "无法检查进程" } });
    expect(screen.getByText("启动参数可能覆盖当前模型")).toBeInTheDocument();
    expect(screen.getByText("写入锁状态无法确定：无法检查进程")).toBeInTheDocument();
  });

  it("opens gateway diagnostics for a profile that requires conversion", async () => {
    const profile: ProviderProfile = { id: "converted", app: "codex", name: "转换服务", routeMode: "custom",
      model: null, baseUrl: "https://example.test/v1", apiKey: "test", upstreamProtocol: "chatCompletions",
      responsesOptions: null, maxOutputTokens: null, parameters: providerParameters("codex"),
      modelOptions: null, websiteUrl: null };
    const onOpenDiagnostics = vi.fn();
    renderCards({ profiles: [profile], status: { ...status, activeProfileId: profile.id }, onOpenDiagnostics });
    await userEvent.click(screen.getByRole("button", { name: "网关诊断" }));
    expect(onOpenDiagnostics).toHaveBeenCalledWith("gateway");
  });

  it("opens official quota details from an observed Codex official connection", async () => {
    const onOpenQuota = vi.fn();
    renderCards({ status: { ...status, route: { ...status.route!, routeMode: "official", providerName: null } }, onOpenQuota });
    await userEvent.click(screen.getByRole("button", { name: "官方额度详情" }));
    expect(onOpenQuota).toHaveBeenCalledOnce();
  });

  it("renders both clients, with starlight only for readable routes and no credentials", () => {
    const { container } = renderCards({ status: { ...status, route: { ...status.route!,
      baseUrl: "https://username:password@example.test/v1?key=private", apiKey: "private-api-key" } } });
    expect(screen.getByRole("region", { name: "Claude 当前连接" })).toBeInTheDocument();
    expect(codexCard().getByText("example.test")).toBeInTheDocument();
    expect(container.querySelectorAll("canvas")).toHaveLength(1);
    expect(container.textContent).not.toMatch(/username|password|private/);
  });

  it.each([{ syntaxOk: false }, { exists: false }, { readError: "无法读取" }])(
    "does not animate or show stale values when config is unreadable: %o", (failure) => {
      const { container } = renderCards({ status: { ...status, ...failure } });
      expect(container.querySelector("canvas")).toBeNull();
      expect(screen.queryByText("current-model")).not.toBeInTheDocument();
    },
  );
});
