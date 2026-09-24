import type { KeyChange } from "../api/client";
import { useI18n } from "../i18n";

interface Props {
  changes: KeyChange[];
  /** Accessible name for the change list; names the surface it serves. */
  label: string;
}

/**
 * The single owner of change-row diff rendering (DESIGN.md §8): one change
 * per row — the key plus its red outgoing → green incoming line in code
 * style on a near-solid backdrop. Every diff surface (switch preview,
 * backup diff) consumes this component. The arrow and the 移除 word keep
 * each change legible without relying on color alone.
 */
export function DiffView({ changes, label }: Props) {
  const { t } = useI18n();
  return (
    <ul className="asb-diff" aria-label={label}>
      {changes.map((change) => (
        <li key={change.key} className="asb-diff-row">
          <span className="asb-diff-key">{change.key}</span>
          <span className="asb-diff-value">
            {change.before === null ? (
              <span className="asb-diff-none">{t("providers.diff.none")}</span>
            ) : (
              <span className="asb-diff-old">{change.before}</span>
            )}{" "}
            <span className="asb-diff-arrow" aria-hidden="true">
              →
            </span>{" "}
            {change.after === null ? (
              <span className="asb-diff-remove">{t("providers.diff.removed")}</span>
            ) : (
              <span className="asb-diff-new">{change.after}</span>
            )}
          </span>
        </li>
      ))}
    </ul>
  );
}
