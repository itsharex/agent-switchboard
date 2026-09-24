import type { ProviderDiagnosticsReport } from "../../api/provider-diagnostics";
import { useI18n } from "../../i18n";
import { localizedMessageText } from "../../i18n/errors";

export function ProviderDiagnosticReport({ report }: { report: ProviderDiagnosticsReport }) {
  const { t } = useI18n();
  return <div className="asb-provider-diagnostic-report">
    <p className="asb-scope-note">{t("doctor.localNote")}</p>
    <dl className="asb-doctor-checks">
      {report.checks.map((check, index) => <div key={`${check.category}-${index}`} data-status={check.status}>
        <dt>{t(`doctor.${check.category}`)}<span>{t(`doctor.${check.status}`)}</span></dt>
        <dd><p>{localizedMessageText(check.message, t)}</p>
          {check.suggestion && <p className="asb-scope-note">{localizedMessageText(check.suggestion, t)}</p>}
        </dd>
      </div>)}
    </dl>
  </div>;
}
