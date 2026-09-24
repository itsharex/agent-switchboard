import { useId, useState } from "react";
import { useI18n } from "../../i18n";
import { Button } from "../Button";
import { Input } from "../Input";
import { PlusIcon } from "../icons";

export function SessionTagInput({ tags, onChange, disabled }: {
  tags: string[]; onChange: (tags: string[]) => void; disabled: boolean;
}) {
  const { t } = useI18n();
  const id = useId();
  const [draft, setDraft] = useState("");
  const add = () => {
    const value = draft.trim();
    if (disabled || !value) return;
    if (!tags.includes(value)) onChange([...tags, value]);
    setDraft("");
  };
  return <div className="asb-session-tag-input">
    <label htmlFor={id}>{t("sessions.organize.tags")}</label>
    <div className="asb-session-tag-draft">
      <Input id={id} value={draft} disabled={disabled} placeholder={t("sessions.organize.tagInput")}
        onChange={(event) => setDraft(event.target.value)} onKeyDown={(event) => {
          if (event.key === "Enter" && !event.nativeEvent.isComposing) { event.preventDefault(); add(); }
        }} />
      <Button variant="icon" disabled={disabled || !draft.trim()} onClick={add}
        aria-label={t("sessions.organize.addTag")} title={t("sessions.organize.addTag")}><PlusIcon /></Button>
    </div>
    {tags.length > 0 && <div className="asb-session-tag-values">{tags.map((tag) =>
      <Button variant="unstyled" key={tag} disabled={disabled} aria-label={t("sessions.organize.removeTag", { tag })}
        onClick={() => onChange(tags.filter((value) => value !== tag))}><span>{tag}</span><span aria-hidden="true">×</span></Button>)}</div>}
  </div>;
}
