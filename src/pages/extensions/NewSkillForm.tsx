import { Button } from "../../components/Button";
import { Input } from "../../components/Input";
import { useI18n } from "../../i18n";

interface Props {
  busy: boolean;
  newSkillName: string;
  setNewSkillName: (value: string) => void;
  newSkillDescription: string;
  setNewSkillDescription: (value: string) => void;
  createSkill: () => void | Promise<void>;
}

/** Creates one editable local Skill from the built-in template. */
export function NewSkillForm({
  busy,
  newSkillName,
  setNewSkillName,
  newSkillDescription,
  setNewSkillDescription,
  createSkill,
}: Props) {
  const { t } = useI18n();
  return (
    <form
      className="asb-form"
      aria-label={t("extensions.toolbar.newSkill")}
      onSubmit={(event) => {
        event.preventDefault();
        void createSkill();
      }}
    >
      <label className="asb-field">
        <span>{t("extensions.newSkill.name")}</span>
        <Input
          required
          placeholder="note-helper"
          value={newSkillName}
          disabled={busy}
          onChange={(event) => setNewSkillName(event.target.value)}
        />
      </label>
      <label className="asb-field">
        <span>{t("extensions.newSkill.description")}</span>
        <Input
          required
          placeholder={t("extensions.newSkill.descriptionPlaceholder")}
          value={newSkillDescription}
          disabled={busy}
          onChange={(event) => setNewSkillDescription(event.target.value)}
        />
      </label>
      <div className="asb-form-actions">
        <Button type="submit" variant="primary" disabled={busy}>
          {t("extensions.newSkill.create")}
        </Button>
      </div>
    </form>
  );
}
