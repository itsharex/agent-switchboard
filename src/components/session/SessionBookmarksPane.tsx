import { useEffect, useRef, useState } from "react";
import { getSessionMetadata, type SessionBookmark, type SessionMeta } from "../../api/client";
import { useI18n } from "../../i18n";
import { useMessageState } from "../../i18n/use-message-state";
import { Button } from "../Button";
import type { ClientFilterValue } from "../ClientFilter";
import { ClientLogo } from "../ClientLogo";
import { ConfirmSheet } from "../ConfirmSheet";
import { Time } from "../Time";
import { SessionMessageView } from "./SessionMessageView";
import type { SessionBookmarks } from "./use-session-bookmarks";

const normalized = (value: string) => value.toLowerCase().replace(/\s+/gu, " ").trim();

function BookmarkDetail({ item, busy, onRemove, onOpen }: {
  item: SessionBookmark; busy: boolean; onRemove: () => void;
  onOpen: (session: SessionMeta, messageId: string) => void;
}) {
  const { t } = useI18n();
  const [opening, setOpening] = useState(false);
  const [error, setError] = useMessageState();
  const mounted = useRef(true);
  useEffect(() => { mounted.current = true; return () => { mounted.current = false; }; }, []);
  const open = async () => {
    setOpening(true); setError(null);
    try {
      const source = await getSessionMetadata(item.app, item.sessionId);
      if (!mounted.current) return;
      onOpen(source, item.messageId);
    } catch (caught) { if (mounted.current) setError(caught); }
    finally { if (mounted.current) setOpening(false); }
  };
  return <section className="asb-session-detail" aria-label={t("sessions.bookmarks.detail")}>
    <header className="asb-session-detail-head">
      <div className="asb-session-detail-title"><h3 className="asb-section-title" title={item.sessionTitle}>{item.sessionTitle}</h3>
        <span className="asb-scope-note">{t("sessions.bookmarks.savedAt")} <Time iso={item.savedAt} /></span>
      </div>
      <div className="asb-session-actions">
        <Button variant="secondary" disabled={opening} onClick={() => void open()}>{t("sessions.bookmarks.openSource")}</Button>
        <Button variant="danger" disabled={busy} onClick={onRemove}>{t("sessions.bookmarks.remove")}</Button>
      </div>
    </header>
    <p className="asb-scope-note">{t("sessions.bookmarks.snapshot")}</p>
    {item.projectDir && <p className="asb-session-bookmark-path">{item.projectDir}</p>}
    {error && <p className="asb-warn-text" role="alert">{error}</p>}
    <div className="asb-session-transcript">
      <SessionMessageView message={{ id: item.messageId, role: item.role, content: item.content, at: item.at }}
        targeted={false} expanded />
    </div>
  </section>;
}

export function SessionBookmarksPane({ bookmarks, onOpen, query, filter }: {
  bookmarks: SessionBookmarks; onOpen: (session: SessionMeta, messageId: string) => void;
  query: string; filter: ClientFilterValue;
}) {
  const { t } = useI18n();
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [pending, setPending] = useState<SessionBookmark | null>(null);
  const needle = normalized(query);
  const items = bookmarks.items?.filter((item) => (filter === "all" || filter === item.app)
    && [item.content, item.sessionTitle, item.projectDir ?? ""].some((text) => normalized(text).includes(needle))) ?? [];
  const selected = items.find((item) => item.id === selectedId);
  return <>
    <div className="asb-session-layout">
      <section className="asb-session-list" aria-label={t("sessions.bookmarks.title")} aria-busy={bookmarks.loading}>
        <div className="asb-session-list-heading"><span>{t("sessions.bookmarks.title")}</span><span className="asb-session-count">{items.length}</span></div>
        {bookmarks.loading && <p role="status">{t("sessions.bookmarks.loading")}</p>}
        {!bookmarks.loading && bookmarks.items !== null && !items.length && <p>{t("sessions.bookmarks.empty")}</p>}
        <div className="asb-session-items">{items.map((item) =>
          <Button variant="unstyled" key={item.id} aria-pressed={selectedId === item.id}
            className={`asb-session-item${selectedId === item.id ? " is-active" : ""}`} onClick={() => setSelectedId(item.id)}>
            <span className="asb-session-item-title"><ClientLogo app={item.app} className="asb-session-logo" />
              <span title={item.sessionTitle}>{item.sessionTitle}</span></span>
            <span className="asb-session-search-excerpt">{item.content.slice(0, 220)}</span>
            <span className="asb-session-item-time"><Time iso={item.savedAt} /></span>
          </Button>)}</div>
      </section>
      {selected ? <BookmarkDetail key={selected.id} item={selected} busy={bookmarks.busy || bookmarks.loading} onOpen={onOpen}
        onRemove={() => setPending(selected)} /> : <section className="asb-session-detail asb-session-empty-detail"
          aria-label={t("sessions.bookmarks.detail")}><p>{t("sessions.bookmarks.select")}</p></section>}
    </div>
    {pending && <ConfirmSheet title={t("sessions.bookmarks.remove")} confirmLabel={t("sessions.bookmarks.remove")}
      confirmDisabled={bookmarks.busy || bookmarks.loading}
      destructive onCancel={() => setPending(null)} onConfirm={() => { void bookmarks.remove(pending.id); setPending(null); }}>
      <p>{t("sessions.bookmarks.removeDetail", { title: pending.sessionTitle })}</p>
    </ConfirmSheet>}
  </>;
}
