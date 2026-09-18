import { Check } from "lucide-react";
import type { TraySnapshot } from "../api/client";
import { Button } from "../components/Button";
import { formatUsageBalance } from "../lib/usage-format";
import { cx } from "../utils/cx";

type TrayProvider = TraySnapshot["providers"][number];

interface Props {
  provider: TrayProvider;
  busy: boolean;
  pending: string | null;
  onSwitch: () => void;
}

export function TrayProviderItem({ provider, busy, pending, onSwitch }: Props) {
  const usage = provider.usage;
  const balance = usage ? formatUsageBalance(usage) : null;
  const action = provider.active
    ? `${provider.name}，当前供应商`
    : `切换到 ${provider.name}`;

  return (
    <article className={cx("tray-provider-item", provider.active && "is-active")}>
      <Button
        variant="secondary"
        className="tray-provider"
        disabled={busy || provider.active}
        aria-label={balance ? `${action}，${balance}` : action}
        onClick={onSwitch}
      >
        <span className="tray-check">{provider.active && <Check size={18} aria-hidden="true" />}</span>
        <span className="tray-provider-name">{provider.name}</span>
        {balance && <span className="tray-usage-balance">{balance}</span>}
        <span className="tray-switch">
          {provider.active ? null : pending === provider.id ? "切换中" : "切换"}
        </span>
      </Button>
    </article>
  );
}
