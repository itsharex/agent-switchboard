import { useMessageState } from "../i18n/use-message-state";
import { useEffect, useState } from "react";
import { getRuntimeOverview, openAppDataDir, type RuntimeOverview } from "../api/client";
import { useI18n, type TFunction } from "../i18n";
import { FactPath } from "./FactPath";
import { ModuleHeader } from "./WorkspaceHeader";

function buildModeLabel(buildMode: RuntimeOverview["buildMode"], t: TFunction): string {
  return buildMode === "release" ? t("providers.runtime.build.release") : t("providers.runtime.build.debug");
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

function listenerLabel(transport: RuntimeOverview["transport"], t: TFunction): string {
  return transport.kind === "webDevelopment" ? `${transport.host}:${transport.port}` : t("providers.runtime.noTcpPort");
}

function responseLabel(transport: RuntimeOverview["transport"], t: TFunction): string {
  return transport.kind === "webDevelopment"
    ? t("providers.runtime.health", { status: transport.healthStatus })
    : t("providers.runtime.desktop");
}

/** A compact, read-only footer for the application process itself. */
export function RuntimeOverviewPanel() {
  const { t } = useI18n();
  const [runtime, setRuntime] = useState<RuntimeOverview | null>(null);
  const [error, setError] = useMessageState();

  useEffect(() => {
    let active = true;

    void getRuntimeOverview()
      .then((overview) => {
        if (active) setRuntime(overview);
      })
      .catch((reason) => {
        if (active) setError(reason);
      });

    return () => {
      active = false;
    };
  }, []);

  return (
    <section className="asb-panel" aria-labelledby="runtime-overview-heading">
      <ModuleHeader id="runtime-overview-heading" title={t("providers.runtime.title")} />
      {runtime === null && error === null && (
        /* Six fact lines at the fact list's footprint, never a one-line note. */
        <div className="asb-runtime-overview-skeleton" role="status" aria-label={t("providers.runtime.loading")}>
          {Array.from({ length: 6 }, (_, line) => (
            <span key={line} className="asb-skeleton" aria-hidden="true" />
          ))}
        </div>
      )}
      {error && (
        <p className="asb-warn-text" role="alert">
          {t("providers.runtime.loadFailed", { detail: error })}
        </p>
      )}
      {runtime !== null && (
        <dl className="asb-fact-row">
          <div>
            <dt>{t("providers.runtime.appVersion")}</dt>
            <dd>v{runtime.appVersion}</dd>
          </div>
          <div>
            <dt>{t("providers.runtime.buildMode")}</dt>
            <dd>{buildModeLabel(runtime.buildMode, t)}</dd>
          </div>
          <div>
            <dt>{t("providers.runtime.platform")}</dt>
            <dd>
              {platformLabel(runtime.platform)} · {architectureLabel(runtime.architecture)}
            </dd>
          </div>
          <div>
            <dt>{t("providers.runtime.listener")}</dt>
            <dd className="asb-code">{listenerLabel(runtime.transport, t)}</dd>
          </div>
          <div>
            <dt>{t("providers.runtime.response")}</dt>
            <dd>{responseLabel(runtime.transport, t)}</dd>
          </div>
          <div>
            <dt>{t("providers.runtime.appData")}</dt>
            <dd>
              <FactPath path={runtime.appDataPath} open={openAppDataDir} />
            </dd>
          </div>
        </dl>
      )}
    </section>
  );
}
