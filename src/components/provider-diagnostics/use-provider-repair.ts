import { useEffect, useRef, useState } from "react";
import {
  cancelProviderRepairPreparation, commitProviderRepair, commitProviderRepairUndo,
  prepareProviderRepair, prepareProviderRepairUndo, type ProviderRepairPreview,
  type ProviderRepairResult, type ProviderRepairUndoPreview,
} from "../../api/provider-diagnostics";
import type { LocalizedMessage } from "../../api/client";
import { uiMessage } from "../../i18n/errors";
import { useMessageState } from "../../i18n/use-message-state";

type Pending = { kind: "repair"; preview: ProviderRepairPreview }
  | { kind: "undo"; preview: ProviderRepairUndoPreview };

export function useProviderRepair(profileId: string, refresh: () => Promise<void>) {
  const [pending, setPending] = useState<Pending | null>(null);
  const [receipt, setReceipt] = useState<ProviderRepairResult | null>(null);
  const [warnings, setWarnings] = useState<LocalizedMessage[]>([]);
  const [status, setStatus] = useMessageState();
  const [error, setError] = useMessageState();
  const [busy, setBusy] = useState(false);
  const lock = useRef(false);
  const alive = useRef(true);
  const preparation = useRef<string | null>(null);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      if (preparation.current) void cancelProviderRepairPreparation(preparation.current).catch(console.warn);
    };
  }, []);
  const run = async (action: () => Promise<void>) => {
    if (lock.current) return;
    lock.current = true; setBusy(true); setError(null);
    try { await action(); }
    catch (caught) { if (alive.current) setError(caught); }
    finally { lock.current = false; if (alive.current) setBusy(false); }
  };
  const preview = (kind: "repair" | "undo") => run(async () => {
    if (pending || (kind === "undo" && !receipt?.canUndo)) return;
    const next: Pending = kind === "undo" && receipt
      ? { kind: "undo", preview: await prepareProviderRepairUndo(receipt.repairId) }
      : { kind: "repair", preview: await prepareProviderRepair(profileId) };
    if (!alive.current) { await cancelProviderRepairPreparation(next.preview.preparationId); return; }
    preparation.current = next.preview.preparationId;
    setPending(next);
  });
  const cancel = () => run(async () => {
    if (preparation.current) await cancelProviderRepairPreparation(preparation.current);
    preparation.current = null; setPending(null);
  });
  const confirm = () => run(async () => {
    if (!pending) return;
    const current = pending;
    setPending(null); preparation.current = null;
    try {
      if (current.kind === "repair") {
        const result = await commitProviderRepair(current.preview.preparationId, true);
        setReceipt(result); setWarnings(result.outcome.warnings); setStatus(uiMessage("doctor.repaired"));
      } else {
        const result = await commitProviderRepairUndo(current.preview.preparationId, true);
        setReceipt(null); setWarnings(result.warnings); setStatus(uiMessage("doctor.undone"));
      }
    } finally { await refresh(); }
  });
  return { pending, receipt, warnings, status, error, busy, preview, cancel, confirm };
}

export type ProviderRepairState = ReturnType<typeof useProviderRepair>;
