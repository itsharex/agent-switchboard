import { useI18n } from "../../i18n";
import { Select } from "../Select";
import { Input } from "../Input";
import type { SessionSearch } from "./use-session-search";

function dateBoundary(value: string, nextDay: boolean) {
  if (!value) return null;
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || Number(value.slice(0, 4)) === 0) return false;
  const date = new Date(`${value}T00:00:00`);
  if (!Number.isFinite(date.getTime()) || date.getMonth() + 1 !== Number(value.slice(5, 7))
    || date.getDate() !== Number(value.slice(8, 10))) return false;
  if (nextDay) date.setDate(date.getDate() + 1);
  if (date.getFullYear() > 9999) return false;
  return date.toISOString();
}

function SessionTimeFilter({ search, disabled }: { search: SessionSearch; disabled: boolean }) {
  const { t } = useI18n();
  const { range, from, to } = search.timeDraft;
  const invalidDate = dateBoundary(from, false) === false || dateBoundary(to, true) === false;
  const invalid = Boolean(from && to && from > to);
  const apply = (start: string, end: string) => {
    const activeAfter = dateBoundary(start, false), activeBefore = dateBoundary(end, true);
    if (activeAfter !== false && activeBefore !== false && (!start || !end || start <= end)) {
      search.changeDates({ activeAfter, activeBefore });
    }
  };
  const change = (value: string) => {
    search.setTimeDraft({ range: value, from, to });
    if (value === "custom") {
      apply(from, to);
    } else search.changeDates({ activeAfter: value === "all" ? null
      : new Date(Date.now() - Number(value) * 86400000).toISOString(), activeBefore: null });
  };
  const update = (start: string, end: string) => {
    search.setTimeDraft({ range, from: start, to: end });
    apply(start, end);
  };
  return <>
    <Select value={range} ariaLabel={t("sessions.time.range")} disabled={disabled} onChange={change} options={[
      { value: "all", label: t("sessions.time.all") }, { value: "7", label: t("sessions.time.week") },
      { value: "30", label: t("sessions.time.month") }, { value: "custom", label: t("sessions.time.custom") },
    ]} />
    {range === "custom" && <div className="asb-session-date-range">
      <label>{t("sessions.time.from")}<Input type="date" value={from} disabled={disabled} aria-invalid={invalidDate || invalid}
        onChange={(event) => update(event.target.value, to)} /></label>
      <label>{t("sessions.time.to")}<Input type="date" value={to} disabled={disabled} aria-invalid={invalidDate || invalid}
        onChange={(event) => update(from, event.target.value)} /></label>
      {(invalidDate || invalid) && <p className="asb-warn-text" role="alert">{t(invalidDate ? "sessions.time.invalidDate" : "sessions.time.invalid")}</p>}
    </div>}
  </>;
}

export function SessionOrganizationFilters({ search, disabled }: { search: SessionSearch; disabled: boolean }) {
  const { t } = useI18n();
  return <>
    <SessionTimeFilter search={search} disabled={disabled} />
    <Select value={search.tag === null ? "all" : `tag:${search.tag}`} ariaLabel={t("sessions.organize.tagFilter")}
      disabled={disabled} onChange={(value) => search.changeTag(value === "all" ? null : value.slice(4))}
      options={[{ value: "all", label: t("sessions.organize.allTags") },
        ...[...new Set([...search.tags, ...(search.tag ? [search.tag] : [])])].map((tag) => ({ value: `tag:${tag}`, label: tag }))]} />
    <label className="asb-session-pin-filter"><input type="checkbox" checked={search.pinnedOnly} disabled={disabled}
      onChange={(event) => search.changePinned(event.target.checked)} />{t("sessions.organize.pinnedOnly")}</label>
  </>;
}
