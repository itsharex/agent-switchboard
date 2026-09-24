import { uiMessage } from "../../i18n/errors";
import { useMessageState } from "../../i18n/use-message-state";
import { useState } from "react";
import { Save } from "lucide-react";
import type { SkillRepository, SkillRepositoryInput } from "../../api/extensions/skill-sources";
import { useI18n } from "../../i18n";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Input } from "../Input";
import { skillRepositoryName } from "./skill-repository-model";

interface Props {
  initial: SkillRepository | null;
  busy: boolean;
  onSave: (input: SkillRepositoryInput) => Promise<boolean>;
  onCancel: () => void;
}

export function SkillRepositoryForm({ initial, busy, onSave, onCancel }: Props) {
  const { t } = useI18n();
  const [repo, setRepo] = useState(initial ? skillRepositoryName(initial.repo) ?? "" : "");
  const [refName, setRefName] = useState(initial?.refName ?? "");
  const [subpath, setSubpath] = useState(initial?.subpath ?? "");
  const [enabled, setEnabled] = useState(initial?.enabled ?? true);
  const [error, setError] = useMessageState();
  return (
    <form className="asb-skill-repository-form" aria-label={initial ? t("extensions.repo.formEditAria") : t("extensions.repo.formAddAria")}
      onSubmit={async (event) => {
        event.preventDefault();
        if (busy) return;
        const name = skillRepositoryName(repo);
        if (!name) { setError(uiMessage("extensions.repo.errorInvalid")); return; }
        setError(null);
        if (await onSave({ ...(initial ? { id: initial.id } : {}), repo: name,
          refName: refName.trim() || null, subpath: subpath.trim(), enabled })) onCancel();
      }}>
      <h3 className="asb-group-title">{initial ? t("extensions.repo.editTitle") : t("extensions.repo.addTitle")}</h3>
      <label className="asb-field">
        <span>{t("extensions.repo.repoLabel")}</span>
        <Input required placeholder={t("extensions.repo.repoPlaceholder")} value={repo} disabled={busy}
          onChange={(event) => { setRepo(event.target.value); setError(null); }} />
      </label>
      <div className="asb-skill-source-options">
        <label className="asb-field">
          <span>{t("extensions.repo.refLabel")}</span>
          <Input placeholder={t("extensions.repo.refPlaceholder")} value={refName} disabled={busy}
            onChange={(event) => setRefName(event.target.value)} />
        </label>
        <label className="asb-field">
          <span>{t("extensions.repo.subpathLabel")}</span>
          <Input placeholder={t("extensions.repo.subpathPlaceholder")} value={subpath} disabled={busy}
            onChange={(event) => setSubpath(event.target.value)} />
        </label>
      </div>
      <Checkbox label={t("extensions.repo.enable")} checked={enabled} onChange={setEnabled} disabled={busy} />
      {error && <p className="asb-warn-text" role="alert">{error}</p>}
      <div className="asb-skill-repository-actions">
        <Button variant="secondary" disabled={busy} onClick={onCancel}>{t("confirm.cancel")}</Button>
        <Button variant="primary" type="submit" disabled={busy}><Save size={16} />{t("extensions.repo.save")}</Button>
      </div>
    </form>
  );
}
