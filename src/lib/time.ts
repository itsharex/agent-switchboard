/**
 * Formats an RFC 3339 UTC timestamp in the machine's local timezone as
 * `YYYY年MM月DD日 HH：MM`. Rendering goes through the `Time` component, the
 * single consumer of this formatter.
 */
export function timeLabel(iso: string): string {
  const date = new Date(iso);
  const [y, mo, d, h, mi] = [
    date.getFullYear(),
    date.getMonth() + 1,
    date.getDate(),
    date.getHours(),
    date.getMinutes(),
  ].map((part) => String(part).padStart(2, "0"));
  return `${y}年${mo}月${d}日 ${h}：${mi}`;
}

/**
 * A coarse human countdown to an RFC 3339 timestamp, computed against the
 * current time at render. There is no timer: a refresh re-renders the label.
 */
export function countdownLabel(iso: string, now = Date.now()): string {
  const remaining = new Date(iso).getTime() - now;
  if (remaining <= 0) return "已到重置时间";
  const totalMinutes = Math.floor(remaining / 60_000);
  const days = Math.floor(totalMinutes / (60 * 24));
  const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
  const minutes = totalMinutes % 60;
  if (totalMinutes < 1) return "不到 1 分钟后";
  if (days >= 1) return `约 ${days} 天 ${hours} 小时后`;
  if (hours >= 1) return `约 ${hours} 小时 ${minutes} 分钟后`;
  return `约 ${minutes} 分钟后`;
}

/**
 * A coarse relative label for an RFC 3339 timestamp, signed by direction
 * ("8 天前" / "约 45 小时后"), computed against the current time at render.
 * Like the countdown label there is no timer: a refresh re-renders it.
 */
export function relativeLabel(iso: string, now = Date.now()): string {
  const delta = new Date(iso).getTime() - now;
  if (Math.abs(delta) < 60_000) return "刚刚";
  const suffix = delta >= 0 ? "后" : "前";
  const totalMinutes = Math.floor(Math.abs(delta) / 60_000);
  const days = Math.floor(totalMinutes / (60 * 24));
  const hours = Math.floor((totalMinutes % (60 * 24)) / 60);
  const minutes = totalMinutes % 60;
  if (days >= 1) return `${days} 天 ${hours} 小时${suffix}`;
  if (hours >= 1) return `${hours} 小时 ${minutes} 分钟${suffix}`;
  return `${minutes} 分钟${suffix}`;
}
