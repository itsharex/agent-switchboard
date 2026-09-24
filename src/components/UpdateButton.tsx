import { useI18n } from "../i18n";
import { UpdateIcon } from "./icons";
import { Button } from "./Button";
import { Tooltip } from "./Tooltip";

/** Topbar indicator rendered only while a newer release is known. Stateless
 * by design: the check result stays with App; clicking leads to the settings
 * page, where the release link lives. */
export function UpdateButton({
  latestVersion,
  onOpen,
}: {
  latestVersion: string;
  onOpen: () => void;
}) {
  const { t } = useI18n();
  const label = t("update.available", { version: latestVersion });
  return (
    <Tooltip label={label} side="bottom">
      <span className="asb-tooltip-anchor">
        <Button
          variant="unstyled"
          className="asb-winbtn asb-updatebtn"
          aria-label={label}
          onClick={onOpen}
        >
          <UpdateIcon />
          <span className="asb-updatebtn-label">{t("update.button")}</span>
          <span className="asb-updatebtn-dot" aria-hidden="true" />
        </Button>
      </span>
    </Tooltip>
  );
}
