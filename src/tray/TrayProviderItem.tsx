import { Check } from "lucide-react";
import type { TraySnapshot } from "../api/client";
import { useI18n } from "../i18n";
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
  const { t } = useI18n();
  const { readings, stale, stateKey } = trayUsageContent(provider.usage);
  const balance = [
    ...readings.map((reading) => `${reading.planName ?? ""} ${formatTrayReading(reading)}`),
    stateKey ? t(stateKey) : null,
    stale ? t("tray.state.stale") : null,
  ].filter(Boolean).join("，");
  const action = provider.active
    ? t("tray.provider.active", { name: provider.name })
    : t("tray.provider.switchTo", { name: provider.name });

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
        {provider.active ? null : switchingId === provider.id ? t("tray.provider.switching") : t("tray.provider.switch")}
      </span>
    </button>
  );
}
