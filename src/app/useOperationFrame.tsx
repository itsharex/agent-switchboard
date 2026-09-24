import { useCallback, useState } from "react";
import type { CommandError } from "../api/client";
import { toast, toastMessage } from "../components/use-toast";
import { CommandErrorLines } from "./notifications";

/**
 * The cross-domain operation frame: one busy flag and the error gate.
 * Persistent store-format errors keep the actionable repair banner; every other
 * failure becomes a transient global toast (longer 10s error dwell).
 * Toast copy resolves at render time, so a visible toast follows a language
 * switch; the scrubbed backend message stays as the diagnostic detail line.
 */
export function useOperationFrame() {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);

  const reportError = useCallback((caught: CommandError) => {
    if (caught.code === "profile-store-unsupported" || caught.code === "profile-store-repair-failed") {
      setError(caught);
      return;
    }
    toast({
      kind: "error",
      title: toastMessage("common.operationFailed"),
      description: <CommandErrorLines error={caught} />,
    });
  }, []);

  const clearError = useCallback(() => setError(null), []);

  return { busy, setBusy, error, reportError, clearError };
}
