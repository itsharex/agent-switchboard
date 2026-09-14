import { useState } from "react";
import { Save } from "lucide-react";
import type { SkillRepository, SkillRepositoryInput } from "../../api/extensions/skill-sources";
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
  const [repo, setRepo] = useState(initial ? skillRepositoryName(initial.repo) ?? "" : "");
  const [refName, setRefName] = useState(initial?.refName ?? "");
  const [subpath, setSubpath] = useState(initial?.subpath ?? "");
  const [enabled, setEnabled] = useState(initial?.enabled ?? true);
  const [error, setError] = useState<string | null>(null);
  return (
    <form className="asb-skill-repository-form" aria-label={initial ? "编辑 Skill 仓库" : "添加 Skill 仓库"}
      onSubmit={async (event) => {
        event.preventDefault();
        if (busy) return;
        const name = skillRepositoryName(repo);
        if (!name) { setError("请输入 owner/repository 或不含凭据、查询参数的 GitHub 仓库 URL"); return; }
        setError(null);
        if (await onSave({ ...(initial ? { id: initial.id } : {}), repo: name,
          refName: refName.trim() || null, subpath: subpath.trim(), enabled })) onCancel();
      }}>
      <h3 className="asb-group-title">{initial ? "编辑仓库" : "添加仓库"}</h3>
      <label className="asb-field">
        <span>GitHub 仓库</span>
        <Input required placeholder="owner/repository 或 GitHub 仓库 URL" value={repo} disabled={busy}
          onChange={(event) => { setRepo(event.target.value); setError(null); }} />
      </label>
      <div className="asb-skill-source-options">
        <label className="asb-field">
          <span>分支或提交（可选）</span>
          <Input placeholder="默认分支" value={refName} disabled={busy}
            onChange={(event) => setRefName(event.target.value)} />
        </label>
        <label className="asb-field">
          <span>Skill 子目录（可选）</span>
          <Input placeholder="整个仓库" value={subpath} disabled={busy}
            onChange={(event) => setSubpath(event.target.value)} />
        </label>
      </div>
      <Checkbox label="启用仓库" checked={enabled} onChange={setEnabled} disabled={busy} />
      {error && <p className="asb-warn-text" role="alert">{error}</p>}
      <div className="asb-skill-repository-actions">
        <Button variant="secondary" disabled={busy} onClick={onCancel}>取消</Button>
        <Button variant="primary" type="submit" disabled={busy}><Save size={16} />保存仓库</Button>
      </div>
    </form>
  );
}
