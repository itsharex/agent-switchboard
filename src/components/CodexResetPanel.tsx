import { openUrl } from "@tauri-apps/plugin-opener";
import {
  type CodexResetHeatmap,
  type CodexResetHeatmapDay,
  type ResetSignal,
  type ResetType,
} from "../api/client";
import type { MessageKey, TFunction } from "../i18n";
import { useI18n } from "../i18n";
import { Button } from "./Button";
import { Time } from "./Time";
import { relativeLabel } from "../lib/time";
import { UpdateIcon } from "./icons";
import { ModuleHeader } from "./WorkspaceHeader";
import { useCodexResetSignal } from "./quota-reads";

const DAY_MS = 86_400_000;

const RESET_TYPE_LABELS: Record<ResetType, MessageKey> = {
  global: "codex.reset.typeGlobal",
  banked: "codex.reset.typeBanked",
  other: "codex.reset.typeOther",
};

function resetTypeBadgeClass(type: ResetType): string {
  switch (type) {
    case "global":
      return " is-global";
    case "banked":
      return " is-banked";
    case "other":
      return "";
  }
}

function scheduleDescription(signal: ResetSignal, t: TFunction): string {
  if (signal.effectiveAt === null) return t("codex.reset.noTimeAnnounced");
  return signal.schedulePrecision === "date" ? t("codex.reset.datePrecision") : t("codex.reset.exactPrecision");
}

function confidenceLabel(confidence: number, t: TFunction): string {
  return t("codex.reset.confidence", { value: Math.round(confidence * 100) });
}

/** The site buckets a schedule's confidence down to the nearest ten percent:
 * 0.7 reads as "≥70%", and anything from 0.9 up reads as "≥90%". */
function probabilityLabel(confidence: number): string {
  return `≥${Math.min(9, Math.floor(confidence * 10)) * 10}%`;
}

interface ForecastWindow {
  start: string;
  end: string | null;
}

/** A date-level schedule is an arrival window (the announced day), so it
 * renders as start ~ end like the public site does; a datetime schedule is a
 * single point. */
function forecastWindow(signal: ResetSignal): ForecastWindow | null {
  if (signal.effectiveAt === null) return null;
  const end =
    signal.schedulePrecision === "date"
      ? new Date(new Date(signal.effectiveAt).getTime() + DAY_MS - 60_000).toISOString()
      : null;
  return { start: signal.effectiveAt, end };
}

function Badge({ type }: { type: ResetType }) {
  const { t } = useI18n();
  return (
    <span className={`asb-codex-reset-badge${resetTypeBadgeClass(type)}`}>
      {t(RESET_TYPE_LABELS[type])}
    </span>
  );
}

/** The feed's UTC-day history as a contribution-style grid. The grid anchors
 * on the newest known day (a scheduled future day included) and always spans
 * the feed's week count; days after the anchor stay invisible placeholders. */
function ResetHeatmap({ heatmap }: { heatmap: CodexResetHeatmap }) {
  const { t } = useI18n();
  const levels = new Map(heatmap.days.map((day) => [day.date, day]));
  const todayKey = new Date().toISOString().slice(0, 10);
  const anchorKey = heatmap.days.reduce(
    (latest, day) => (day.date > latest ? day.date : latest),
    todayKey,
  );
  const anchorWeekday = new Date(`${anchorKey}T00:00:00Z`).getUTCDay();
  const gridEndTime = Date.parse(`${anchorKey}T00:00:00Z`) + (6 - anchorWeekday) * DAY_MS;
  const weeks = heatmap.weeks;
  const gridStartTime = gridEndTime - (weeks * 7 - 1) * DAY_MS;
  const cells: Array<{ key: string; day: CodexResetHeatmapDay | null; hidden: boolean }> = [];
  for (let column = 0; column < weeks; column += 1) {
    for (let row = 0; row < 7; row += 1) {
      const key = new Date(gridStartTime + (column * 7 + row) * DAY_MS).toISOString().slice(0, 10);
      cells.push({ key, day: levels.get(key) ?? null, hidden: key > anchorKey });
    }
  }

  return (
    <div
      className="asb-codex-reset-heatmap"
      role="img"
      aria-label={t("codex.reset.heatmapAria", { weeks, total: heatmap.total, timezone: heatmap.timezone })}
    >
      <div className="asb-codex-reset-heatmap-head">
        <p className="asb-codex-reset-heatmap-title">{t("codex.reset.heatmapTitle")}</p>
        <p className="asb-codex-reset-heatmap-total">
          {t("codex.reset.heatmapTotal", { total: heatmap.total, weeks, timezone: heatmap.timezone })}
        </p>
      </div>
      <div className="asb-codex-reset-heatmap-grid" aria-hidden="true">
        {cells.map((cell) => (
          <span
            key={cell.key}
            className={`asb-codex-reset-heatmap-cell${cell.day ? ` is-level-${cell.day.level}` : ""}${
              cell.hidden ? " is-hidden" : ""
            }`}
            title={cell.day ? t("codex.reset.heatmapCellTitle", { date: cell.key, count: cell.day.count }) : undefined}
          />
        ))}
      </div>
      <div className="asb-codex-reset-heatmap-legend" aria-hidden="true">
        <span>{t("codex.reset.few")}</span>
        <span className="asb-codex-reset-heatmap-cell" />
        <span className="asb-codex-reset-heatmap-cell is-level-1" />
        <span className="asb-codex-reset-heatmap-cell is-level-2" />
        <span className="asb-codex-reset-heatmap-cell is-level-3" />
        <span className="asb-codex-reset-heatmap-cell is-level-4" />
        <span>{t("codex.reset.many")}</span>
      </div>
    </div>
  );
}

/** An explicit, read-only view of public reset signals; the refresh action
 * and freshness state live in the usage page header. */
export function CodexResetPanel({ read }: { read: ReturnType<typeof useCodexResetSignal> }) {
  const { t } = useI18n();
  const { snapshot, cacheLoading, cacheError, readError } = read;

  const status = snapshot?.status ?? null;
  const cacheWarning = snapshot?.cacheWarning ?? null;
  const isCachedRead = snapshot?.freshness === "cached";
  const forecast = status?.nextScheduledReset ?? null;
  const latestCompleted = status?.latestConfirmedSignal ?? null;
  const forecastResult = forecast !== null ? forecastWindow(forecast) : null;

  return (
    <section className="asb-panel asb-codex-reset" aria-labelledby="codex-reset-heading">
      <ModuleHeader id="codex-reset-heading" title={t("codex.reset.title")} />
      {cacheLoading && status === null && <p className="asb-empty" role="status">{t("codex.reset.loadingCache")}</p>}
      {status === null && !cacheLoading && readError === null && (
        <div className="asb-empty-state">
          <span className="asb-empty-state-icon" aria-hidden="true">
            <UpdateIcon />
          </span>
          <h3 className="asb-section-title">{t("codex.reset.empty")}</h3>
        </div>
      )}
      {cacheError && <p className="asb-warn-text" role="alert">{t("codex.reset.cacheUnavailable", { error: cacheError })}</p>}
      {readError && (
        <p className="asb-warn-text" role="alert">
          {t("codex.reset.readFailed", { error: readError })}
          {status !== null ? t("codex.reset.showingStale") : ""}
        </p>
      )}
      {status !== null && (
        <>
          <div className="asb-codex-reset-board">
            <article className="asb-codex-reset-summary" aria-label={t("codex.reset.forecastAria")}>
              <p className="asb-codex-reset-question">{t("codex.reset.question")}</p>
              {forecast !== null ? (
                <>
                  <strong className="asb-codex-reset-answer is-forecast">
                    {t("codex.reset.answerYes", { probability: probabilityLabel(forecast.confidence) })}
                  </strong>
                  {forecastResult !== null ? (
                    <p className="asb-codex-reset-detail">
                      {t("codex.reset.highConfidenceArrival")}<Time iso={forecastResult.start} />
                      {forecastResult.end !== null && (
                        <>
                          {" ~ "}
                          <Time iso={forecastResult.end} />
                        </>
                      )}
                      （<span>{relativeLabel(forecastResult.start)}</span>
                      {forecastResult.end !== null && (
                        <>
                          {" ~ "}
                          <span>{relativeLabel(forecastResult.end)}</span>
                        </>
                      )}
                      ）
                    </p>
                  ) : (
                    <p className="asb-codex-reset-detail">{scheduleDescription(forecast, t)}。</p>
                  )}
                </>
              ) : (
                <>
                  <strong className="asb-codex-reset-answer is-none">{t("codex.reset.answerNo")}</strong>
                  <p className="asb-codex-reset-detail">{t("codex.reset.noForecast")}</p>
                </>
              )}
              {status.latestRelevantTiboPost && (
                <div className="asb-codex-reset-post-summary">
                  <p className="asb-codex-reset-label">{t("codex.reset.tiboLabel")}</p>
                  <p className="asb-codex-reset-post">{status.latestRelevantTiboPost.text}</p>
                  <div className="asb-codex-reset-post-actions">
                    <span className="asb-codex-reset-detail">
                      <Time iso={status.latestRelevantTiboPost.announcedAt} />
                    </span>
                    <Button
                      variant="secondary"
                      onClick={() => void openUrl(status.latestRelevantTiboPost!.url)}
                    >
                      {t("codex.reset.viewPost")}
                    </Button>
                  </div>
                </div>
              )}
            </article>
            <dl className="asb-codex-reset-facts" aria-label={t("codex.reset.factsAria")}>
              <div className="asb-codex-reset-fact">
                <dt>{t("codex.reset.lastCompleted")}</dt>
                <dd>
                  {latestCompleted !== null ? (
                    <>
                      <Badge type={latestCompleted.resetType} />
                      <span>
                        <Time iso={latestCompleted.announcedAt} />
                        <span className="asb-codex-reset-relative">
                          （{relativeLabel(latestCompleted.announcedAt)}）
                        </span>
                      </span>
                    </>
                  ) : (
                    t("codex.reset.none")
                  )}
                </dd>
              </div>
              <div className="asb-codex-reset-fact">
                <dt>{t("codex.reset.lastCheck")}</dt>
                <dd>
                  <span>
                    <Time iso={status.lastSuccessfulCheckAt} />
                    <span className="asb-codex-reset-relative">
                      （{relativeLabel(status.lastSuccessfulCheckAt)}）
                    </span>
                  </span>
                </dd>
              </div>
              <div className="asb-codex-reset-fact">
                <dt>{t("codex.reset.nextForecast")}</dt>
                <dd>
                  {forecast !== null ? (
                    <>
                      <Badge type={forecast.resetType} />
                      <span>
                        {forecastResult !== null && <Time iso={forecastResult.start} />}
                        {forecastResult?.end != null && (
                          <>
                            {" ~ "}
                            <Time iso={forecastResult.end} />
                          </>
                        )}
                      </span>
                      <span className="asb-codex-reset-detail">
                        {scheduleDescription(forecast, t)} · {confidenceLabel(forecast.confidence, t)}
                      </span>
                    </>
                  ) : (
                    t("codex.reset.noAnnouncement")
                  )}
                </dd>
              </div>
            </dl>
          </div>
          <ResetHeatmap heatmap={status.heatmap} />
          {status.sourceWarning && <p className="asb-warn-text">{status.sourceWarning}</p>}
          {cacheWarning && <p className="asb-warn-text">{cacheWarning}</p>}
          <p className="asb-codex-reset-source">
            {t("codex.reset.feedGenerated")} <Time iso={status.generatedAt} /> · {t("codex.reset.lastSuccessCheck")} <Time iso={status.lastSuccessfulCheckAt} /> · {isCachedRead ? t("codex.reset.cachedAt") : t("codex.reset.readThisTime")} <Time iso={status.checkedAt} />
          </p>
          <p className="asb-codex-reset-note">
            {t("codex.reset.sourceNoteLead")}{" "}
            <a
              className="asb-codex-reset-source-link"
              href={status.sourceUrl}
              onClick={(event) => {
                event.preventDefault();
                void openUrl(status.sourceUrl);
              }}
            >
              {status.sourceUrl}
            </a>
            {t("codex.reset.sourceNoteTail")}
          </p>
        </>
      )}
    </section>
  );
}
