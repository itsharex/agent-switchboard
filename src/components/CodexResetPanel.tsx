import { openUrl } from "@tauri-apps/plugin-opener";
import {
  type CodexResetHeatmap,
  type CodexResetHeatmapDay,
  type ResetSignal,
  type ResetType,
} from "../api/client";
import { Button } from "./Button";
import { Time } from "./Time";
import { relativeLabel } from "../lib/time";
import { UpdateIcon } from "./icons";
import { ModuleHeader } from "./WorkspaceHeader";
import { useCodexResetSignal } from "./quota-reads";

const DAY_MS = 86_400_000;

function resetTypeLabel(type: ResetType): string {
  switch (type) {
    case "global":
      return "全局重置";
    case "banked":
      return "重置卡";
    case "other":
      return "重置";
  }
}

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

function scheduleDescription(signal: ResetSignal): string {
  if (signal.effectiveAt === null) return "已公告，但未提供预计时间";
  return signal.schedulePrecision === "date" ? "日期级预告" : "精确时间预告";
}

function confidenceLabel(confidence: number): string {
  return `信心 ${Math.round(confidence * 100)}%`;
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
  return (
    <span className={`asb-codex-reset-badge${resetTypeBadgeClass(type)}`}>
      {resetTypeLabel(type)}
    </span>
  );
}

/** The feed's UTC-day history as a contribution-style grid. The grid anchors
 * on the newest known day (a scheduled future day included) and always spans
 * the feed's week count; days after the anchor stay invisible placeholders. */
function ResetHeatmap({ heatmap }: { heatmap: CodexResetHeatmap }) {
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
      aria-label={`Codex 重置热力图：近 ${weeks} 周共 ${heatmap.total} 条公开重置信号（${heatmap.timezone} 时区）`}
    >
      <div className="asb-codex-reset-heatmap-head">
        <p className="asb-codex-reset-heatmap-title">Codex 重置热力图</p>
        <p className="asb-codex-reset-heatmap-total">
          共 {heatmap.total} 条重置信号 · 近 {weeks} 周 · {heatmap.timezone}
        </p>
      </div>
      <div className="asb-codex-reset-heatmap-grid" aria-hidden="true">
        {cells.map((cell) => (
          <span
            key={cell.key}
            className={`asb-codex-reset-heatmap-cell${cell.day ? ` is-level-${cell.day.level}` : ""}${
              cell.hidden ? " is-hidden" : ""
            }`}
            title={cell.day ? `${cell.key}：${cell.day.count} 条信号` : undefined}
          />
        ))}
      </div>
      <div className="asb-codex-reset-heatmap-legend" aria-hidden="true">
        <span>少</span>
        <span className="asb-codex-reset-heatmap-cell" />
        <span className="asb-codex-reset-heatmap-cell is-level-1" />
        <span className="asb-codex-reset-heatmap-cell is-level-2" />
        <span className="asb-codex-reset-heatmap-cell is-level-3" />
        <span className="asb-codex-reset-heatmap-cell is-level-4" />
        <span>多</span>
      </div>
    </div>
  );
}

/** An explicit, read-only view of public reset signals; the refresh action
 * and freshness state live in the usage page header. */
export function CodexResetPanel({ read }: { read: ReturnType<typeof useCodexResetSignal> }) {
  const { snapshot, cacheLoading, cacheError, readError } = read;

  const status = snapshot?.status ?? null;
  const cacheWarning = snapshot?.cacheWarning ?? null;
  const isCachedRead = snapshot?.freshness === "cached";
  const forecast = status?.nextScheduledReset ?? null;
  const latestCompleted = status?.latestConfirmedSignal ?? null;
  const forecastResult = forecast !== null ? forecastWindow(forecast) : null;

  return (
    <section className="asb-panel asb-codex-reset" aria-labelledby="codex-reset-heading">
      <ModuleHeader id="codex-reset-heading" title="Codex 重置信号" />
      {cacheLoading && status === null && <p className="asb-empty" role="status">正在读取本地缓存</p>}
      {status === null && !cacheLoading && readError === null && (
        <div className="asb-empty-state">
          <span className="asb-empty-state-icon" aria-hidden="true">
            <UpdateIcon />
          </span>
          <h3 className="asb-section-title">尚无本地缓存。手动刷新以读取公开重置信号。</h3>
        </div>
      )}
      {cacheError && <p className="asb-warn-text" role="alert">本地缓存不可用：{cacheError}</p>}
      {readError && (
        <p className="asb-warn-text" role="alert">
          无法刷新公开重置信号：{readError}
          {status !== null ? "；仍在显示上次成功读取的数据。" : ""}
        </p>
      )}
      {status !== null && (
        <>
          <div className="asb-codex-reset-board">
            <article className="asb-codex-reset-summary" aria-label="下一次 Codex 重置预告">
              <p className="asb-codex-reset-question">接下来会有 Codex 重置吗？</p>
              {forecast !== null ? (
                <>
                  <strong className="asb-codex-reset-answer is-forecast">
                    {probabilityLabel(forecast.confidence)} 是
                  </strong>
                  {forecastResult !== null ? (
                    <p className="asb-codex-reset-detail">
                      高概率到账预告：<Time iso={forecastResult.start} />
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
                    <p className="asb-codex-reset-detail">{scheduleDescription(forecast)}。</p>
                  )}
                </>
              ) : (
                <>
                  <strong className="asb-codex-reset-answer is-none">否</strong>
                  <p className="asb-codex-reset-detail">公开 feed 尚未预告下一次重置。</p>
                </>
              )}
              {status.latestRelevantTiboPost && (
                <div className="asb-codex-reset-post-summary">
                  <p className="asb-codex-reset-label">Tibo 最近相关动态</p>
                  <p className="asb-codex-reset-post">{status.latestRelevantTiboPost.text}</p>
                  <div className="asb-codex-reset-post-actions">
                    <span className="asb-codex-reset-detail">
                      <Time iso={status.latestRelevantTiboPost.announcedAt} />
                    </span>
                    <Button
                      variant="secondary"
                      onClick={() => void openUrl(status.latestRelevantTiboPost!.url)}
                    >
                      查看原帖
                    </Button>
                  </div>
                </div>
              )}
            </article>
            <dl className="asb-codex-reset-facts" aria-label="公开信号详情">
              <div className="asb-codex-reset-fact">
                <dt>最近一次已完成重置</dt>
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
                    "暂无"
                  )}
                </dd>
              </div>
              <div className="asb-codex-reset-fact">
                <dt>最近检查</dt>
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
                <dt>预计下次重置</dt>
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
                        {scheduleDescription(forecast)} · {confidenceLabel(forecast.confidence)}
                      </span>
                    </>
                  ) : (
                    "暂无公告预计"
                  )}
                </dd>
              </div>
            </dl>
          </div>
          <ResetHeatmap heatmap={status.heatmap} />
          {status.sourceWarning && <p className="asb-warn-text">{status.sourceWarning}</p>}
          {cacheWarning && <p className="asb-warn-text">{cacheWarning}</p>}
          <p className="asb-codex-reset-source">
            公开 feed 生成于 <Time iso={status.generatedAt} /> · 最近成功检查 <Time iso={status.lastSuccessfulCheckAt} /> · {isCachedRead ? "缓存于" : "本次读取"} <Time iso={status.checkedAt} />
          </p>
          <p className="asb-codex-reset-note">
            数据来自 Codex Runway 公开 feed：{" "}
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
            ，非 OpenAI 官方，不代表你的账号额度。
          </p>
        </>
      )}
    </section>
  );
}
