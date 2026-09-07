import { Button } from "../../components/Button";
import { Input } from "../../components/Input";

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
  return (
    <form
      className="asb-form"
      aria-label="新建本地 Skill"
      onSubmit={(event) => {
        event.preventDefault();
        void createSkill();
      }}
    >
      <label className="asb-field">
        <span>Skill 名称（小写字母、数字、连字符）</span>
        <Input
          required
          placeholder="note-helper"
          value={newSkillName}
          disabled={busy}
          onChange={(event) => setNewSkillName(event.target.value)}
        />
      </label>
      <label className="asb-field">
        <span>描述（通用 Skill 必填）</span>
        <Input
          required
          placeholder="这个 Skill 做什么、何时使用"
          value={newSkillDescription}
          disabled={busy}
          onChange={(event) => setNewSkillDescription(event.target.value)}
        />
      </label>
      <div className="asb-form-actions">
        <Button type="submit" variant="primary" disabled={busy}>
          从模板创建
        </Button>
      </div>
    </form>
  );
}
