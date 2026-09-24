import type { CcSwitchImportOutcome } from "../api/client";
import { useI18n, type TFunction } from "../i18n";

export function ccOutcomeNeedsAttention(result: CcSwitchImportOutcome): boolean {
  return result.importedCount === 0 || result.notImported.length > 0;
}

function ccOutcomeSummary(result: CcSwitchImportOutcome, t: TFunction): string {
  return [
    t("importDiscovery.result.imported", { count: result.importedCount }),
    result.usageScriptImportedCount > 0 ? t("importDiscovery.result.usageScripts", { count: result.usageScriptImportedCount }) : null,
    result.endpointCandidatesImported > 0 ? t("importDiscovery.result.endpointCandidates", { count: result.endpointCandidatesImported }) : null,
    result.skippedExisting.length > 0 ? t("importDiscovery.result.skippedExisting", { count: result.skippedExisting.length }) : null,
    result.notImported.length > 0 ? t("importDiscovery.result.notImported", { count: result.notImported.length }) : null,
  ].filter(Boolean).join(" · ");
}

/** Both the banner and the toast follow the current interface language. */
export function CcOutcomeText({ result }: { result: CcSwitchImportOutcome }) {
  const { t } = useI18n();
  return <>{ccOutcomeSummary(result, t)}</>;
}
