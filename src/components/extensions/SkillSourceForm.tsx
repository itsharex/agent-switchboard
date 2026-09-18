import { FileArchive, FolderGit2, Settings2 } from "lucide-react";
import { Button } from "../Button";
import { Input } from "../Input";
import { RadioOption } from "../RadioOption";
import { Tooltip } from "../Tooltip";
import { FolderOpenIcon } from "../icons";
import type { SkillSourceKind } from "./skill-source-model";
import type { SkillSourceState } from "./useSkillSource";

const SOURCES: { value: SkillSourceKind; label: string }[] = [
  { value: "catalog", label: "仓库目录" }, { value: "directory", label: "skills.sh" },
  { value: "zip", label: "ZIP 文件" }, { value: "local", label: "本地目录" },
];

function SourceInput({ state, disabled, busy }: { state: SkillSourceState; disabled: boolean; busy: boolean }) {
  if (state.source === "catalog") return (
    <span className="asb-skill-source-catalog-count asb-num">
      <FolderGit2 size={16} />
      {state.repositories.loading ? "正在读取仓库…" : state.repositories.ready ?
        `${state.repositories.items.length} 个仓库 · ${state.repositories.items.filter((repo) => repo.enabled).length} 个启用` : "仓库列表未加载"}
    </span>
  );
  if (state.source === "directory") return (
    <Input type="search" aria-label="搜索 skills.sh" placeholder="搜索 skills.sh" value={state.directory.input}
      disabled={disabled} onChange={(event) => state.directory.changeInput(event.target.value)} />
  );
  const kind = state.source;
  return <>
    <Input required aria-label={kind === "zip" ? "ZIP 文件路径" : "本地来源目录"}
      placeholder={kind === "zip" ? "本地 .zip 文件路径" : "本地 Skills 目录"}
      value={state.files.paths[kind]} disabled={disabled}
      onChange={(event) => state.files.changePath(kind, event.target.value)} />
    <Button variant="secondary" disabled={busy || state.loading || state.imports.busy}
      onClick={() => void state.pick()}>
      {kind === "zip" ? <FileArchive size={16} /> : <FolderOpenIcon />}
      {kind === "zip" ? "选择 ZIP" : "浏览目录"}
    </Button>
  </>;
}

export function SkillSourceForm({ state, busy }: { state: SkillSourceState; busy: boolean }) {
  const disabled = state.imports.busy || (busy && !state.loading);
  return (
    <form id="skill-source-scan" className="asb-skill-source-form" aria-label="Skill 来源" onSubmit={(event) => {
      event.preventDefault(); void state.search();
    }}>
      <div className="asb-skill-source-input-row">
        <div className="asb-segments" role="radiogroup" aria-label="Skill 来源类型">
          {SOURCES.map(({ value, label }) => (
            <RadioOption key={value} name="skill-source-type" checked={state.source === value}
              disabled={disabled} label={label} onChange={() => state.changeSource(value)} />
          ))}
        </div>
        <Tooltip label="管理仓库">
          <Button variant="icon" aria-label="管理仓库" disabled={disabled}
            onClick={() => state.setManagerOpen(true)}><Settings2 size={16} /></Button>
        </Tooltip>
      </div>
      <div className="asb-skill-source-input-row">
        <SourceInput state={state} disabled={disabled} busy={busy} />
      </div>
    </form>
  );
}
