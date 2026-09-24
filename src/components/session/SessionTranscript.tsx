import { useEffect, useRef, useState } from "react";
import type { SessionMessage, SessionMeta } from "../../api/client";
import { useI18n } from "../../i18n";
import { Button } from "../Button";
import { SessionMessageView } from "./SessionMessageView";
import { codexOutlinePreview, previewLine } from "./session-content";
import type { SessionBookmarks } from "./use-session-bookmarks";

interface Props {
  session: SessionMeta;
  messages: SessionMessage[];
  target: string | null;
  bookmarks: SessionBookmarks;
}

function outline(messages: SessionMessage[], app: SessionMeta["app"]) {
  return messages.filter((message) => message.role === "user").flatMap((message) => {
    const content = app === "codex" ? codexOutlinePreview(message.content) : message.content;
    return content === null ? [] : [{ id: message.id, preview: previewLine(content) }];
  });
}

export function SessionTranscript({ session, messages, target, bookmarks }: Props) {
  const { t } = useI18n();
  const [expanded, setExpanded] = useState(new Set<string>());
  const [focused, setFocused] = useState(target);
  const [jump, setJump] = useState(0);
  const container = useRef<HTMLDivElement>(null);
  const items = outline(messages, session.app);
  const saved = new Set(bookmarks.items?.filter((item) => item.app === session.app
    && item.sessionId === session.sessionId).map((item) => item.messageId));
  useEffect(() => {
    if (!focused) return;
    const node = Array.from(container.current?.querySelectorAll<HTMLElement>("[data-message-id]") ?? [])
      .find((element) => element.dataset.messageId === focused);
    node?.focus({ preventScroll: true });
    node?.scrollIntoView({ block: "center" });
  }, [focused, jump]);
  const toggle = (id: string) => {
    if (focused === id) setFocused(null);
    setExpanded((current) => {
      const next = new Set(current);
      if (current.has(id) || focused === id) next.delete(id); else next.add(id);
      return next;
    });
  };
  return <div className="asb-session-body">
    <div className="asb-session-conversation">
      <div className="asb-session-list-heading">
        <span>{t("sessions.transcript.heading")}</span><span className="asb-session-count">{messages.length}</span>
      </div>
      <div className="asb-session-transcript" ref={container} aria-label={t("sessions.transcript.aria")}>
        {messages.length === 0 && <p>{t("sessions.transcript.empty")}</p>}
        {messages.map((message) => <SessionMessageView key={message.id} message={message}
          targeted={focused === message.id} expanded={focused === message.id || expanded.has(message.id)}
          onToggleExpanded={toggle} bookmarked={saved.has(message.id)}
          bookmarkBusy={bookmarks.busy || bookmarks.loading || bookmarks.items === null}
          onBookmark={["user", "assistant"].includes(message.role) ? () => void bookmarks.save(session, message.id) : undefined} />)}
      </div>
    </div>
    {items.length > 2 && <nav className="asb-session-toc" aria-label={t("sessions.toc.aria")}>
      <div className="asb-session-toc-heading">{t("sessions.toc.heading")}</div>
      <div className="asb-session-toc-items">{items.map((item, index) =>
        <Button variant="unstyled" key={item.id} onClick={() => { setFocused(item.id); setJump((value) => value + 1); }}>
          <span className="asb-session-toc-index">{index + 1}</span>
          <span className="asb-session-toc-preview" title={item.preview}>{item.preview}</span>
        </Button>)}</div>
    </nav>}
  </div>;
}
