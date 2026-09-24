import { useEffect, useState } from "react";
import type { SessionMeta, SessionOrganizationChange } from "../../api/client";
import { useI18n } from "../../i18n";
import { AppDialog } from "../AppDialog";
import { Button } from "../Button";
import { Input } from "../Input";
import { SessionTagInput } from "./SessionTagInput";
import type { SessionOrganization } from "./use-session-organization";

export function SessionOrganizationEditor({ session, organization, disabled, onClose }: {
  session: SessionMeta; organization: SessionOrganization; disabled: boolean; onClose: () => void;
}) {
  const { t } = useI18n();
  const [alias, setAlias] = useState(session.alias ?? "");
  const [tags, setTags] = useState(session.tags);
  const [submitted, setSubmitted] = useState(false);
  const savedTags = JSON.stringify(session.tags);
  useEffect(() => setAlias(session.alias ?? ""), [session.alias]);
  useEffect(() => setTags(JSON.parse(savedTags) as string[]), [savedTags]);
  const busy = disabled || organization.busy;
  const save = (change: SessionOrganizationChange) => {
    setSubmitted(true);
    void organization.save([session], change);
  };
  return <AppDialog title={t("sessions.organize.heading")} busy={organization.busy} onClose={onClose}>
    <p className="asb-scope-note">{t("sessions.organize.note")}</p>
    <p className="asb-session-original-title" title={session.title}>{t("sessions.organize.original", { title: session.title })}</p>
    <div className="asb-session-organization-fields">
      <label>{t("sessions.organize.alias")}<Input value={alias} maxLength={120} disabled={busy}
        onChange={(event) => setAlias(event.target.value)} /></label>
      <Button variant="secondary" disabled={busy || alias.trim() === (session.alias ?? "")}
        onClick={() => save({ kind: "alias", alias: alias.trim() || null })}>{t("sessions.organize.saveAlias")}</Button>
      <SessionTagInput tags={tags} onChange={setTags} disabled={busy} />
      <Button variant="secondary" disabled={busy || JSON.stringify(tags) === savedTags}
        onClick={() => save({ kind: "setTags", tags })}>{t("sessions.organize.saveTags")}</Button>
    </div>
    {submitted && organization.status && <p className="asb-scope-note" role="status">{organization.status}</p>}
    {submitted && organization.error && <p className="asb-warn-text" role="alert">{organization.error}</p>}
  </AppDialog>;
}
