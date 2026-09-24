import { useEffect, useState } from "react";
import {
  closeWindow,
  getWindowMaximized,
  minimizeWindow,
  onWindowResized,
  toggleMaximizeWindow,
} from "../api/client";
import { useI18n } from "../i18n";
import { CloseIcon, MaximizeIcon, MinimizeIcon, RestoreIcon } from "./icons";
import { Button } from "./Button";

/** Custom window controls for the undecorated, integrated title bar
 * (PC Manager-style). All Tauri access goes through the api client. The
 * maximize button mirrors the live window state: single square while
 * windowed, overlapping squares while maximized. */
export function WindowControls() {
  const [maximized, setMaximized] = useState(false);
  const { t } = useI18n();

  useEffect(() => {
    let active = true;
    const sync = () => {
      void getWindowMaximized()
        .then((value) => {
          if (active) setMaximized(value);
        })
        .catch(() => {});
    };
    sync();
    const unlisten = onWindowResized(sync).catch(() => () => {});
    return () => {
      active = false;
      void unlisten.then((stop) => stop());
    };
  }, []);

  return (
    <div className="asb-wincontrols">
      <Button
        variant="unstyled"
        className="asb-winbtn"
        aria-label={t("window.minimize")}
        onClick={() => void minimizeWindow().catch(() => {})}
      >
        <MinimizeIcon />
      </Button>
      <Button
        variant="unstyled"
        className="asb-winbtn"
        aria-label={maximized ? t("window.restore") : t("window.maximize")}
        onClick={() => void toggleMaximizeWindow().catch(() => {})}
      >
        {maximized ? <RestoreIcon /> : <MaximizeIcon />}
      </Button>
      <Button
        variant="unstyled"
        className="asb-winbtn asb-winbtn-close"
        aria-label={t("window.close")}
        onClick={() => void closeWindow().catch(() => {})}
      >
        <CloseIcon />
      </Button>
    </div>
  );
}
