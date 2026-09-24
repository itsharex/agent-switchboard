import type { ClientCapabilityReport } from "../../api/client";
import { useI18n } from "../../i18n";
import { catalogText } from "../../i18n/errors";
import { clientName } from "../../lib/client-name";
import { DashIcon } from "../icons";

interface Props {
  reports: ClientCapabilityReport[];
}

/** Read-only capability audit for both clients. It is secondary diagnostic
 * detail, so it stays collapsed until the user asks to inspect it. */
export function CapabilityPanel({ reports }: Props) {
  const { t } = useI18n();
  return (
    <details className="asb-ext-disclosure asb-ext-capabilities" aria-label={t("extensions.capabilities.title")}>
      <summary>
        <span className="asb-section-title">{t("extensions.capabilities.title")}</span>
        <span className="asb-scope-note">{t("extensions.capabilities.count", { count: reports.length })}</span>
      </summary>
      <div className="asb-ext-capability-grid">
        {reports.map((report) => (
          <section key={report.client} aria-label={t("extensions.capabilities.clientAria", { client: clientName(report.client) })}>
            <h4 className="asb-group-title">{clientName(report.client)}</h4>
            {report.entries.length === 0 ? (
              <div className="asb-empty-state">
                <span className="asb-empty-state-icon" aria-hidden="true">
                  <DashIcon />
                </span>
                <h3 className="asb-section-title">{t("extensions.capabilities.empty")}</h3>
              </div>
            ) : (
              <ul className="asb-ext-capability-list">
                {report.entries.map((entry) =>
                  entry.verification === "verified" ? (
                    <li key={entry.code}>
                      <span>{catalogText(entry.resource, t)}</span>{" "}
                      {entry.supported ? (
                        <span className="asb-ok-text">{t("extensions.capabilities.verified", { version: entry.clientVersion })}</span>
                      ) : (
                        <span className="asb-warn-text">{t("extensions.capabilities.unverified")}</span>
                      )}
                    </li>
                  ) : (
                    <li key={entry.code}>
                      <span>{catalogText(entry.resource, t)}</span>{" "}
                      <span className="asb-warn-text">{t("extensions.capabilities.unverifiedReason", { condition: catalogText(entry.condition, t) })}</span>
                    </li>
                  ),
                )}
              </ul>
            )}
          </section>
        ))}
      </div>
    </details>
  );
}
