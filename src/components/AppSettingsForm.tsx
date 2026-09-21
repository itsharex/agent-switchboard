import { useRef } from "react";
import type { AppSettings } from "../api/client";
import { AppSegmentSetting, AppSettingRow } from "./AppPreferenceControls";
import { Button } from "./Button";
import { FontPicker } from "./FontPicker";
import { GlobalShortcutSetting } from "./GlobalShortcutSetting";
import { Switch } from "./Switch";

interface Props {
  settings: AppSettings;
  busy: boolean;
  desktopError: string | null;
  onPatch: (patch: Partial<AppSettings>) => Promise<boolean>;
  onRestart: () => void;
}

function AppearanceSettings({ settings, busy, onPatch }: Props) {
  return (
    <section className="asb-app-settings-group" aria-labelledby="appearance-settings">
      <h3 id="appearance-settings" className="asb-section-title">外观</h3>
      <AppSegmentSetting<AppSettings["theme"]> label="界面主题" value={settings.theme} busy={busy}
        options={[{ value: "system", label: "跟随系统" }, { value: "light", label: "浅色" }, { value: "dark", label: "深色" }]}
        onChange={(theme) => void onPatch({ theme })} />
      <AppSettingRow label="界面字体">
        <FontPicker value={settings.interfaceFont} busy={busy} onChange={(interfaceFont) => void onPatch({ interfaceFont })} />
      </AppSettingRow>
      <AppSegmentSetting<AppSettings["interfaceScale"]> label="主界面缩放" detail="同步调整文字与控件大小，立即生效。"
        value={settings.interfaceScale} busy={busy}
        options={[{ value: 90, label: "90%" }, { value: 100, label: "100%" }, { value: 110, label: "110%" }, { value: 125, label: "125%" }]}
        onChange={(interfaceScale) => void onPatch({ interfaceScale })} />
      <AppSegmentSetting<AppSettings["motion"]> label="动态效果" value={settings.motion} busy={busy}
        options={[{ value: "system", label: "跟随系统" }, { value: "reduce", label: "减少动态效果" }]}
        onChange={(motion) => void onPatch({ motion })} />
    </section>
  );
}

function WindowSettings({ settings, busy, onPatch }: Props) {
  return (
    <section className="asb-app-settings-group" aria-labelledby="window-tray-settings">
      <h3 id="window-tray-settings" className="asb-section-title">窗口与托盘</h3>
      <AppSegmentSetting<AppSettings["closeBehavior"]> label="点击关闭按钮时" value={settings.closeBehavior} busy={busy}
        options={[{ value: "hideToTray", label: "最小化到托盘" }, { value: "exit", label: "退出应用" }]}
        onChange={(closeBehavior) => void onPatch({ closeBehavior })} />
      <AppSettingRow label="窗口始终置顶">
        <Switch label="窗口始终置顶" checked={settings.alwaysOnTop} disabled={busy}
          onChange={(alwaysOnTop) => void onPatch({ alwaysOnTop })} />
      </AppSettingRow>
      <AppSettingRow label="开机自动启动">
        <Switch label="开机自动启动" checked={settings.launchAtLogin} disabled={busy}
          onChange={(launchAtLogin) => void onPatch({ launchAtLogin })} />
      </AppSettingRow>
      <AppSettingRow label="启动时最小化到托盘" detail="应用启动后保留在系统托盘，不显示主窗口。">
        <Switch label="启动时最小化到托盘" checked={settings.startMinimized} disabled={busy}
          onChange={(startMinimized) => void onPatch({ startMinimized })} />
      </AppSettingRow>
      <AppSegmentSetting<AppSettings["startupPage"]> label="启动页面" detail="下次启动生效。上次访问页面仅恢复顶层页面，不恢复草稿或待确认操作。"
        value={settings.startupPage} busy={busy}
        options={[{ value: "providers", label: "供应商切换" }, { value: "lastVisited", label: "上次访问页面" }]}
        onChange={(startupPage) => void onPatch({ startupPage })} />
      <GlobalShortcutSetting value={settings.globalShortcut} busy={busy}
        onChange={(globalShortcut) => onPatch({ globalShortcut })} />
    </section>
  );
}

function PerformanceSettings({ settings, busy, onPatch, onRestart }: Props) {
  const initialAcceleration = useRef(settings.hardwareAcceleration);
  if (!/Windows/.test(navigator.userAgent)) return null;
  return (
    <section className="asb-app-settings-group" aria-labelledby="performance-settings">
      <h3 id="performance-settings" className="asb-section-title">性能</h3>
      <AppSettingRow label="启用硬件加速">
        <Switch label="启用硬件加速" checked={settings.hardwareAcceleration} disabled={busy}
          onChange={(hardwareAcceleration) => void onPatch({ hardwareAcceleration })} />
      </AppSettingRow>
      {initialAcceleration.current !== settings.hardwareAcceleration && (
        <AppSettingRow label="重启以应用硬件加速">
          <Button variant="secondary" disabled={busy} onClick={onRestart}>重启应用</Button>
        </AppSettingRow>
      )}
    </section>
  );
}

/** Application preferences use the single native save transaction. */
export function AppSettingsForm(props: Props) {
  const { settings, busy, onPatch } = props;
  return (
    <div className="asb-app-settings">
      {props.desktopError && <p className="asb-field-error" role="alert">{props.desktopError}</p>}
      <AppearanceSettings {...props} />
      <WindowSettings {...props} />
      <section className="asb-app-settings-group" aria-labelledby="log-settings">
        <h3 id="log-settings" className="asb-section-title">日志</h3>
        <AppSegmentSetting<AppSettings["runtimeLogLevel"]> label="运行事件记录级别" detail="低于该级别的新事件不再写入日志文件，保存后立即生效。"
          value={settings.runtimeLogLevel} busy={busy}
          options={[{ value: "debug", label: "调试" }, { value: "info", label: "信息" }, { value: "warn", label: "警告" }, { value: "error", label: "错误" }, { value: "silent", label: "静默" }]}
          onChange={(runtimeLogLevel) => void onPatch({ runtimeLogLevel })} />
      </section>
      <PerformanceSettings {...props} />
    </div>
  );
}
