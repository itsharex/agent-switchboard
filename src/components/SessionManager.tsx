import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  deleteSession,
  deleteSessions,
  getSessionMessages,
  listSessions,
  resumeSession,
  type SessionDeleteOutcome,
  type SessionIssue,
  type SessionMessage,
  type SessionMeta,
} from "../api/client";
import { ClientLogo } from "./ClientLogo";
import { ClientFilter, type ClientFilterValue } from "./ClientFilter";
import { Button } from "./Button";
import { ConfirmSheet } from "./ConfirmSheet";
import { Input } from "./Input";
import { toast } from "./use-toast";
import { SessionMessageView } from "./session/SessionMessageView";
import {
  codexOutlinePreview,
  copyText,
  directoryName,
  previewLine,
  sessionMatchesSearch,
} from "./session/session-content";
import { clientFullName } from "../lib/client-name";
import { Time } from "./Time";
import { WorkspaceHeader } from "./WorkspaceHeader";
import { PreviewIcon, RequestIcon, SearchIcon } from "./icons";

const TARGET_HIGHLIGHT_MS = 2000;
/* Within the TTL a re-activation or re-selection serves cache with zero
   requests; past it the cache is shown while a refresh runs in the
   background. */
const SESSION_CACHE_TTL_MS = 30_000;
/* Upper bound on retained transcripts so a long-running tray process cannot
   accumulate unbounded message memory; the oldest fetched entry is evicted. */
const MAX_CACHED_TRANSCRIPTS = 16;

const sessionKey = (session: { app: SessionMeta["app"]; sessionId: string }) =>
  `${session.app}:${session.sessionId}`;

/**
 * Local history browser. Client records remain read-only; an explicit resume
 * action invokes the backend-owned fixed command. Scan and transcript results
 * are cached in memory for `SESSION_CACHE_TTL_MS`, so revisiting the page or
 * a recently viewed session displays instantly without rescanning.
 */
export function SessionManager({ active }: { active: boolean }) {
  const [sessions, setSessions] = useState<SessionMeta[] | null>(null);
  const [issues, setIssues] = useState<SessionIssue[]>([]);
  const [scanError, setScanError] = useState<string | null>(null);
  const [filter, setFilter] = useState<ClientFilterValue>("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<SessionMeta | null>(null);
  const [messages, setMessages] = useState<SessionMessage[] | null>(null);
  const [scanning, setScanning] = useState(false);
  const [messageLoading, setMessageLoading] = useState(false);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [copyStatus, setCopyStatus] = useState<string | null>(null);
  const [resumeStatus, setResumeStatus] = useState<string | null>(null);
  const [resuming, setResuming] = useState(false);
  const [pendingDelete, setPendingDelete] = useState<SessionMeta | null>(null);
  const [deleting, setDeleting] = useState(false);
  /* Batch mode turns list clicks into selection toggles; the detail pane is
     untouched so a half-built selection never loses the open transcript. */
  const [selecting, setSelecting] = useState(false);
  const [chosen, setChosen] = useState<Set<string>>(() => new Set());
  const [pendingBatch, setPendingBatch] = useState(false);
  const [expandedMessages, setExpandedMessages] = useState<Set<number>>(() => new Set());
  const [targetMessage, setTargetMessage] = useState<number | null>(null);
  const selectionVersion = useRef(0);
  const scanVersion = useRef(0);
  const scanRequest = useRef<Promise<Awaited<ReturnType<typeof listSessions>>> | null>(null);
  const lastScanAt = useRef(0);
  const messageCache = useRef(new Map<string, { fetchedAt: number; messages: SessionMessage[] }>());
  const transcriptRef = useRef<HTMLDivElement | null>(null);
  const highlightTimer = useRef<number | undefined>(undefined);

  useEffect(() => () => window.clearTimeout(highlightTimer.current), []);

  const refresh = useCallback(async () => {
    const version = ++scanVersion.current;
    setScanning(true);
    setCopyStatus(null);
    setScanError(null);
    try {
      const scan = await (scanRequest.current ?? (() => {
        const next = listSessions();
        scanRequest.current = next;
        void next.finally(() => {
          if (scanRequest.current === next) scanRequest.current = null;
        }).catch(() => {});
        return next;
      })());
      if (scanVersion.current !== version) return;
      lastScanAt.current = Date.now();
      setSessions(scan.sessions);
      setIssues(scan.issues);
      setSelected((current) =>
        current
          ? scan.sessions.find((session) => session.app === current.app && session.sessionId === current.sessionId) ?? null
          : null,
      );
    } catch (caught) {
      if (scanVersion.current !== version) return;
      /* A failed refresh keeps the last displayed list (stale-while-revalidate);
         only a first-ever scan failure falls back to the empty state. */
      if (lastScanAt.current === 0) {
        setSessions([]);
        setIssues([]);
        setSelected(null);
      }
      setScanError((caught as { message?: string }).message ?? "无法扫描本地会话");
    } finally {
      if (scanVersion.current === version) setScanning(false);
    }
  }, []);

  /* The instance stays mounted across page switches. Activation serves the
     cached list directly inside the TTL and only rescans (in the background)
     when no scan has ever succeeded or the cache has gone stale; the decision
     reads the scan timestamp, never the list state, so a failed first scan
     does not immediately retrigger itself. */
  useEffect(() => {
    if (active && (lastScanAt.current === 0 || Date.now() - lastScanAt.current > SESSION_CACHE_TTL_MS)) {
      void refresh();
    }
  }, [active, refresh]);

  const filtered = useMemo(
    () =>
      (sessions ?? []).filter(
        (session) => (filter === "all" || session.app === filter) && sessionMatchesSearch(session, query),
      ),
    [filter, query, sessions],
  );

  const outlineItems = useMemo(() => {
    const previewOf = (content: string): string | null =>
      selected?.app === "codex" ? codexOutlinePreview(content) : content;
    return (
      messages
        ?.map((message, index) => ({ message, index }))
        .filter(({ message }) => message.role.toLowerCase() === "user")
        .map(({ message, index }) => {
          const raw = previewOf(message.content);
          return raw === null ? null : { index, preview: previewLine(raw) };
        })
        .filter((item): item is { index: number; preview: string } => item !== null) ?? []
    );
  }, [messages, selected?.app]);

  const selectSession = async (session: SessionMeta) => {
    const version = ++selectionVersion.current;
    setSelected(session);
    setDetailError(null);
    setCopyStatus(null);
    setResumeStatus(null);
    setResuming(false);
    setPendingDelete(null);
    setDeleting(false);
    setExpandedMessages(new Set());
    setTargetMessage(null);
    window.clearTimeout(highlightTimer.current);

    /* Recently viewed transcripts come straight from cache; a stale entry is
       shown immediately and revalidated in the background. */
    const key = `${session.app}:${session.sessionId}`;
    const cached = messageCache.current.get(key);
    if (cached) {
      setMessages(cached.messages);
      if (Date.now() - cached.fetchedAt <= SESSION_CACHE_TTL_MS) return;
    } else {
      setMessages(null);
      setMessageLoading(true);
    }
    try {
      const nextMessages = await getSessionMessages(session.app, session.sessionId);
      if (selectionVersion.current !== version) return;
      messageCache.current.set(key, { fetchedAt: Date.now(), messages: nextMessages });
      if (messageCache.current.size > MAX_CACHED_TRANSCRIPTS) {
        const oldest = [...messageCache.current.entries()].sort(
          ([, a], [, b]) => a.fetchedAt - b.fetchedAt,
        )[0];
        if (oldest) messageCache.current.delete(oldest[0]);
      }
      setMessages(nextMessages);
    } catch (caught) {
      if (selectionVersion.current === version) {
        setDetailError((caught as { message?: string }).message ?? "无法读取会话内容");
      }
    } finally {
      if (selectionVersion.current === version) setMessageLoading(false);
    }
  };

  const copy = async (text: string, label: string) => {
    const version = selectionVersion.current;
    try {
      await copyText(text);
      if (selectionVersion.current === version) setCopyStatus(`已复制${label}`);
    } catch {
      if (selectionVersion.current === version) setCopyStatus(`无法复制${label}`);
    }
  };

  const resume = async () => {
    if (!selected || resuming) return;
    const version = selectionVersion.current;
    const target = selected;
    setResuming(true);
    setResumeStatus(null);
    try {
      const result = await resumeSession(target.app, target.sessionId);
      if (selectionVersion.current === version) {
        setResumeStatus(
          result.usedProjectDir
            ? "已在新终端窗口中恢复会话"
            : "已在新终端窗口中启动恢复；原工作目录不可用",
        );
      }
    } catch (caught) {
      if (selectionVersion.current === version) {
        setResumeStatus((caught as { message?: string }).message ?? "无法启动会话恢复");
      }
    } finally {
      if (selectionVersion.current === version) setResuming(false);
    }
  };

  const toggleExpanded = useCallback((index: number) => {
    setExpandedMessages((current) => {
      const next = new Set(current);
      if (next.has(index)) {
        next.delete(index);
      } else {
        next.add(index);
      }
      return next;
    });
  }, []);

  /* Deletion is irreversible, so the shared confirmation sheet is the only
     gate; once confirmed the removal is reported by the session vanishing
     from the list plus a toast, never by a stale inline status. */
  const runDelete = async () => {
    const target = pendingDelete;
    if (!target || deleting) return;
    setPendingDelete(null);
    setDeleting(true);
    try {
      await deleteSession(target.app, target.sessionId);
      messageCache.current.delete(`${target.app}:${target.sessionId}`);
      setSessions((current) =>
        current
          ? current.filter(
              (session) => !(session.app === target.app && session.sessionId === target.sessionId),
            )
          : current,
      );
      setSelected((current) =>
        current && current.app === target.app && current.sessionId === target.sessionId
          ? null
          : current,
      );
      setMessages(null);
      toast({ kind: "success", title: "已删除会话" });
    } catch (caught) {
      toast({ kind: "error", title: (caught as { message?: string }).message ?? "无法删除会话" });
    } finally {
      setDeleting(false);
    }
  };

  /* Every removed record leaves the list and caches; the ones that stayed
     are named with their backend reason instead of being summarized away. */
  const dropDeleted = (outcomes: SessionDeleteOutcome[]) => {
    const removed = new Set(outcomes.filter((outcome) => outcome.deleted).map(sessionKey));
    for (const key of removed) messageCache.current.delete(key);
    setSessions((current) => current?.filter((session) => !removed.has(sessionKey(session))) ?? current);
    setSelected((current) => (current && removed.has(sessionKey(current)) ? null : current));
    if (selected && removed.has(sessionKey(selected))) setMessages(null);
    setChosen((current) => new Set([...current].filter((key) => !removed.has(key))));
    return removed.size;
  };

  const runBatchDelete = async () => {
    if (deleting || chosen.size === 0) return;
    const targets = (sessions ?? []).filter((session) => chosen.has(sessionKey(session)));
    setPendingBatch(false);
    setDeleting(true);
    try {
      const outcomes = await deleteSessions(
        targets.map((session) => ({ app: session.app, sessionId: session.sessionId })),
      );
      const removed = dropDeleted(outcomes);
      const failed = outcomes.filter((outcome) => !outcome.deleted);
      if (removed > 0) toast({ kind: "success", title: `已删除 ${removed} 个会话` });
      if (failed.length > 0) {
        const titleOf = (outcome: SessionDeleteOutcome) =>
          targets.find((session) => sessionKey(session) === sessionKey(outcome))?.title ?? outcome.sessionId;
        toast({
          kind: "error",
          title: `${failed.length} 个会话未能删除`,
          description: failed.map((outcome) => `${titleOf(outcome)}：${outcome.error ?? "未知原因"}`).join("；"),
        });
      }
    } catch (caught) {
      toast({ kind: "error", title: (caught as { message?: string }).message ?? "无法批量删除会话" });
    } finally {
      setDeleting(false);
    }
  };

  const toggleChosen = (session: SessionMeta) => {
    const key = sessionKey(session);
    setChosen((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  const chosenSessions = (sessions ?? []).filter((session) => chosen.has(sessionKey(session)));

  const jumpToMessage = useCallback((index: number) => {
    const node = transcriptRef.current?.querySelector<HTMLElement>(`[data-index="${index}"]`);
    /* Instant, not smooth: on multi-thousand-line transcripts the animation
       gets cancelled by the highlight re-render and strands the scroll. */
    node?.scrollIntoView?.({ block: "center" });
    setTargetMessage(index);
    window.clearTimeout(highlightTimer.current);
    highlightTimer.current = window.setTimeout(() => setTargetMessage(null), TARGET_HIGHLIGHT_MS);
  }, []);

  return (
    <div className="asb-sessions">
      <WorkspaceHeader
        title="会话记录"
        secondary={
          <>
            <Input
              aria-label="搜索会话"
              value={query}
              placeholder="搜索标题、摘要、目录或会话 ID"
              onChange={(event) => setQuery(event.target.value)}
            />
            <ClientFilter
              value={filter}
              onChange={setFilter}
              label="会话客户端筛选"
              showLogos
            />
            <Button variant="secondary" disabled={scanning} onClick={() => void refresh()}>
              刷新会话
            </Button>
            <Button
              variant="secondary"
              aria-pressed={selecting}
              disabled={deleting}
              onClick={() => {
                setSelecting((current) => !current);
                setChosen(new Set());
              }}
            >
              {selecting ? "退出批量选择" : "批量选择"}
            </Button>
            {selecting && (
              <Button
                variant="secondary"
                disabled={deleting || filtered.length === 0}
                onClick={() => setChosen(new Set(filtered.map(sessionKey)))}
              >
                全选筛选结果
              </Button>
            )}
          </>
        }
      />
      {selecting && chosen.size > 0 && (
        <div className="asb-session-selection-bar">
          <span>已选择 {chosen.size} 个会话</span>
          <Button
            variant="danger"
            disabled={deleting}
            onClick={() => setPendingBatch(true)}
          >
            删除所选（{chosen.size}）
          </Button>
        </div>
      )}
      {issues.length > 0 && (
        <ul className="asb-session-issues" aria-label="会话扫描提示">
          {issues.map((issue) => (
            <li key={`${issue.app}-${issue.message}`} className="asb-warn-text">
              {clientFullName(issue.app)}：{issue.message}
            </li>
          ))}
        </ul>
      )}
      {scanError && <p className="asb-warn-text" role="alert">{scanError}</p>}
      <div className="asb-session-layout">
        <section className="asb-session-list" aria-label="会话列表">
          <div className="asb-session-list-heading">
            <span>会话</span>
            <span className="asb-session-count">{filtered.length}</span>
          </div>
          {sessions === null ? (
            <div role="status" aria-label="正在扫描本地会话">
              <div className="asb-session-skeleton">
                {Array.from({ length: 5 }, (_, index) => (
                  <div key={index} className="asb-skeleton asb-session-skeleton-row" />
                ))}
              </div>
            </div>
          ) : filtered.length === 0 ? (
            <div className="asb-empty-state">
              <span className="asb-empty-state-icon" aria-hidden="true">
                <SearchIcon />
              </span>
              <h3 className="asb-section-title">未找到匹配的 Codex 或 Claude Code 会话</h3>
            </div>
          ) : (
            <div className="asb-session-items">
              {filtered.map((session) => {
                const active = selected?.app === session.app && selected.sessionId === session.sessionId;
                const picked = selecting && chosen.has(sessionKey(session));
                return (
                  <Button
                    variant="unstyled"
                    className={`asb-session-item${active ? " is-active" : ""}${picked ? " is-selected" : ""}`}
                    key={`${session.app}-${session.sessionId}`}
                    aria-pressed={selecting ? picked : active}
                    onClick={() => (selecting ? toggleChosen(session) : void selectSession(session))}
                  >
                    <span className="asb-session-item-title">
                      <ClientLogo app={session.app} className="asb-session-logo" />
                      <span>{session.title}</span>
                    </span>
                    <span className="asb-session-item-summary">{session.summary}</span>
                    <span className="asb-session-item-time">
                      {session.lastActiveAt ? <Time iso={session.lastActiveAt} /> : "时间未知"}
                    </span>
                  </Button>
                );
              })}
            </div>
          )}
        </section>
        <section className="asb-session-detail" aria-label="会话详情">
          {!selected ? (
            <div className="asb-empty-state">
              <span className="asb-empty-state-icon" aria-hidden="true">
                <PreviewIcon />
              </span>
              <h3 className="asb-section-title">选择一条会话即可查看内容并复制恢复命令。</h3>
            </div>
          ) : (
            <>
              <header className="asb-session-detail-head">
                <div className="asb-session-detail-title">
                  <span className="asb-session-client">
                    <ClientLogo app={selected.app} className="asb-session-logo" />
                    {clientFullName(selected.app)}
                  </span>
                  <h3 className="asb-section-title">{selected.title}</h3>
                </div>
                <div className="asb-session-actions">
                  <Button variant="primary" disabled={resuming} onClick={() => void resume()}>
                    {resuming ? "正在启动" : "在终端中恢复"}
                  </Button>
                  <Button
                    variant="secondary"
                    onClick={() => void copy(selected.resumeCommand, "恢复命令")}
                  >
                    复制恢复命令
                  </Button>
                  <Button
                    variant="secondary"
                    disabled={!selected.projectDir}
                    onClick={() => selected.projectDir && void copy(selected.projectDir, "工作目录")}
                  >
                    复制工作目录
                  </Button>
                </div>
              </header>
              <div className="asb-session-danger-row">
                <Button
                  variant="danger"
                  disabled={deleting}
                  onClick={() => setPendingDelete(selected)}
                >
                  删除会话
                </Button>
              </div>
              <p className="asb-session-meta-line">
                <span className="asb-code asb-session-meta-id">{selected.sessionId}</span>
                <span>{selected.lastActiveAt ? <Time iso={selected.lastActiveAt} /> : "时间未知"}</span>
                {selected.projectDir && (
                  <Button
                    variant="unstyled"
                    className="asb-session-meta-dir"
                    title={`${selected.projectDir}（点击复制）`}
                    onClick={() => selected.projectDir && void copy(selected.projectDir, "工作目录")}
                  >
                    {directoryName(selected.projectDir)}
                  </Button>
                )}
              </p>
              <div className="asb-session-command">
                <code className="asb-code">{selected.resumeCommand}</code>
              </div>
              {resumeStatus && <p className="asb-scope-note" role="status">{resumeStatus}</p>}
              {copyStatus && <p className="asb-scope-note" role="status">{copyStatus}</p>}
              <div className="asb-session-body">
                <div className="asb-session-conversation">
                  <div className="asb-session-list-heading">
                    <span>对话记录</span>
                    <span className="asb-session-count">{messages?.length ?? 0}</span>
                  </div>
                  <div className="asb-session-transcript" ref={transcriptRef} aria-label="对话历史">
                    {messageLoading && (
                      <div role="status" aria-label="正在读取会话">
                        <div className="asb-session-skeleton">
                          {Array.from({ length: 6 }, (_, index) => (
                            <div key={index} className="asb-skeleton asb-session-skeleton-row" />
                          ))}
                        </div>
                      </div>
                    )}
                    {detailError && <p className="asb-warn-text">{detailError}</p>}
                    {messages !== null && messages.length === 0 && (
                      <div className="asb-empty-state">
                        <span className="asb-empty-state-icon" aria-hidden="true">
                          <RequestIcon />
                        </span>
                        <h3 className="asb-section-title">会话中没有可展示的消息。</h3>
                      </div>
                    )}
                    {messages?.map((message, index) => (
                      <SessionMessageView
                        key={`${message.at ?? ""}-${index}`}
                        message={message}
                        index={index}
                        targeted={targetMessage === index}
                        expanded={expandedMessages.has(index)}
                        onToggleExpanded={toggleExpanded}
                      />
                    ))}
                  </div>
                </div>
                {outlineItems.length > 2 && (
                  <nav className="asb-session-toc" aria-label="消息目录">
                    <div className="asb-session-toc-heading">消息目录</div>
                    <div className="asb-session-toc-items">
                      {outlineItems.map((item, outlineIndex) => (
                        <Button
                          variant="unstyled"
                          key={item.index}
                          onClick={() => jumpToMessage(item.index)}
                        >
                          <span className="asb-session-toc-index">{outlineIndex + 1}</span>
                          <span className="asb-session-toc-preview">{item.preview}</span>
                        </Button>
                      ))}
                    </div>
                  </nav>
                )}
              </div>
            </>
          )}
        </section>
      </div>
      {pendingDelete && (
        <ConfirmSheet
          title="删除会话"
          confirmLabel="确认删除"
          destructive
          onConfirm={() => void runDelete()}
          onCancel={() => setPendingDelete(null)}
        >
          <ul className="asb-dialog-details">
            <li>将永久删除本地会话「{pendingDelete.title}」</li>
            <li>会话 ID {pendingDelete.sessionId}</li>
            <li>此操作不可恢复；Codex 或 Claude Code 将无法再恢复该会话。</li>
          </ul>
        </ConfirmSheet>
      )}
      {pendingBatch && chosenSessions.length > 0 && (
        <ConfirmSheet
          title="批量删除会话"
          confirmLabel="确认批量删除"
          destructive
          onConfirm={() => void runBatchDelete()}
          onCancel={() => setPendingBatch(false)}
        >
          <ul className="asb-dialog-details">
            <li>将永久删除 {chosenSessions.length} 个本地会话</li>
            {chosenSessions.slice(0, 3).map((session) => (
              <li key={session.sessionId}>{clientFullName(session.app)}「{session.title}」</li>
            ))}
            {chosenSessions.length > 3 && <li>以及另外 {chosenSessions.length - 3} 个会话</li>}
            <li>此操作不可恢复；每个会话的删除结果会单独报告。</li>
          </ul>
        </ConfirmSheet>
      )}
    </div>
  );
}
