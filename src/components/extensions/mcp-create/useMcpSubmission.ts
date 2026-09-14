import { useRef, useState } from "react";

export function useMcpSubmission(externalBusy: boolean, action: () => Promise<void>, onBusyChange?: (busy: boolean) => void) {
  const locked = useRef(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const submit = async () => {
    if (externalBusy || locked.current) return;
    locked.current = true;
    setSaving(true);
    setError(null);
    try {
      onBusyChange?.(true);
      await action();
    } catch (caught) {
      setError(caught instanceof Error ? caught.message : "保存失败，配置已保留，请重试");
    } finally {
      locked.current = false;
      setSaving(false);
      onBusyChange?.(false);
    }
  };
  return { saving, busy: externalBusy || saving, error, setError, submit };
}
