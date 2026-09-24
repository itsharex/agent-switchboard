import type { ReactNode } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { SessionMessage } from "../../api/client";
import { useI18n } from "../../i18n";
import { Time } from "../Time";
import { Button } from "../Button";
import { toast, toastMessage } from "../use-toast";
import { copyText, messageRole } from "./session-content";

const MESSAGE_COLLAPSE_THRESHOLD = 3000;

/* Transcript links leave the webview the same way every external link in the
   app does — through the OS opener; anything that is not http(s) stays inert
   text so recorded payloads cannot smuggle other schemes into the shell. */
function MarkdownLink({ href, children }: { href?: string; children?: ReactNode }) {
  const target = typeof href === "string" && /^https?:\/\//i.test(href) ? href : null;
  if (!target) return <span>{children}</span>;
  return (
    <a
      href={target}
      onClick={(event) => {
        event.preventDefault();
        void openUrl(target);
      }}
    >
      {children}
    </a>
  );
}

interface Props {
  message: SessionMessage;
  targeted: boolean;
  expanded: boolean;
  onToggleExpanded?: (id: string) => void;
  onBookmark?: () => void;
  bookmarked?: boolean;
  bookmarkBusy?: boolean;
}

function MessageHeader({ message, onBookmark, bookmarked, bookmarkBusy }: Pick<Props,
  "message" | "onBookmark" | "bookmarked" | "bookmarkBusy">) {
  const { t } = useI18n();
  const copy = async () => {
    try {
      await copyText(message.content);
      toast({ kind: "success", title: toastMessage("sessions.message.copied") });
    } catch {
      toast({ kind: "error", title: toastMessage("sessions.message.copyFailed") });
    }
  };
  return <header>
    <span>{messageRole(message.role)}</span>
    <span className="asb-session-message-time">{message.at ? <Time iso={message.at} /> : null}</span>
    {onBookmark && (bookmarked
      ? <span className="asb-session-message-saved">{t("sessions.bookmarks.saved")}</span>
      : <Button variant="unstyled" className="asb-session-message-bookmark" disabled={bookmarkBusy}
        onClick={onBookmark}>{t("sessions.bookmarks.save")}</Button>)}
    <Button variant="unstyled" className="asb-session-message-copy" onClick={() => void copy()}>
      {t("sessions.message.copy")}
    </Button>
  </header>;
}

/** Renders one read-only transcript entry and owns its local copy feedback. */
export function SessionMessageView({
  message,
  targeted,
  expanded,
  onToggleExpanded,
  onBookmark,
  bookmarked,
  bookmarkBusy,
}: Props) {
  const { t } = useI18n();
  const isLong = message.content.length > MESSAGE_COLLAPSE_THRESHOLD;
  const collapsed = isLong && !expanded;

  return (
    <article
      className={`asb-session-message is-${message.role.toLowerCase()}${targeted ? " is-target" : ""}`}
      data-message-id={message.id}
      tabIndex={targeted ? -1 : undefined}
    >
      <MessageHeader message={message} onBookmark={onBookmark} bookmarked={bookmarked} bookmarkBusy={bookmarkBusy} />
      <div className={`asb-session-message-body${collapsed ? " is-collapsed" : ""}`}>
        <Markdown remarkPlugins={[remarkGfm]} components={{ a: MarkdownLink }}>
          {message.content}
        </Markdown>
      </div>
      {isLong && onToggleExpanded && (
        <Button
          variant="unstyled"
          className="asb-session-message-toggle"
          aria-expanded={expanded}
          onClick={() => onToggleExpanded(message.id)}
        >
          {expanded
            ? t("sessions.message.collapse")
            : t("sessions.message.expand", { size: Math.round(message.content.length / 1000) })}
        </Button>
      )}
    </article>
  );
}
