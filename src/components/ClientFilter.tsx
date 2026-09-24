import { useId } from "react";
import type { AppKind } from "../api/client";
import { clientName } from "../lib/client-name";
import { useI18n } from "../i18n";
import { ClientLogo } from "./ClientLogo";

export type ClientFilterValue = "all" | AppKind;

const FILTER_OPTIONS: readonly ClientFilterValue[] = ["all", "codex", "claude"];

/**
 * The one client filter.
 *
 * "Which client am I looking at" is the same question in the extension
 * library and the session list, so it is the same control: the shared
 * `asb-segments` radio language, never a `tablist`. A tablist promises
 * panel-switching semantics that filtering does not have, and having both
 * idioms for one question was the reason two pages answered it differently.
 */
export function ClientFilter({
  value,
  onChange,
  label,
  showLogos = false,
}: {
  value: ClientFilterValue;
  onChange: (value: ClientFilterValue) => void;
  label: string;
  showLogos?: boolean;
}) {
  const { t } = useI18n();
  const name = useId();
  return (
    <div className="asb-segments asb-client-filter" role="radiogroup" aria-label={label}>
      {FILTER_OPTIONS.map((option) => (
        <label key={option} className={`asb-seg-opt${value === option ? " is-active" : ""}`}>
          <input
            type="radio"
            name={name}
            checked={value === option}
            onChange={() => onChange(option)}
          />
          {showLogos && option !== "all" && (
            <ClientLogo app={option} className="asb-seg-logo" />
          )}
          {option === "all" ? t("clientConfig.filter.all") : clientName(option)}
        </label>
      ))}
    </div>
  );
}
