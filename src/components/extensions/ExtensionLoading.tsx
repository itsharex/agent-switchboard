import { useI18n } from "../../i18n";

/** Loading placeholder that reserves the final list's footprint (S7): rows at
 * list-row height instead of a one-line note, announced as one status region. */
export function ExtensionLoading({ rows = 4 }: { rows?: number }) {
  const { t } = useI18n();
  return (
    <div className="asb-ext-loading" role="status" aria-label={t("extensions.loading.aria")}>
      {Array.from({ length: rows }, (_, index) => (
        <span key={index} className="asb-skeleton" aria-hidden="true" />
      ))}
    </div>
  );
}
