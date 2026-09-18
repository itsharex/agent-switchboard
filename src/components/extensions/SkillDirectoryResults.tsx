import { Download, FolderGit2, LoaderCircle, Plus } from "lucide-react";
import type { ExtensionListItem } from "../../api/client";
import type { SkillDirectoryEntry } from "../../api/extensions/skill-sources";
import { Button } from "../Button";
import { SearchIcon } from "../icons";
import { ExtensionLoading } from "./ExtensionLoading";
import { skillCandidateInstalled } from "./skill-source-model";
import { skillRepositoryLabel, skillSourceUrl } from "./skill-repository-model";
import { SkillSourceCandidate, SkillSourceLink } from "./SkillSourceCandidate";
import type { SkillSourceState } from "./useSkillSource";
import { skillDirectoryKey } from "./useSkillSourceDirectory";

function DirectoryEntry({ entry, state, busy }: { entry: SkillDirectoryEntry; state: SkillSourceState; busy: boolean }) {
  const key = skillDirectoryKey(entry);
  const resolving = state.directory.resolving === key;
  const empty = state.directory.resolutions[key]?.length === 0;
  return (
    <li className="asb-skill-source-card" aria-label={entry.name}>
      <div className="asb-skill-source-card-heading">
        <h3 className="asb-group-title">{entry.name}</h3>
        <SkillSourceLink url={skillSourceUrl(entry.repo, entry.readmeUrl)} label={`查看 ${entry.name} 来源`} />
      </div>
      <p className="asb-skill-source-origin"><FolderGit2 size={14} /><span>{skillRepositoryLabel(entry)}</span></p>
      {empty && <p className="asb-warn-text">未找到 SKILL.md</p>}
      <div className="asb-skill-source-card-footer">
        <span className="asb-skill-source-meta asb-num">{entry.installs.toLocaleString()} 次目录安装</span>
        <Button variant="primary" disabled={busy || state.loading || state.imports.busy}
          aria-label={`${resolving ? "正在解析" : empty ? "重试解析" : "安装"} ${entry.name}`}
          onClick={() => void state.installDirectory(entry)}>
          {resolving ? <LoaderCircle size={16} className="asb-skill-source-spinner" /> : <Download size={16} />}
          {resolving ? "正在解析" : empty ? "重试解析" : "安装"}
        </Button>
      </div>
    </li>
  );
}

export function SkillDirectoryResults({ state, busy, items }: {
  state: SkillSourceState; busy: boolean; items: ExtensionListItem[];
}) {
  const { directory } = state;
  if (!directory.result) return directory.searching ? (
    <section className="asb-skill-source-results" aria-label="skills.sh 搜索结果" aria-busy="true">
      <ExtensionLoading />
    </section>
  ) : (
    <div className="asb-skill-source-empty" role="status">
      <SearchIcon />
      <p>尚未搜索 skills.sh</p>
    </div>
  );
  return (
    <section className="asb-skill-source-results" aria-label="skills.sh 搜索结果" aria-busy={directory.loading}>
      <p className="asb-skill-source-count asb-num" role="status">
        {directory.result.query} · 已加载 {directory.result.items.length} / {directory.result.total} 个 Skills
      </p>
      {directory.result.items.length === 0 ? <div className="asb-skill-source-empty"><p>没有匹配的 Skill</p></div> :
        <ul className="asb-skill-source-grid">
          {directory.result.items.flatMap((entry) => {
            const key = skillDirectoryKey(entry);
            const candidates = directory.resolutions[key];
            return candidates?.length ? candidates.map((candidate) => (
              <SkillSourceCandidate key={`${key}:${candidate.digest}`} row={{
                key, candidate, label: skillRepositoryLabel(entry), sourceUrl: skillSourceUrl(entry.repo, entry.readmeUrl),
              }} installed={skillCandidateInstalled(candidate, items)} imports={state.imports} source="directory"
                busy={busy || state.loading} installs={entry.installs} />
            )) : [<DirectoryEntry key={key} entry={entry} state={state} busy={busy} />];
          })}
        </ul>}
      {directory.result.hasMore && <div className="asb-skill-source-more">
        <Button variant="secondary" disabled={busy || state.loading || state.imports.busy ||
          directory.input.trim() !== directory.result.query} onClick={() => void directory.loadMore()}>
          {directory.searching ? <LoaderCircle size={16} className="asb-skill-source-spinner" /> : <Plus size={16} />}
          加载更多
        </Button>
      </div>}
    </section>
  );
}
