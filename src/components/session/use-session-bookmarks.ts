import { useCallback, useEffect, useRef, useState } from "react";
import { deleteSessionBookmark, listSessionBookmarks, saveSessionBookmark,
  type SessionBookmark, type SessionMeta } from "../../api/client";
import { useMessageState } from "../../i18n/use-message-state";

export function useSessionBookmarks(active: boolean) {
  const [items, setItems] = useState<SessionBookmark[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useMessageState();
  const [revision, setRevision] = useState(0);
  const generation = useRef(0);
  const locked = useRef(false);

  useEffect(() => {
    if (!active || locked.current) return;
    const version = ++generation.current;
    setLoading(true);
    setError(null);
    void listSessionBookmarks().then((next) => {
      if (generation.current === version) setItems(next);
    }).catch((caught: unknown) => {
      if (generation.current === version) { setItems(null); setError(caught); }
    }).finally(() => {
      if (generation.current === version) setLoading(false);
    });
    return () => { generation.current += 1; };
  }, [active, revision, setError]);

  const save = async (session: SessionMeta, messageId: string) => {
    if (locked.current || loading || items === null) return;
    locked.current = true;
    setBusy(true);
    setError(null);
    try {
      const saved = await saveSessionBookmark(session.app, session.sessionId, messageId);
      setItems((current) => [saved, ...(current ?? []).filter((item) => item.id !== saved.id)]);
    } catch (caught) { setError(caught); }
    finally { locked.current = false; setBusy(false); }
  };

  const remove = async (id: string) => {
    if (locked.current || loading) return;
    locked.current = true;
    setBusy(true);
    setError(null);
    try {
      await deleteSessionBookmark(id);
      setItems((current) => current?.filter((item) => item.id !== id) ?? null);
    } catch (caught) { setError(caught); }
    finally { locked.current = false; setBusy(false); }
  };

  const refresh = useCallback(() => { if (!locked.current) setRevision((value) => value + 1); }, []);
  return { items, busy, loading, error, save, remove, refresh };
}

export type SessionBookmarks = ReturnType<typeof useSessionBookmarks>;
