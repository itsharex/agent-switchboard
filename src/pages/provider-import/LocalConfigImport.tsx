import { openConfigFileLocation, type AppKind, type ClaudeImportProposal, type CodexImportProposal, type DiscoveredFile, type DiscoveryReport } from "../../api/client";
import { Button } from "../../components/Button";
import { FactPath } from "../../components/FactPath";
import { ModuleHeader } from "../../components/WorkspaceHeader";
import { SearchIcon } from "../../components/icons";
import { useI18n, type TFunction } from "../../i18n";
import { clientName } from "../../lib/client-name";

interface LocalConfigImportProps {
  app: AppKind;
  discovery: DiscoveryReport | null;
  busy: boolean;
  onScan: () => void;
  onImport: () => void;
}

function stateLabel(file: DiscoveredFile, t: TFunction): string {
  switch (file.state.kind) {
    case "ok": return t("importDiscovery.local.state.ok");
    case "missing": return t("importDiscovery.local.state.missing");
    case "readError": return t("importDiscovery.local.state.readError");
    case "parseError": return t("importDiscovery.local.state.parseError");
  }
}

function LocalRouteFacts({ file }: { file: DiscoveredFile }) {
  const { t } = useI18n();
  if (file.state.kind !== "ok") return null;
  const { route, managed, warnings, importable } = file.state;
  return (
    <>
      <div><dt>{t("importDiscovery.local.currentService")}</dt><dd>
        {route.routeMode === "official" ? t("importDiscovery.label.official") : t("importDiscovery.local.customService")} · {route.model ?? t("importDiscovery.local.defaultModel")}
      </dd></div>
      {route.providerName && <div><dt>{t("importDiscovery.label.provider")}</dt><dd>{route.providerName}</dd></div>}
      {route.baseUrl && <div><dt>{t("importDiscovery.local.serviceUrl")}</dt><dd className="asb-code">{route.baseUrl}</dd></div>}
      {route.apiKey && <div><dt>{t("importDiscovery.local.credentialVar")}</dt><dd className="asb-code">{route.apiKey}</dd></div>}
      <div><dt>{t("importDiscovery.local.managedState")}</dt><dd>{managed ? t("importDiscovery.local.managed") : t("importDiscovery.local.unmanaged")}</dd></div>
      {(warnings.length > 0 || (!importable && !managed)) && (
        <div><dt>{t("importDiscovery.local.warnings")}</dt><dd>
          {warnings.map((warning) => <span key={warning} className="asb-warn-text asb-status-warn">{warning}</span>)}
          {!importable && !managed && <span className="asb-warn-text asb-status-warn">{t("importDiscovery.local.unimportable")}</span>}
        </dd></div>
      )}
    </>
  );
}

function LocalConfigCard({ file, proposal, busy, onImport }: {
  file: DiscoveredFile;
  proposal: ClaudeImportProposal | CodexImportProposal | undefined;
  busy: boolean;
  onImport: () => void;
}) {
  const { t } = useI18n();
  return (
    <article className="asb-client-status" aria-label={t("importDiscovery.local.cardAria", { client: clientName(file.app) })}>
      <header className="asb-client-status-head">
        <span className="asb-client-status-name">{clientName(file.app)}</span>
        <span className={`asb-status-pill${file.state.kind === "ok" ? " is-ok" : ""}`}>
          <span className="asb-status-pill-dot" aria-hidden="true" />{stateLabel(file, t)}
        </span>
      </header>
      <dl className="asb-fact-row">
        <div><dt>{t("importDiscovery.local.configFile")}</dt><dd><FactPath path={file.path} open={() => openConfigFileLocation(file.app)} /></dd></div>
        {file.state.kind === "readError" && (
          <div><dt>{t("importDiscovery.local.errorDetail")}</dt><dd className="asb-warn-text">{file.state.message}</dd></div>
        )}
        {file.state.kind === "parseError" && (
          <div><dt>{t("importDiscovery.local.errorDetail")}</dt><dd className="asb-warn-text">
            {file.state.line !== null ? t("importDiscovery.local.errorLine", { line: file.state.line }) : ""}{file.state.message}
          </dd></div>
        )}
        <LocalRouteFacts file={file} />
      </dl>
      {proposal && <div className="asb-discovery-import">
        <p className="asb-discovery-basis">{proposal.basis}</p>
        <Button variant="secondary" disabled={busy} onClick={onImport}>{t("importDiscovery.local.importProvider")}</Button>
      </div>}
    </article>
  );
}

export function LocalConfigImport({ app, discovery, busy, onScan, onImport }: LocalConfigImportProps) {
  const { t } = useI18n();
  return (
    <>
      <ModuleHeader
        title={t("importDiscovery.tab.local")}
        primaryActions={
          <Button variant="secondary" disabled={busy} onClick={onScan}>{discovery ? t("importDiscovery.local.scanRefresh") : t("importDiscovery.local.scan")}</Button>
        }
      />
      {discovery ? <LocalConfigCard file={discovery[app]} busy={busy} onImport={onImport}
        proposal={app === "claude" ? discovery.claudeImportProposals[0] : discovery.codexImportProposals[0]} />
        : (
          <div className="asb-empty-state">
            <span className="asb-empty-state-icon" aria-hidden="true">
              <SearchIcon />
            </span>
            <h3 className="asb-section-title">{t("importDiscovery.local.notScanned", { client: clientName(app) })}</h3>
          </div>
        )}
    </>
  );
}
