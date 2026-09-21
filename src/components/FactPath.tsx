import { useState } from "react";
import type { CommandError } from "../api/client";
import { Button } from "./Button";

/** A fact-row path value the reader can open: the backend resolves the real
 * location, the path text is the affordance. Open failures speak as an
 * error-red line in the same fact row, matching the runtime-log pattern. */
export function FactPath({ path, open }: { path: string; open: () => Promise<void> }) {
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
      <Button variant="unstyled" className="asb-fact-path" title="打开所在位置"
        disabled={busy} onClick={() => void reveal()}>
        {path}
      </Button>
      {error && (
        <span className="asb-fact-path-error" role="alert">无法打开所在位置：{error.message}</span>
      )}
    </>
  );
}
