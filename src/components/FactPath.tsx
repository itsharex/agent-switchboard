import { commandErrorText } from "../i18n/errors";
import { useState } from "react";
import type { CommandError } from "../api/client";
import { useI18n } from "../i18n";
import { Button } from "./Button";

/** A fact-row path value the reader can open: the backend resolves the real
 * location, the path text is the affordance. Open failures speak as an
 * error-red line in the same fact row, matching the runtime-log pattern. */
export function FactPath({ path, open }: { path: string; open: () => Promise<void> }) {
  const { t } = useI18n();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);

  const reveal = async () => {
    setBusy(true);
    setError(null);
    try {
      await open();
    } catch (cause) {
      setError(cause as CommandError);
    } finally {
      setBusy(false);
    }
  };

  return (
    <>
      <Button variant="unstyled" className="asb-fact-path" title={t("providers.factPath.openTitle")}
        disabled={busy} onClick={() => void reveal()}>
        {path}
      </Button>
      {error && (
        <span className="asb-fact-path-error" role="alert">{t("providers.factPath.openFailed", { detail: commandErrorText(error, t) })}</span>
      )}
    </>
  );
}
