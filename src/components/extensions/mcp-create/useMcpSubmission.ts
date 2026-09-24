import { useMessageState } from "../../../i18n/use-message-state";
import { useRef, useState } from "react";

export function useMcpSubmission(externalBusy: boolean, action: () => Promise<void>, onBusyChange?: (busy: boolean) => void) {
  const locked = useRef(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useMessageState();
  const submit = async () => {
    if (externalBusy || locked.current) return;
    locked.current = true;
    setSaving(true);
    setError(null);
    try {
      onBusyChange?.(true);
      await action();
    } catch (caught) {
      setError(caught);
    } finally {
      locked.current = false;
      setSaving(false);
      onBusyChange?.(false);
    }
  };
  return { saving, busy: externalBusy || saving, error, setError, submit };
}
