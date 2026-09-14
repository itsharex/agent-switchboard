import { useState } from "react";
import { ArrowLeft, Pencil, Plus, RefreshCw, Trash2 } from "lucide-react";
import type { SkillRepository } from "../../api/extensions/skill-sources";
import { Button } from "../Button";
import { Checkbox } from "../Checkbox";
import { Tooltip } from "../Tooltip";
import { SkillRepositoryForm } from "./SkillRepositoryForm";
import { SkillSourceLink } from "./SkillSourceCandidate";
import { skillRepositoryName, skillSourceUrl } from "./skill-repository-model";
import type { SkillSourceState } from "./useSkillSource";

function RepositoryRow({ repo, count, failed, busy, removing, onEdit, onSave, onRemove, onConfirm, onCancel }: {
  repo: SkillRepository; count: number | null; failed: boolean; busy: boolean; removing: boolean;
  onEdit: () => void; onSave: (enabled: boolean) => void; onRemove: () => void;
  onConfirm: () => void; onCancel: () => void;
}) {
  const name = skillRepositoryName(repo.repo) ?? "GitHub 仓库";
  return (
    <li className="asb-skill-repository-row" aria-label={name}>
      <div className="asb-skill-repository-info">
        <h3 className="asb-group-title">{name}</h3>
        <p className="asb-skill-source-origin">
          <span>{repo.refName || "默认分支"} · {repo.subpath || "整个仓库"}</span>
          <span className="asb-num">{count === null ? "未扫描" : `${count} 个 Skills`}</span>
          {failed && <span className="asb-warn-text">刷新失败</span>}
        </p>
      </div>
      <div className="asb-skill-repository-actions">
        <Checkbox label="启用" ariaLabel={`启用仓库 ${name}`} checked={repo.enabled} disabled={busy} onChange={onSave} />
        <SkillSourceLink url={skillSourceUrl(repo.repo)} label={`查看仓库 ${name}`} />
        <Tooltip label={`编辑仓库 ${name}`}>
          <Button variant="icon" disabled={busy} aria-label={`编辑仓库 ${name}`} onClick={onEdit}><Pencil size={16} /></Button>
        </Tooltip>
        <Tooltip label={`移除仓库 ${name}`}>
          <Button variant="icon" disabled={busy} aria-label={`移除仓库 ${name}`} onClick={onRemove}><Trash2 size={16} /></Button>
        </Tooltip>
      </div>
      {removing && <div className="asb-skill-repository-remove">
        <p>仅移除发现来源，已安装的 Skill 保持不变。</p>
        <Button variant="secondary" disabled={busy} onClick={onCancel}>取消移除</Button>
        <Button variant="danger" disabled={busy} onClick={onConfirm}><Trash2 size={16} />确认移除仓库</Button>
      </div>}
    </li>
  );
}

export function SkillRepositoryManager({ state, busy }: { state: SkillSourceState; busy: boolean }) {
  const [editing, setEditing] = useState<SkillRepository | null>(null);
  const [formOpen, setFormOpen] = useState(state.repositories.ready && state.repositories.items.length === 0);
  const [removing, setRemoving] = useState<string | null>(null);
  const { repositories, catalog } = state;
  const disabled = busy || !repositories.ready || repositories.loading || state.loading || state.imports.busy;
  return (
    <section className="asb-skill-repository-manager" aria-label="Skill 仓库">
      <header className="asb-skill-repository-manager-heading">
        <div>
          <h2 className="asb-section-title">Skill 仓库</h2>
          <p className="asb-scope-note">管理发现来源，已安装的 Skill 不会被移除。</p>
        </div>
        <Button variant="secondary" disabled={repositories.loading} onClick={() => state.setManagerOpen(false)}>
          <ArrowLeft size={16} />返回发现
        </Button>
      </header>
      <div className="asb-skill-source-toolbar">
        <span className="asb-skill-source-catalog-count asb-num">{repositories.items.length} 个仓库</span>
        <Button variant="secondary" disabled={disabled} onClick={() => { setEditing(null); setFormOpen(true); }}>
          <Plus size={16} />添加仓库
        </Button>
      </div>
      {repositories.error && <div className="asb-skill-source-error" role="alert">
        <span>{repositories.error}</span>
        <Button variant="secondary" disabled={busy || repositories.loading || state.imports.busy}
          onClick={() => void repositories.reload()}>
          <RefreshCw size={16} />重试加载仓库
        </Button>
      </div>}
      {formOpen && <SkillRepositoryForm key={editing?.id ?? "new"} initial={editing} busy={disabled}
        onSave={repositories.save} onCancel={() => { setFormOpen(false); setEditing(null); }} />}
      {repositories.loading && <p className="asb-skill-source-count" role="status">正在更新仓库列表…</p>}
      {!repositories.loading && repositories.ready && repositories.items.length === 0 &&
        <div className="asb-skill-source-empty"><p>尚未添加仓库</p></div>}
      <ul className="asb-skill-repository-list" aria-label="已保存的 Skill 仓库">
        {repositories.items.map((repo) => <RepositoryRow key={repo.id} repo={repo} count={catalog.count(repo.id)}
          failed={catalog.result?.failures.some((failure) => failure.repositoryId === repo.id) ?? false}
          busy={disabled} removing={removing === repo.id}
          onEdit={() => { setEditing(repo); setFormOpen(true); setRemoving(null); }}
          onSave={(enabled) => void repositories.save({ ...repo, enabled })}
          onRemove={() => setRemoving(repo.id)} onCancel={() => setRemoving(null)}
          onConfirm={() => { void repositories.remove(repo.id).then((saved) => { if (saved) setRemoving(null); }); }} />)}
      </ul>
    </section>
  );
}
