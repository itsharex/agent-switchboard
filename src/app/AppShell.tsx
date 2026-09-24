import type { ReactNode } from "react";
import type { CommandError } from "../api/client";
import { useI18n } from "../i18n";
import { commandErrorText } from "../i18n/errors";
import { PAGES, pageLabelKey, type Page } from "./navigation";
import { PinTopButton } from "../components/PinTopButton";
import { UpdateButton } from "../components/UpdateButton";
import { Button } from "../components/Button";
import { WindowControls } from "../components/WindowControls";
import appIcon from "../assets/app-icon.svg";
import { isBrowserDevelopment } from "../lib/runtime";

interface AppShellProps {
  page: Page;
  onPageChange: (page: Page) => void;
  /** Persistent decision errors only; one-shot feedback lives in the global
   * toaster. */
  error: CommandError | null;
  busy: boolean;
  /** Why application settings could not load; the banner keeps the repair
   * reachable from every page while settings-backed actions silently wait. */
  settingsError: string | null;
  /** Replaces an unreadable settings file with validated defaults. */
  onRepairSettings: () => void;
  /** Offered only for an unsupported profile store. */
  onRepairStore: () => void;
  /** Always-on-top toggle state; null until settings load. */
  pin: { active: boolean; onToggle: () => void } | null;
  /** Update indicator; present only while a newer release is known. */
  update: { latestVersion: string; onOpen: () => void } | null;
  children: ReactNode;
}

function OperationNotices({ error, settingsError, busy, onRepairStore, onRepairSettings }:
  Pick<AppShellProps, "error" | "settingsError" | "busy" | "onRepairStore" | "onRepairSettings">) {
  const { t } = useI18n();
  return (
    <div className="asb-banner-stack" aria-label={t("shell.banner.aria")}>
      {/* Persistent decision errors only: the banner carries the store
          repair entry. One-shot operation feedback lives in the global
          toaster (DESIGN.md §7/§8). */}
      {error && (
        <div className="asb-banner asb-banner-error" role="alert" aria-label={t("shell.banner.error.aria")}>
          <span>{commandErrorText(error, t)}</span>
          {(error.code === "profile-store-unsupported" || error.code === "profile-store-repair-failed") && (
            <Button
              variant="secondary"
              disabled={busy}
              onClick={onRepairStore}
            >
              {t("shell.banner.repairStore")}
            </Button>
          )}
        </div>
      )}
      {settingsError && (
        <div className="asb-banner asb-banner-error" role="alert" aria-label={t("shell.banner.settingsUnavailable.aria")}>
          <span>{t("shell.banner.settingsUnavailable", { detail: settingsError })}</span>
          <Button
            variant="secondary"
            disabled={busy}
            onClick={onRepairSettings}
          >
            {t("shell.banner.repair")}
          </Button>
        </div>
      )}
    </div>
  );
}

/** Application frame: brand, primary navigation, the persistent-error
 * banner, and the busy overlay. Page geometry never changes when banners
 * appear or disappear. */
export function AppShell({
  page,
  onPageChange,
  error,
  busy,
  settingsError,
  onRepairSettings,
  onRepairStore,
  pin,
  update,
  children,
}: AppShellProps) {
  const { t } = useI18n();
  return (
    <>
      <div className="asb-ambient" aria-hidden="true" />
      <div className="asb-shell">
        <header className="asb-topbar asb-surface-rail" data-tauri-drag-region>
          <span className="asb-topbar-brand" data-tauri-drag-region>
            <img className="asb-topbar-icon" src={appIcon} alt="" />
            <h1 className="asb-topbar-title" data-tauri-drag-region>
              Agent Switchboard
            </h1>
            <span className="asb-topbar-beta" aria-label={t("shell.beta.aria")} data-tauri-drag-region>
              Beta
            </span>
          </span>
          <nav aria-label={t("shell.nav.aria")}>
            <ul className="asb-nav">
              {PAGES.map((item) => (
                <li key={item}>
                  <Button
                    variant="unstyled"
                    aria-current={page === item ? "page" : undefined}
                    onClick={() => onPageChange(item)}
                  >
                    {t(pageLabelKey(item))}
                  </Button>
                </li>
              ))}
            </ul>
          </nav>
          {isBrowserDevelopment ? <span className="asb-web-development-badge">{t("shell.devBadge")}</span> : null}
          {update ? (
            <UpdateButton latestVersion={update.latestVersion} onOpen={update.onOpen} />
          ) : null}
          {pin ? (
            <PinTopButton active={pin.active} disabled={busy} onToggle={pin.onToggle} />
          ) : null}
          {!isBrowserDevelopment && <WindowControls />}
        </header>
        <div className="asb-workspace">
          <OperationNotices error={error} settingsError={settingsError} busy={busy}
            onRepairStore={onRepairStore} onRepairSettings={onRepairSettings} />
          {children}
        </div>
      </div>
      {busy && (
        <div className="asb-busy" role="status" aria-label={t("shell.busy.aria")}>
          {t("shell.busy")}
        </div>
      )}
    </>
  );
}
