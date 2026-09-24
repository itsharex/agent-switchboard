import { useCallback, useEffect, useRef, useState } from "react";
import { getSessionMessages, getSessionMetadata, type SessionMessage, type SessionMeta } from "../../api/client";
import { useMessageState } from "../../i18n/use-message-state";
import { uiMessage } from "../../i18n/errors";
import { sessionKey } from "./session-content";

export function useSessionDetail() {
  const [selected, setSelected] = useState<SessionMeta | null>(null);
  const [messages, setMessages] = useState<SessionMessage[] | null>(null);
  const [target, setTarget] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useMessageState();
  const generation = useRef(0);
  const metadataGeneration = useRef(0);

  useEffect(() => () => { generation.current += 1; }, []);
  const select = useCallback(async (session: SessionMeta, messageId: string | null = null) => {
    const version = ++generation.current;
    const metadataVersion = metadataGeneration.current;
    setSelected(session);
    setMessages(null);
    setTarget(null);
    setError(null);
    setBusy(true);
    try {
      const [next, metadata] = await Promise.all([getSessionMessages(session.app, session.sessionId),
        getSessionMetadata(session.app, session.sessionId)]);
      if (generation.current !== version) return;
      if (metadataVersion === metadataGeneration.current) setSelected(metadata);
      setMessages(next);
      if (messageId && !next.some((message) => message.id === messageId)) {
        setError(uiMessage("sessions.search.changed"));
      } else setTarget(messageId);
    } catch (caught) {
      if (generation.current === version) setError(caught);
    } finally {
      if (generation.current === version) setBusy(false);
    }
  }, [setError]);

  const clear = () => {
    generation.current += 1;
    setSelected(null); setMessages(null); setTarget(null); setBusy(false); setError(null);
  };
  const updateMetadata = (sessions: SessionMeta[]) => {
    metadataGeneration.current += 1;
    setSelected((current) => current
      ? sessions.find((session) => sessionKey(session) === sessionKey(current)) ?? current : null);
  };
  return { selected, messages, target, busy, error, select, clear, updateMetadata };
}

export type SessionDetailState = ReturnType<typeof useSessionDetail>;
