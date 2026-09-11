import type { ExtensionListItem, SkillCandidateDto } from "../../api/client";
import { Button } from "../Button";
import { Input } from "../Input";
import { RadioOption } from "../RadioOption";
import { Select } from "../Select";
import { CheckIcon, FolderOpenIcon, PlusIcon, SearchIcon } from "../icons";
import { useSkillSource, type SkillSourceActions, type SkillSourceState } from "./useSkillSource";

interface Props extends SkillSourceActions {
  busy: boolean;
  items: ExtensionListItem[];
}

function SourceFields({ state, busy }: { state: SkillSourceState; busy: boolean }) {
  return (
    <form
      className="asb-ext-source-form"
      aria-label="Skill 来源"
      onSubmit={(event) => {
        event.preventDefault();
        void state.search();
      }}
    >
      <div className="asb-segments" role="radiogroup" aria-label="Skill 来源类型">
        {(["github", "local"] as const).map((source) => (
          <RadioOption
            key={source}
            name="skill-source-type"
            checked={state.source === source}
            disabled={busy}
            label={source === "github" ? "GitHub 仓库" : "本地目录"}
            onChange={() => state.changeSource(source)}
          />
        ))}
      </div>
      <div className="asb-ext-source-fields">
        {state.source === "local" ? (
          <label className="asb-field asb-ext-source-root">
            <span>本地来源目录</span>
            <Input
              required
              placeholder="D:\skills"
              value={state.fields.root}
              disabled={busy}
              onChange={(event) => state.changeField("root", event.target.value)}
            />
          </label>
        ) : (
          <>
            <label className="asb-field">
              <span>GitHub 仓库</span>
              <Input
                required
                placeholder="owner/repository"
                value={state.fields.repository}
                disabled={busy}
                onChange={(event) => state.changeField("repository", event.target.value)}
              />
            </label>
            <label className="asb-field">
              <span>Skill 子目录</span>
              <Input
                required
                placeholder="skills/api-spec"
                value={state.fields.subpath}
                disabled={busy}
                onChange={(event) => state.changeField("subpath", event.target.value)}
              />
            </label>
            <label className="asb-field">
              <span>分支或提交（可选）</span>
              <Input
                placeholder="默认分支"
                value={state.fields.ref}
                disabled={busy}
                onChange={(event) => state.changeField("ref", event.target.value)}
              />
            </label>
          </>
        )}
        <div className="asb-ext-source-actions">
          {state.source === "local" && (
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => void state.pickDirectory()}
            >
              <FolderOpenIcon />
              浏览…
            </Button>
          )}
          <Button type="submit" variant="primary" disabled={busy}>
            <SearchIcon />
            {state.loading ? "正在读取…" : state.source === "github" ? "解析来源" : "扫描来源"}
          </Button>
        </div>
      </div>
    </form>
  );
}

function SourceCandidate({
  candidate,
  installed,
  busy,
  onImport,
}: {
  candidate: SkillCandidateDto;
  installed: boolean;
  busy: boolean;
  onImport: () => void;
}) {
  return (
    <li className="asb-ext-source-card">
      <h4 className="asb-group-title">{candidate.name}</h4>
      {candidate.description && <p>{candidate.description}</p>}
      {candidate.diagnostics.length > 0 && (
        <ul className="asb-ext-source-diagnostics">
          {candidate.diagnostics.map((message) => (
            <li key={message}>{message}</li>
          ))}
        </ul>
      )}
      <div className="asb-ext-source-card-footer">
        <span>{candidate.fileCount} 个文件</span>
        <Button
          variant="secondary"
          disabled={busy || installed}
          onClick={onImport}
          aria-label={`${installed ? "已在扩展库" : "加入扩展库"} ${candidate.name}`}
        >
          {installed ? <CheckIcon /> : <PlusIcon />}
          {installed ? "已在扩展库" : "加入扩展库"}
        </Button>
      </div>
    </li>
  );
}

function SourceResults({ state, props }: { state: SkillSourceState; props: Props }) {
  if (state.candidates === null)
    return (
      <div className="asb-empty-state">
        <span className="asb-empty-state-icon" aria-hidden="true">
          <SearchIcon />
        </span>
        <h3 className="asb-section-title">从来源发现 Skills</h3>
        <p className="asb-empty-state-detail">填写仓库或本地目录，读取后选择要加入扩展库的 Skill。</p>
      </div>
    );
  const host = state.host === "all" ? null : state.host;
  const installed = new Set(
    props.items.flatMap((item) =>
      item.kind === "skill" && item.hostScoped === host ? [item.contentDigest] : [],
    ),
  );
  return (
    <section className="asb-ext-source-results" aria-label="Skill 来源候选">
      <div className="asb-ext-toolbar">
        <div className="asb-ext-toolbar-view">
          <Input
            type="search"
            placeholder="筛选发现的 Skills"
            aria-label="筛选发现的 Skills"
            value={state.query}
            onChange={(event) => state.setQuery(event.target.value)}
          />
        </div>
        <div className="asb-ext-toolbar-actions">
          <span className="asb-scope-note">{state.visible.length} 项</span>
          <Select
            value={state.host}
            onChange={(value) => state.setHost(value as "all" | "codex" | "claude")}
            disabled={props.busy}
            ariaLabel="Skill 兼容范围"
            options={[
              { value: "all", label: "Codex 与 Claude" },
              { value: "codex", label: "仅 Codex" },
              { value: "claude", label: "仅 Claude" },
            ]}
          />
        </div>
      </div>
      {state.visible.length === 0 ? (
        <p className="asb-empty">
          {state.candidates.length === 0 ? "来源中未发现可导入的 Skill" : "没有符合搜索条件的 Skill"}
        </p>
      ) : (
        <ul className="asb-ext-source-grid">
          {state.visible.map((candidate) => (
            <SourceCandidate
              key={candidate.digest}
              candidate={candidate}
              installed={installed.has(candidate.digest)}
              busy={props.busy}
              onImport={() => void props.onImport(candidate.digest, candidate.name, host)}
            />
          ))}
        </ul>
      )}
    </section>
  );
}

export function SkillSourceBrowser(props: Props) {
  const state = useSkillSource(props);
  return (
    <section className="asb-ext-source-browser" aria-label="发现 Skills">
      <SourceFields state={state} busy={props.busy || state.loading} />
      {state.error && (
        <p className="asb-warn-text" role="alert">
          {state.error}
        </p>
      )}
      <SourceResults state={state} props={props} />
    </section>
  );
}
