import { useState } from "react";
import type { SessionMeta } from "../api/client";
import { useI18n } from "../i18n";
import { Button } from "./Button";
import type { ClientFilterValue } from "./ClientFilter";
import { Input } from "./Input";
import { Tabs } from "./Tabs";
import { WorkspaceHeader } from "./WorkspaceHeader";
import { SessionBookmarksPane } from "./session/SessionBookmarksPane";
import { SessionDeletion, useSessionDeletion } from "./session/SessionDeletion";
import { SessionPageActions, SessionSearchControls } from "./session/SessionSearchControls";
import { SessionHistory } from "./session/SessionHistory";
import { useSessionOrganization } from "./session/use-session-organization";
import { sessionKey } from "./session/session-content";
import { useSessionBookmarks } from "./session/use-session-bookmarks";
import { useSessionDetail } from "./session/use-session-detail";
import { useSessionSearch } from "./session/use-session-search";

export function SessionManager({ active }: { active: boolean }) {
  const { t } = useI18n();
  const [view, setView] = useState<"history" | "bookmarks">("history");
  const [bookmarkQuery, setBookmarkQuery] = useState("");
  const [bookmarkFilter, setBookmarkFilter] = useState<ClientFilterValue>("all");
  const search = useSessionSearch(active && view === "history");
  const detail = useSessionDetail();
  const bookmarks = useSessionBookmarks(active);
  const deletion = useSessionDeletion((removed) => {
    if (detail.selected && removed.has(sessionKey(detail.selected))) detail.clear();
    search.refresh();
  });
  const organization = useSessionOrganization((updated) => {
    detail.updateMetadata(updated); deletion.updateMetadata(updated); search.refresh();
  });
  const busy = deletion.busy || organization.busy;
  const open = (session: SessionMeta, messageId: string | null = null) => {
    setView("history");
    void detail.select(session, messageId);
  };
  const refresh = () => {
    search.refresh(); bookmarks.refresh();
    if (detail.selected) void detail.select(detail.selected);
  };
  const changeView = (next: "history" | "bookmarks") => {
    if (next === "bookmarks") {
      deletion.setPending([]);
      if (deletion.selecting) deletion.toggleMode();
    }
    setView(next);
  };
  return <div className="asb-sessions">
    <WorkspaceHeader title={t("nav.page.sessions")} primary={
      <Tabs value={view} onChange={changeView} scope="sessions" label={t("sessions.views")} tabs={[
        { value: "history", label: t("sessions.list.heading"), controls: "sessions-history-panel", disabled: busy },
        { value: "bookmarks", label: t("sessions.bookmarks.title"), controls: "sessions-bookmarks-panel", disabled: busy },
      ]} />
    } primaryActions={<SessionPageActions filter={view === "history" ? search.filter : bookmarkFilter}
      onFilterChange={view === "history" ? search.changeFilter : setBookmarkFilter}
      refresh={view === "history" ? refresh : bookmarks.refresh}
      refreshKey={view === "history" ? "sessions.refresh" : "sessions.bookmarks.refresh"}
      busy={view === "history" ? search.busy || busy : bookmarks.loading || bookmarks.busy} />}
      secondary={view === "history" ? <SessionSearchControls search={search} disabled={busy} />
        : <div className="asb-session-bookmark-search"><Input type="search" value={bookmarkQuery}
          onChange={(event) => setBookmarkQuery(event.target.value)} aria-label={t("sessions.bookmarks.search")}
          placeholder={t("sessions.bookmarks.search")} /></div>} />
    {bookmarks.error && <div className="asb-session-bookmark-error" role="alert">
      <p className="asb-warn-text">{bookmarks.error}</p>
      <Button variant="secondary" disabled={bookmarks.loading || bookmarks.busy}
        onClick={bookmarks.refresh}>{t("sessions.bookmarks.refresh")}</Button>
    </div>}
    {view === "history" && organization.error && <p className="asb-warn-text" role="alert">{organization.error}</p>}
    {view === "history" && organization.status && <p className="asb-scope-note" role="status">{organization.status}</p>}
    {view === "history" && <SessionDeletion state={deletion} disabled={organization.busy} />}
    <div className="asb-session-panel" id="sessions-history-panel" role="tabpanel" aria-labelledby="sessions-history-tab" hidden={view !== "history"}>
      <SessionHistory search={search} detail={detail} bookmarks={bookmarks} organization={organization}
        deletion={deletion} onOpen={open} />
    </div>
    <div className="asb-session-panel" id="sessions-bookmarks-panel" role="tabpanel" aria-labelledby="sessions-bookmarks-tab" hidden={view !== "bookmarks"}>
      {active && view === "bookmarks" && <SessionBookmarksPane bookmarks={bookmarks} onOpen={open}
        query={bookmarkQuery} filter={bookmarkFilter} />}
    </div>
  </div>;
}
