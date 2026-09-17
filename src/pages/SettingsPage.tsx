import type { ReactNode } from "react";
import type { AppSettings, UpdateChannel, UpdateCheck } from "../api/client";
import appIcon from "../assets/app-icon.svg";
import { SETTINGS_SECTIONS, type SettingsSection } from "../app/navigation";
import type { UpdateDownloadProgress } from "../app/useUpdateCheck";
import { AppSettingsForm } from "../components/AppSettingsForm";
import { Button } from "../components/Button";
import { ModuleHeader } from "../components/WorkspaceHeader";
import { UpdateSection } from "../components/UpdateSection";

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
  /** Re-runs the settings load after a failure. */
  onRetryLoad: () => void;
  /** Replaces an invalid settings file with defaults. */
  onRepair: () => void;
  busy: boolean;
  /** Saves one field of the currently loaded settings. */
  onPatch: (patch: Partial<AppSettings>) => void;
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
  return (
    <div className="asb-app-settings">
      {settings ? (
        <AppSettingsForm settings={settings} busy={busy}
          onCloseBehaviorChange={(closeBehavior) => onPatch({ closeBehavior })}
          onThemeChange={(theme) => onPatch({ theme })}
          onMotionChange={(motion) => onPatch({ motion })}
          onInterfaceFontChange={(interfaceFont) => onPatch({ interfaceFont })}
          onAlwaysOnTopChange={(alwaysOnTop) => onPatch({ alwaysOnTop })}
          onLaunchAtLoginChange={(launchAtLogin) => onPatch({ launchAtLogin })}
          onHardwareAccelerationChange={(hardwareAcceleration) => onPatch({ hardwareAcceleration })}
          onRestart={props.onRestart} />
      ) : props.loadError ? (
        <div className="asb-app-setting-row" role="alert">
          <div className="asb-app-setting-copy">
            <span className="asb-checkbox-label">设置加载失败：{props.loadError}</span>
            <span className="asb-app-setting-detail">读取失败期间，外观与关闭行为使用默认值</span>
          </div>
          <div className="asb-panel-actions">
            <Button variant="secondary" disabled={busy} onClick={props.onRetryLoad}>重试</Button>
            <Button variant="secondary" disabled={busy} onClick={props.onRepair}>一键修复</Button>
          </div>
        </div>
      ) : <p className="asb-empty">加载中</p>}
    </div>
  );
}

function AboutSettings(props: SettingsPageProps) {
  return (
    <div className="asb-app-settings">
      <div className="asb-about-product">
        <img className="asb-about-logo" src={appIcon} alt="" />
        <div className="asb-about-copy">
          <h3 className="asb-section-title">Agent Switchboard</h3>
          <p className="asb-field-help">Codex 与 Claude Code 的本地配置控制台。</p>
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
  const title = SETTINGS_SECTIONS.find((item) => item.value === section)!.label;
  return (
    <section className="asb-panel" hidden={selected !== section} aria-label={title}>
      <ModuleHeader title={title} />
      {children}
    </section>
  );
}

export function SettingsPage(props: SettingsPageProps) {
  const { section, onSectionChange } = props;
  return (
    <section className="asb-settings-workspace" aria-label="设置">
      <aside className="asb-settings-sidebar">
        <div className="asb-settings-sidebar-heading">
          <h2 className="asb-panel-title">设置</h2>
          {props.onReturnToProviders && <Button variant="secondary" onClick={props.onReturnToProviders}>返回供应商</Button>}
        </div>
        <nav className="asb-settings-navigation" aria-label="设置分类">
          {SETTINGS_SECTIONS.map(({ value, label }) => (
            <Button key={value} variant="unstyled" aria-current={section === value ? "page" : undefined}
              className="asb-settings-category" onClick={() => onSectionChange(value)}>{label}</Button>
          ))}
        </nav>
      </aside>
      <div className="asb-settings-content">
        <SettingsPanel section="application" selected={section}><ApplicationSettings {...props} /></SettingsPanel>
        {section === "backups" && props.backups}
        <div hidden={section !== "gateway"}>{props.gateway}</div>
        {section === "client-management" && props.clientManagement}
        <div hidden={section !== "diagnostics"}>{props.diagnostics}</div>
        <SettingsPanel section="about" selected={section}><AboutSettings {...props} /></SettingsPanel>
      </div>
    </section>
  );
}
