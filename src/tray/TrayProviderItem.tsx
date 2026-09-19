import { Check } from "lucide-react";
import type { TraySnapshot } from "../api/client";
import { formatUsageBalance } from "../lib/usage-format";
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
  const usage = provider.usage;
  const balance = usage ? formatUsageBalance(usage) : null;
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
      {balance && <span className="tray-provider-balance">{balance}</span>}
      <span className="tray-provider-switch">
        {provider.active ? null : switchingId === provider.id ? "切换中" : "切换"}
      </span>
    </button>
  );
}
