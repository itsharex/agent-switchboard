import { LoaderCircle, Plus, RefreshCw } from "lucide-react";
import type { ReactNode } from "react";
import type { ExtensionListItem } from "../../api/client";
import { Button } from "../Button";
import { EditorFrame } from "../EditorFrame";
import { Input } from "../Input";
import { Select } from "../Select";
import { SearchIcon } from "../icons";
import { ExtensionLoading } from "./ExtensionLoading";
import { SkillDirectoryResults } from "./SkillDirectoryResults";
import { SkillRepositoryManager } from "./SkillRepositoryManager";
import { SkillSourceCandidate } from "./SkillSourceCandidate";
import { SkillSourceForm } from "./SkillSourceForm";
import { sourceErrorMessage, type SkillSourceFilter } from "./skill-source-model";
import { skillRepositoryLabel, skillRepositoryName } from "./skill-repository-model";
import { useSkillSource, type SkillSourceActions, type SkillSourceState } from "./useSkillSource";

interface Props extends SkillSourceActions {
  busy: boolean;
  items: ExtensionListItem[];
  onBack: () => void;
  /** Persistent failure notice rendered at the top of the scroll body. */
  notice?: ReactNode;
}

function SourceFilters({ state }: { state: SkillSourceState }) {
  return <div className="asb-skill-source-toolbar">
    <div className="asb-skill-source-search">
      <SearchIcon />
      <Input type="search" placeholder="搜索名称、描述或来源" aria-label="筛选发现的 Skills"
        value={state.query} onChange={(event) => state.setQuery(event.target.value)} />
    </div>
    {state.source === "catalog" && <Select value={state.repository} onChange={state.setRepository}
      ariaLabel="Skill 来源仓库" options={[
        { value: "all", label: "全部仓库" },
        ...state.repositories.items.map((repo) => ({ value: repo.id, label: skillRepositoryLabel(repo) })),
      ]} />}
    <Select value={state.filter} onChange={(value) => state.setFilter(value as SkillSourceFilter)}
      ariaLabel="Skill 安装状态" options={[
        { value: "all", label: "全部状态" }, { value: "installed", label: "已安装" }, { value: "pending", label: "待安装" },
      ]} />
  </div>;
}

function SourceResults({ state, busy }: { state: SkillSourceState; busy: boolean }) {
  const noRepositories = state.source === "catalog" && state.repositories.ready && state.repositories.items.length === 0;
  if (noRepositories) return <div className="asb-skill-source-empty" role="status">
    <SearchIcon /><p>尚未添加仓库</p>
    <Button variant="secondary" disabled={busy || state.imports.busy} onClick={() => state.setManagerOpen(true)}>
      <Plus size={16} />添加仓库
    </Button>
  </div>;
  if (state.candidates === null) return state.loading ? (
    <section className="asb-skill-source-results" aria-label="Skill 来源候选" aria-busy="true">
      <ExtensionLoading />
    </section>
  ) : (
    <div className="asb-skill-source-empty" role="status">
      <SearchIcon />
      <p>{state.source === "catalog" ? "尚未刷新仓库" : "尚未扫描来源"}</p>
    </div>
  );
  return (
    <section className="asb-skill-source-results" aria-label="Skill 来源候选" aria-busy={state.loading}>
      <SourceFilters state={state} />
      <p className="asb-skill-source-count asb-num" role="status">
        {state.visible.length} / {state.candidates.length} 个 Skills · 已安装 {state.installedCount}
        {state.loading && " · 正在重新扫描…"}
      </p>
      {state.visible.length === 0 ? (
        <div className="asb-skill-source-empty">
          <p>{state.candidates.length === 0 ? (state.source === "catalog" && state.catalog.result?.failures.length ?
            "未获取到候选结果" : "来源中没有 SKILL.md") : "没有符合筛选条件的 Skill"}</p>
          {state.candidates.length > 0 && <Button variant="secondary" onClick={() => {
            state.setQuery(""); state.setFilter("all"); state.setRepository("all");
          }}>清除筛选</Button>}
        </div>
      ) : <ul className="asb-skill-source-grid">
        {state.visible.map((row) => <SkillSourceCandidate key={row.key} row={row} installed={row.installed}
          imports={state.imports} source={state.source} busy={busy || state.loading} />)}
      </ul>}
    </section>
  );
}

function SourceErrors({ state, busy }: { state: SkillSourceState; busy: boolean }) {
  const disabled = busy || state.loading || state.imports.busy;
  return <>
    {state.error && <p className="asb-warn-text" role="alert">{state.error}</p>}
    {state.imports.error && <p className="asb-warn-text" role="alert">{state.imports.error}</p>}
    {state.source === "directory" && state.directory.resolveError &&
      <p className="asb-warn-text" role="alert">{state.directory.resolveError}</p>}
    {!state.managerOpen && state.repositories.error && <div className="asb-skill-source-error" role="alert">
      <span>{state.repositories.error}</span>
      <Button variant="secondary" disabled={disabled || state.repositories.loading}
        onClick={() => void state.repositories.reload()}><RefreshCw size={16} />重试加载仓库</Button>
    </div>}
    {state.source === "catalog" && Boolean(state.catalog.result?.failures.length) &&
      <div className="asb-skill-source-error" role="alert">
        <ul className="asb-skill-source-diagnostics">
          {state.catalog.result!.failures.map((failure) => <li key={failure.repositoryId}>
            {skillRepositoryName(failure.repo) ?? "GitHub 仓库"}：{sourceErrorMessage(failure.message, "仓库刷新失败")}
            {state.catalog.count(failure.repositoryId) !== null && "；仍显示上次结果"}
          </li>)}
        </ul>
        <Button variant="secondary" disabled={disabled || state.repositories.loading} onClick={() => void state.search()}>
          <RefreshCw size={16} />重试刷新
        </Button>
      </div>}
  </>;
}

/** 发现 Skills is a full-page workbench on the shared editor frame; the scan
 * action lives in the fixed bottom bar and submits the source form via its id. */
export function SkillSourceBrowser(props: Props) {
  const state = useSkillSource(props);
  const scanning = state.source === "directory" ? state.directory.searching : state.loading;
  const scanDisabled = props.busy || state.loading || state.imports.busy || (state.source === "catalog" &&
    (!state.repositories.ready || state.repositories.loading || !state.repositories.items.some((repo) => repo.enabled)));
  const scanLabel = scanning ? (state.source === "directory" ? "正在搜索…" : "正在扫描…")
    : state.source === "catalog" ? "刷新仓库"
      : state.source === "directory" ? "搜索" : state.source === "zip" ? "扫描 ZIP" : "扫描来源";
  return (
    <EditorFrame
      title="发现 Skills"
      backLabel="返回扩展库"
      busy={props.busy}
      onBack={props.onBack}
      footer={
        <Button type="submit" form="skill-source-scan" variant="primary" disabled={scanDisabled}>
          {scanning ? <LoaderCircle size={16} className="asb-skill-source-spinner" />
            : state.source === "catalog" ? <RefreshCw size={16} /> : <SearchIcon />}
          {scanLabel}
        </Button>
      }
    >
      {props.notice}
      <div className="asb-skill-source-browser">
        <SkillSourceForm state={state} busy={props.busy} />
        {state.managerOpen ? <SkillRepositoryManager state={state} busy={props.busy} /> : <>
          <SourceErrors state={state} busy={props.busy} />
          {state.source === "directory" ? <SkillDirectoryResults state={state} busy={props.busy} items={props.items} /> :
            <SourceResults state={state} busy={props.busy} />}
        </>}
      </div>
    </EditorFrame>
  );
}
