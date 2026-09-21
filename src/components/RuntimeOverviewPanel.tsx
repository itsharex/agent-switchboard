import { useEffect, useState } from "react";
import { getRuntimeOverview, openAppDataDir, type RuntimeOverview } from "../api/client";
import { FactPath } from "./FactPath";
import { ModuleHeader } from "./WorkspaceHeader";

function errorMessage(reason: unknown): string {
  return reason instanceof Error && reason.message ? reason.message : "未提供具体原因";
}

function buildModeLabel(buildMode: RuntimeOverview["buildMode"]): string {
  return buildMode === "release" ? "正式构建" : "调试构建";
}

function platformLabel(platform: string): string {
  switch (platform) {
    case "windows":
      return "Windows";
    case "macos":
      return "macOS";
    case "linux":
      return "Linux";
    default:
      return platform;
  }
}

function architectureLabel(architecture: string): string {
  switch (architecture) {
    case "x86_64":
      return "x64";
    case "aarch64":
      return "ARM64";
    default:
      return architecture;
  }
}

function listenerLabel(transport: RuntimeOverview["transport"]): string {
  return transport.kind === "webDevelopment" ? `${transport.host}:${transport.port}` : "无 TCP 端口";
}

function responseLabel(transport: RuntimeOverview["transport"]): string {
  return transport.kind === "webDevelopment"
    ? `健康检查 HTTP ${transport.healthStatus}`
    : "桌面协议 · 已响应";
}

/** A compact, read-only footer for the application process itself. */
export function RuntimeOverviewPanel() {
  const [runtime, setRuntime] = useState<RuntimeOverview | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let active = true;

    void getRuntimeOverview()
      .then((overview) => {
        if (active) setRuntime(overview);
      })
      .catch((reason) => {
        if (active) setError(errorMessage(reason));
      });

    return () => {
      active = false;
    };
  }, []);

  return (
    <section className="asb-panel" aria-labelledby="runtime-overview-heading">
      <ModuleHeader id="runtime-overview-heading" title="运行环境" />
      {runtime === null && error === null && (
        /* Six fact lines at the fact list's footprint, never a one-line note. */
        <div className="asb-runtime-overview-skeleton" role="status" aria-label="正在读取运行环境">
          {Array.from({ length: 6 }, (_, line) => (
            <span key={line} className="asb-skeleton" aria-hidden="true" />
          ))}
        </div>
      )}
      {error && (
        <p className="asb-warn-text" role="alert">
          无法读取运行环境：{error}
        </p>
      )}
      {runtime !== null && (
        <dl className="asb-fact-row">
          <div>
            <dt>应用版本</dt>
            <dd>v{runtime.appVersion}</dd>
          </div>
          <div>
            <dt>构建模式</dt>
            <dd>{buildModeLabel(runtime.buildMode)}</dd>
          </div>
          <div>
            <dt>运行平台</dt>
            <dd>
              {platformLabel(runtime.platform)} · {architectureLabel(runtime.architecture)}
            </dd>
          </div>
          <div>
            <dt>监听端口</dt>
            <dd className="asb-code">{listenerLabel(runtime.transport)}</dd>
          </div>
          <div>
            <dt>响应情况</dt>
            <dd>{responseLabel(runtime.transport)}</dd>
          </div>
          <div>
            <dt>应用数据</dt>
            <dd>
              <FactPath path={runtime.appDataPath} open={openAppDataDir} />
            </dd>
          </div>
        </dl>
      )}
    </section>
  );
}
