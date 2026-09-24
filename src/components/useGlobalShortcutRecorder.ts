import { uiMessage } from "../i18n/errors";
import { useMessageState } from "../i18n/use-message-state";
import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { setShortcutRecording } from "../api/client";
import { captureGlobalShortcut } from "../lib/global-shortcut";

export function useGlobalShortcutRecorder(onChange: (value: string) => Promise<boolean>) {
  const [recording, setRecording] = useState(false);
  const [preparing, setPreparing] = useState(false);
  const [error, setError] = useMessageState();
  const captureButton = useRef<HTMLButtonElement>(null);
  const mounted = useRef(false);
  const session = useRef(0);
  const report = (cause: unknown) => {
    if (mounted.current) setError(cause);
    else console.error("结束快捷键录入失败", cause);
  };
  const finish = () => {
    session.current += 1;
    setRecording(false);
    setPreparing(false);
    void setShortcutRecording(false).catch(report);
  };
  useEffect(() => {
    mounted.current = true;
    window.addEventListener("blur", finish);
    return () => {
      mounted.current = false;
      session.current += 1;
      window.removeEventListener("blur", finish);
      void setShortcutRecording(false).catch(report);
    };
  }, []);
  const start = async () => {
    const current = ++session.current;
    setPreparing(true);
    setError(null);
    try {
      await setShortcutRecording(true);
      if (!mounted.current || current !== session.current) {
        await setShortcutRecording(false);
        return;
      }
      setRecording(true);
      captureButton.current?.focus();
    } catch (cause) { report(cause); }
    finally { if (mounted.current) setPreparing(false); }
  };
  const capture = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (!recording) return;
    if (event.key === "Tab") { finish(); return; }
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "Escape") { finish(); return; }
    if (event.repeat || /^(Control|Alt|Shift|Meta)(Left|Right)$/.test(event.code)) return;
    const chord = captureGlobalShortcut(event);
    if (!chord) { setError(uiMessage("settings.shortcut.invalidChord")); return; }
    finish();
    void onChange(chord);
  };
  return { recording, preparing, error, captureButton, finish, start, capture };
}
