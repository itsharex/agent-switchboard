import { useEffect, useState } from "react";
import type { SessionMeta } from "../../api/client";
import { useI18n } from "../../i18n";
import { Button } from "../Button";
import { Input } from "../Input";
import { SessionTagInput } from "./SessionTagInput";
import type { SessionOrganization } from "./use-session-organization";

export function SessionOrganizationEditor({ session, organization, disabled }: {
  session: SessionMeta; organization: SessionOrganization; disabled: boolean;
}) {
  const { t } = useI18n();
  const [alias, setAlias] = useState(session.alias ?? "");
  const [tags, setTags] = useState(session.tags);
  const savedTags = JSON.stringify(session.tags);
  useEffect(() => setAlias(session.alias ?? ""), [session.alias]);
  useEffect(() => setTags(JSON.parse(savedTags) as string[]), [savedTags]);
  const busy = disabled || organization.busy;
  return <details className="asb-session-organize">
    <summary>{t("sessions.organize.heading")}</summary>
    <p className="asb-scope-note">{t("sessions.organize.note")}</p>
    <p className="asb-scope-note">{t("sessions.organize.original", { title: session.title })}</p>
    <div className="asb-session-organization-fields">
      <label>{t("sessions.organize.alias")}<Input value={alias} maxLength={120} disabled={busy}
        onChange={(event) => setAlias(event.target.value)} /></label>
      <Button variant="secondary" disabled={busy || alias.trim() === (session.alias ?? "")}
        onClick={() => void organization.save([session], { kind: "alias", alias: alias.trim() || null })}>{t("sessions.organize.saveAlias")}</Button>
      <SessionTagInput tags={tags} onChange={setTags} disabled={busy} />
      <Button variant="secondary" disabled={busy || JSON.stringify(tags) === savedTags}
        onClick={() => void organization.save([session], { kind: "setTags", tags })}>{t("sessions.organize.saveTags")}</Button>
    </div>
  </details>;
}
