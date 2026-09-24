import { useI18n } from "../i18n";
import { PinIcon } from "./icons";
import { Button } from "./Button";
import { Tooltip } from "./Tooltip";

/** Topbar quick toggle for the「窗口始终置顶」preference. Stateless by
 * design: the active value and the save path stay with the AppSettings
 * owner in App, exactly like the settings-page checkbox. */
export function PinTopButton({
  active,
  disabled,
  onToggle,
}: {
  active: boolean;
  disabled: boolean;
  onToggle: () => void;
}) {
  const { t } = useI18n();
  const label = active ? t("pin.unpin") : t("pin.pin");
  return (
    <Tooltip label={label} side="bottom">
      <span className="asb-tooltip-anchor">
        <Button
          variant="unstyled"
          className={`asb-winbtn asb-pinbtn${active ? " is-active" : ""}`}
          aria-label={label}
          aria-pressed={active}
          disabled={disabled}
          onClick={onToggle}
        >
          <PinIcon />
        </Button>
      </span>
    </Tooltip>
  );
}
