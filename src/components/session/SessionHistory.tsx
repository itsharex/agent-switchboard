import type { SessionMeta } from "../../api/client";
import { useI18n } from "../../i18n";
import { clientFullName } from "../../lib/client-name";
import { SessionDetail } from "./SessionDetail";
import { SessionSearchList } from "./SessionSearchList";
import { SessionSelectionToolbar } from "./SessionSelectionToolbar";
import { sessionKey } from "./session-content";
import type { SessionSearch } from "./use-session-search";
import type { SessionDetailState } from "./use-session-detail";
import type { SessionBookmarks } from "./use-session-bookmarks";
import type { SessionOrganization } from "./use-session-organization";
import type { SessionDeletionState } from "./SessionDeletion";

export function SessionHistory({ search, detail, bookmarks, organization, deletion, onOpen }: {
  search: SessionSearch; detail: SessionDetailState; bookmarks: SessionBookmarks;
  organization: SessionOrganization; deletion: SessionDeletionState;
  onOpen: (session: SessionMeta, messageId: string | null) => void;
}) {
  const { t } = useI18n();
  const busy = deletion.busy || organization.busy;
  return <>
    {deletion.selecting && <SessionSelectionToolbar search={search} selection={deletion} organization={organization} />}
    {search.result && search.result.issues.length > 0 && <ul className="asb-session-issues" aria-label={t("sessions.issues.aria")}>
      {search.result.issues.map((issue, index) => <li key={index} className="asb-warn-text">
        {t("sessions.issues.item", { client: clientFullName(issue.app), message: issue.message })}
      </li>)}
    </ul>}
    <div className="asb-session-layout">
      <SessionSearchList search={search} selected={detail.selected} selecting={deletion.selecting}
        chosen={deletion.chosen} disabled={busy} onToggle={deletion.toggle}
        onToggleMode={deletion.toggleMode}
        onSelect={(hit) => onOpen(hit.session, hit.messageId)} />
      {detail.selected ? <SessionDetail key={sessionKey(detail.selected)} detail={detail} session={detail.selected}
        bookmarks={bookmarks} organization={organization} deleting={busy} onDelete={(session) => deletion.setPending([session])} />
        : <section className="asb-session-detail asb-session-empty-detail" aria-label={t("sessions.detail.aria")}>
          <p>{t("sessions.detail.empty")}</p>
        </section>}
    </div>
  </>;
}
