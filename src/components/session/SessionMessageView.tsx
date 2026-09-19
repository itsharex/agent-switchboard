import type { ReactNode } from "react";
import Markdown from "react-markdown";
import remarkGfm from "remark-gfm";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { SessionMessage } from "../../api/client";
import { Time } from "../Time";
import { Button } from "../Button";
import { toast } from "../use-toast";
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
  index: number;
  targeted: boolean;
  expanded: boolean;
  onToggleExpanded: (index: number) => void;
}

/** Renders one read-only transcript entry and owns its local copy feedback. */
export function SessionMessageView({
  message,
  index,
  targeted,
  expanded,
  onToggleExpanded,
}: Props) {
  const isLong = message.content.length > MESSAGE_COLLAPSE_THRESHOLD;
  const collapsed = isLong && !expanded;

  const copy = async () => {
    try {
      await copyText(message.content);
      toast({ kind: "success", title: "已复制消息内容" });
    } catch {
      toast({ kind: "error", title: "无法复制消息内容" });
    }
  };

  return (
    <article
      className={`asb-session-message is-${message.role.toLowerCase()}${targeted ? " is-target" : ""}`}
      data-index={index}
    >
      <header>
        <span>{messageRole(message.role)}</span>
        <span className="asb-session-message-time">
          {message.at ? <Time iso={message.at} /> : null}
        </span>
        <Button variant="unstyled" className="asb-session-message-copy" onClick={() => void copy()}>
          复制
        </Button>
      </header>
      <div className={`asb-session-message-body${collapsed ? " is-collapsed" : ""}`}>
        <Markdown remarkPlugins={[remarkGfm]} components={{ a: MarkdownLink }}>
          {message.content}
        </Markdown>
      </div>
      {isLong && (
        <Button
          variant="unstyled"
          className="asb-session-message-toggle"
          aria-expanded={expanded}
          onClick={() => onToggleExpanded(index)}
        >
          {expanded ? "收起" : `展开完整内容（约 ${Math.round(message.content.length / 1000)}k 字符）`}
        </Button>
      )}
    </article>
  );
}
