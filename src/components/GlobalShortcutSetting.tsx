import { globalShortcutLabel } from "../lib/global-shortcut";
import { AppSettingRow } from "./AppPreferenceControls";
import { Button } from "./Button";
import { useGlobalShortcutRecorder } from "./useGlobalShortcutRecorder";

export function GlobalShortcutSetting({ value, busy, onChange }: {
  value: string;
  busy: boolean;
  onChange: (value: string) => Promise<boolean>;
}) {
  const recorder = useGlobalShortcutRecorder(onChange);
  const { recording, preparing, error } = recorder;
  return (
    <AppSettingRow label="全局唤起快捷键" detail="打开并聚焦主窗口；窗口已在前台时隐藏到托盘。">
      <div className="asb-shortcut-setting">
        <div className="asb-shortcut-controls">
          <Button ref={recorder.captureButton} variant="secondary" disabled={busy} aria-pressed={recording} aria-busy={preparing}
            aria-label={recording ? "按下快捷键，Esc 取消" : `录入全局快捷键，当前${globalShortcutLabel(value)}`}
            onClick={() => { if (!recording && !preparing) void recorder.start(); }} onKeyDown={recorder.capture}
            onBlur={() => { if (recording || preparing) recorder.finish(); }}>
            {preparing ? "准备录入…" : recording ? "按下快捷键…" : globalShortcutLabel(value)}
          </Button>
          {recording ? <Button variant="secondary" onClick={recorder.finish}>取消</Button>
            : value && <Button variant="secondary" disabled={busy || preparing} onClick={() => void onChange("")}>清除</Button>}
        </div>
        {error && <p className="asb-field-error" role="alert">{error}</p>}
      </div>
    </AppSettingRow>
  );
}
