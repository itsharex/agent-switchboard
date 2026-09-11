import { describe, expect, it } from "vitest";
import { render, screen, within } from "@testing-library/react";
import type { ConfigFileStatus, LockStatus } from "../api/client";
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
    {...overrides} />);
}

function codexCard() { return within(screen.getByRole("region", { name: "Codex 当前连接" })); }

describe("DualRelay", () => {
  it("shows live connection facts with configuration drift and no card actions", () => {
    renderCards({ lock: { state: "stale" } });
    expect(screen.getByText("当前服务")).toBeInTheDocument();
    expect(screen.getByText("current-model")).toBeInTheDocument();
    expect(screen.getByText("自定义服务")).toBeInTheDocument();
    expect(screen.getByText(/配置已被外部修改/)).toBeInTheDocument();
    expect(screen.getByText(/发现遗留写入锁/)).toBeInTheDocument();
    expect(screen.queryByText("配置正常")).not.toBeInTheDocument();
    expect(codexCard().queryByRole("button")).not.toBeInTheDocument();
  });

  it("reports saved changes that have not been applied without claiming the connection is healthy", () => {
    renderCards({ status: { ...status, matchStatus: { kind: "profileChanged", profileName: "已改档案" } } });
    expect(screen.getByText("档案或客户端偏好已更改，尚未应用")).toBeInTheDocument();
    expect(screen.queryByText("配置正常")).not.toBeInTheDocument();
  });

  it("does not display stale route facts after a read failure", () => {
    renderCards({ status: { ...status, readError: "访问被拒绝" } });
    expect(screen.getByText("读取失败：访问被拒绝")).toBeInTheDocument();
    expect(screen.queryByText("当前服务")).not.toBeInTheDocument();
    expect(screen.queryByText("current-model")).not.toBeInTheDocument();
  });

  it("keeps scope and indeterminate lock warnings visible", () => {
    renderCards({ status: { ...status, route: { ...status.route!, scopeWarnings: ["启动参数可能覆盖当前模型"] } },
      lock: { state: "indeterminate", reason: "无法检查进程" } });
    expect(screen.getByText("启动参数可能覆盖当前模型")).toBeInTheDocument();
    expect(screen.getByText("写入锁状态无法确定：无法检查进程")).toBeInTheDocument();
  });

  it("renders both clients with solid readable route facts and no credentials", () => {
    const { container } = renderCards({ status: { ...status, route: { ...status.route!,
      baseUrl: "https://username:password@example.test/v1?key=private", apiKey: "private-api-key" } } });
    expect(screen.getByRole("region", { name: "Claude 当前连接" })).toBeInTheDocument();
    expect(codexCard().getByText("example.test")).toBeInTheDocument();
    expect(container.querySelector("canvas")).toBeNull();
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
