import { Check } from "lucide-react";
import type { TraySnapshot } from "../api/client";
import { formatTrayReading } from "../lib/usage-format";
import { TrayUsage, trayUsageContent } from "./TrayUsage";
import { cx } from "../utils/cx";

type TrayProvider = TraySnapshot["providers"][number];

interface Props {
  provider: TrayProvider;
  busy: boolean;
  switchingId: string | null;
  onSwitch: () => void;
}

/** One provider is one native full-width button row; tray.css owns every
 * visual property. The shared Button is deliberately not used here: its
 * geometry contract targets global action controls, not list rows. */
export function TrayProviderItem({ provider, busy, switchingId, onSwitch }: Props) {
  const { readings, stale, state } = trayUsageContent(provider.usage);
  const balance = [...readings.map((reading) => `${reading.planName ?? ""} ${formatTrayReading(reading)}`),
    state, stale ? "上次读数" : null].filter(Boolean).join("，");
  const action = provider.active
    ? `${provider.name}，当前供应商`
    : `切换到 ${provider.name}`;

  return (
    <button
      type="button"
      className={cx("tray-provider", provider.active && "is-active", switchingId === provider.id && "is-switching")}
      disabled={busy || provider.active}
      aria-label={balance ? `${action}，${balance}` : action}
      onClick={onSwitch}
    >
      <span className="tray-provider-check">{provider.active && <Check size={18} aria-hidden="true" />}</span>
      <span className="tray-provider-name">{provider.name}</span>
      <TrayUsage usage={provider.usage} />
      <span className="tray-provider-switch">
        {provider.active ? null : switchingId === provider.id ? "切换中" : "切换"}
      </span>
    </button>
  );
}
