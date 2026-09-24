import type { ReactNode } from "react";
import { ArchiveRestore, Info, SlidersHorizontal, Stethoscope, Wrench } from "lucide-react";
import type { AppSettings, UpdateChannel, UpdateCheck } from "../api/client";
import appIcon from "../assets/app-icon.svg";
import { SETTINGS_SECTIONS, type SettingsSection } from "../app/navigation";
import type { UpdateDownloadProgress } from "../app/useUpdateCheck";
import { useI18n } from "../i18n";
import { AppSettingsForm } from "../components/AppSettingsForm";
import { Button } from "../components/Button";
import { ModuleHeader } from "../components/WorkspaceHeader";
import { UpdateSection } from "../components/UpdateSection";
import { GatewayIcon } from "../components/icons";

const SECTION_ICONS = {
  application: SlidersHorizontal,
  "client-management": Wrench,
  gateway: GatewayIcon,
  backups: ArchiveRestore,
  diagnostics: Stethoscope,
  about: Info,
};

interface SettingsPageProps {
  section: SettingsSection;
  onSectionChange: (section: SettingsSection) => void;
  onReturnToProviders?: () => void;
  backups: ReactNode;
  gateway: ReactNode;
  clientManagement: ReactNode;
  diagnostics: ReactNode;
  settings: AppSettings | null;
  /** Why settings could not load; null while loading or after success. */
  loadError: string | null;
  desktopError: string | null;
  /** Re-runs the settings load after a failure. */
  onRetryLoad: () => void;
  /** Replaces an invalid settings file with defaults. */
  onRepair: () => void;
  busy: boolean;
  /** Saves one field of the currently loaded settings. */
  onPatch: (patch: Partial<AppSettings>) => Promise<boolean>;
  /** Restarts the desktop process on the user's explicit request. */
  onRestart: () => void;
  /** Latest manual update check; null until the first check runs. */
  updateCheck: UpdateCheck | null;
  updateChannel: UpdateChannel | null;
  /** Running build's version; null until the process reports it. */
  appVersion: string | null;
  /** A startup or user-triggered release lookup is currently in flight. */
  updateChecking: boolean;
  updateInstalling: boolean;
  updateProgress: UpdateDownloadProgress | null;
  updateCheckedAt: string | null;
  updateRestartRequired: boolean;
  onCheckUpdate: () => void;
  onInstallUpdate: () => void;
  onRestartInstalledUpdate: () => void;
}

function ApplicationSettings(props: SettingsPageProps) {
  const { settings, busy, onPatch } = props;
  const { t } = useI18n();
  return (
    <div className="asb-app-settings">
      {settings ? (
        <AppSettingsForm settings={settings} busy={busy} onPatch={onPatch}
          desktopError={props.desktopError} onRestart={props.onRestart} />
      ) : props.loadError ? (
        <>
          <div className="asb-app-setting-row" role="alert">
            <div className="asb-app-setting-copy">
              <span className="asb-checkbox-label">{t("settings.loadFailed", { detail: props.loadError })}</span>
              <span className="asb-app-setting-detail">{t("settings.loadFailedDetail")}</span>
            </div>
            <div className="asb-app-settings-error-actions">
              <Button variant="secondary" disabled={busy} onClick={props.onRetryLoad}>{t("settings.retry")}</Button>
            </div>
          </div>
          <div className="asb-app-settings-danger-zone">
            <Button variant="danger" disabled={busy} onClick={props.onRepair}>{t("shell.banner.repair")}</Button>
          </div>
        </>
      ) : <div className="asb-settings-skeleton" role="status" aria-label={t("settings.page.loading")}>
        <div className="asb-skeleton" />
        <div className="asb-skeleton" />
        <div className="asb-skeleton" />
      </div>}
    </div>
  );
}

function AboutSettings(props: SettingsPageProps) {
  const { t } = useI18n();
  return (
    <div className="asb-app-settings">
      <div className="asb-about-product">
        <img className="asb-about-logo" src={appIcon} alt="" />
        <div className="asb-about-copy">
          <h3 className="asb-section-title">Agent Switchboard</h3>
          <p className="asb-field-help">{t("settings.about.tagline")}</p>
        </div>
      </div>
      <UpdateSection channel={props.updateChannel} appVersion={props.appVersion}
        result={props.updateCheck} busy={props.busy || props.updateChecking} installing={props.updateInstalling}
        progress={props.updateProgress} checkedAt={props.updateCheckedAt} restartRequired={props.updateRestartRequired}
        onCheck={props.onCheckUpdate} onInstall={props.onInstallUpdate} onRestart={props.onRestartInstalledUpdate} />
    </div>
  );
}

function SettingsPanel({ section, selected, children }: {
  section: SettingsSection;
  selected: SettingsSection;
  children: ReactNode;
}) {
  const { t } = useI18n();
  const title = t(SETTINGS_SECTIONS.find((item) => item.value === section)!.labelKey);
  return (
    <section className="asb-panel" hidden={selected !== section} aria-label={title}>
      <ModuleHeader title={title} />
      {children}
    </section>
  );
}

export function SettingsPage(props: SettingsPageProps) {
  const { section, onSectionChange } = props;
  const { t } = useI18n();
  return (
    <section className="asb-settings-workspace" aria-label={t("nav.page.settings")}>
      <aside className="asb-settings-sidebar">
        <div className="asb-settings-sidebar-heading">
          <h2 className="asb-settings-sidebar-title">{t("nav.page.settings")}</h2>
          {props.onReturnToProviders && <Button variant="secondary" onClick={props.onReturnToProviders}>{t("settings.page.backToProviders")}</Button>}
        </div>
        <nav className="asb-settings-navigation" aria-label={t("settings.page.categories.aria")}>
          {SETTINGS_SECTIONS.map(({ value, labelKey }) => {
            const Icon = SECTION_ICONS[value];
            return (
              <Button key={value} variant="unstyled" aria-current={section === value ? "page" : undefined}
                className="asb-settings-category" onClick={() => onSectionChange(value)}>
                <span className="asb-settings-category-icon" aria-hidden="true"><Icon size={18} /></span>
                <span>{t(labelKey)}</span>
              </Button>
            );
          })}
        </nav>
      </aside>
      <div className="asb-settings-content">
        <SettingsPanel section="application" selected={section}><ApplicationSettings {...props} /></SettingsPanel>
        {section === "client-management" && props.clientManagement}
        <div hidden={section !== "gateway"}>{props.gateway}</div>
        {section === "backups" && props.backups}
        <div hidden={section !== "diagnostics"}>{props.diagnostics}</div>
        <SettingsPanel section="about" selected={section}><AboutSettings {...props} /></SettingsPanel>
      </div>
    </section>
  );
}
