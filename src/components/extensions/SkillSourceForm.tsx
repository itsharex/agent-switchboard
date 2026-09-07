import { useState } from "react";
import type {
  AppKind,
  ExtensionMutation,
  SkillCandidateDto,
} from "../../api/client";
import { Button } from "../Button";
import { Input } from "../Input";
import { Select } from "../Select";
import { Table, type TableColumn } from "../Table";

type SourceKind = "local" | "github";

interface Props {
  busy: boolean;
  onScanLocal: (root: string) => Promise<SkillCandidateDto[] | null>;
  onResolveGithub: (
    repository: string,
    subpath: string,
    refName: string | null,
  ) => Promise<SkillCandidateDto[] | null>;
  onImport: (
    digest: string,
    name: string,
    hostScoped: AppKind | null,
  ) => Promise<ExtensionMutation | null>;
}

const SOURCE_OPTIONS = [
  { value: "local", label: "本地目录" },
  { value: "github", label: "GitHub 仓库" },
] as const;

const HOST_SCOPE_OPTIONS = [
  { value: "", label: "通用 Skill" },
  { value: "codex", label: "仅 Codex" },
  { value: "claude", label: "仅 Claude" },
] as const;

/** Explicit source lookup for Skills. Reading a source only creates
 * in-memory candidates; a separate button is required to copy one into the
 * immutable local extension library. */
export function SkillSourceForm({
  busy,
  onScanLocal,
  onResolveGithub,
  onImport,
}: Props) {
  const [sourceKind, setSourceKind] = useState<SourceKind>("local");
  const [root, setRoot] = useState("");
  const [repository, setRepository] = useState("");
  const [subpath, setSubpath] = useState("");
  const [refName, setRefName] = useState("");
  const [hostScoped, setHostScoped] = useState<"" | AppKind>("");
  const [candidates, setCandidates] = useState<SkillCandidateDto[] | null>(null);
  const [error, setError] = useState<string | null>(null);

  const search = async () => {
    setError(null);
    if (sourceKind === "local") {
      if (!root.trim()) {
        setError("请选择包含一个或多个 Skill 目录的本地路径");
        return;
      }
      const result = await onScanLocal(root.trim());
      if (result) setCandidates(result);
      return;
    }
    if (!repository.trim() || !subpath.trim()) {
      setError("GitHub 仓库和 Skill 子目录均不能为空");
      return;
    }
    const result = await onResolveGithub(
      repository.trim(),
      subpath.trim(),
      refName.trim() || null,
    );
    if (result) setCandidates(result);
  };

  const importCandidate = async (candidate: SkillCandidateDto) => {
    const result = await onImport(
      candidate.digest,
      candidate.name,
      hostScoped || null,
    );
    if (result) setCandidates(null);
  };

  const candidateColumns: Array<TableColumn<SkillCandidateDto>> = [
    {
      key: "name",
      header: "名称",
      render: (candidate) => (
        <>
          <strong>{candidate.name}</strong>
          {candidate.description && (
            <div className="asb-scope-note">{candidate.description}</div>
          )}
        </>
      ),
    },
    { key: "files", header: "文件数", render: (candidate) => `${candidate.fileCount} 个` },
    {
      key: "digest",
      header: "内容摘要",
      render: (candidate) => <span className="asb-code">{candidate.digest.slice(0, 12)}</span>,
    },
    {
      key: "status",
      header: "状态",
      render: (candidate) =>
        candidate.diagnostics.length > 0 ? (
          <>
            {candidate.diagnostics.map((diagnostic) => (
              <div key={diagnostic} className="asb-warn-text">
                {diagnostic}
              </div>
            ))}
          </>
        ) : (
          "—"
        ),
    },
    {
      key: "actions",
      header: "操作",
      render: (candidate) => (
        <Button variant="secondary" disabled={busy} onClick={() => void importCandidate(candidate)}>
          加入扩展库
        </Button>
      ),
    },
  ];

  return (
    <section className="asb-form" aria-label="添加 Skill 来源">
      <div className="asb-field">
        <span>来源类型</span>
        <Select
          value={sourceKind}
          options={SOURCE_OPTIONS}
          onChange={(value) => {
            setSourceKind(value as SourceKind);
            setCandidates(null);
            setError(null);
          }}
          ariaLabel="Skill 来源类型"
          disabled={busy}
        />
      </div>
      {sourceKind === "local" ? (
        <label className="asb-field">
          <span>本地来源目录</span>
          <Input
            required
            placeholder="D:\skills"
            value={root}
            disabled={busy}
            onChange={(event) => setRoot(event.target.value)}
          />
        </label>
      ) : (
        <>
          <label className="asb-field">
            <span>GitHub 仓库</span>
            <Input
              required
              placeholder="owner/repository"
              value={repository}
              disabled={busy}
              onChange={(event) => setRepository(event.target.value)}
            />
          </label>
          <label className="asb-field">
            <span>Skill 子目录</span>
            <Input
              required
              placeholder="skills/api-spec"
              value={subpath}
              disabled={busy}
              onChange={(event) => setSubpath(event.target.value)}
            />
          </label>
          <label className="asb-field">
            <span>固定 ref（可选）</span>
            <Input
              placeholder="main 或提交 SHA"
              value={refName}
              disabled={busy}
              onChange={(event) => setRefName(event.target.value)}
            />
          </label>
        </>
      )}
      <div className="asb-field">
        <span>兼容范围</span>
        <Select
          value={hostScoped}
          options={HOST_SCOPE_OPTIONS}
          onChange={(value) => setHostScoped(value as "" | AppKind)}
          ariaLabel="Skill 兼容范围"
          disabled={busy}
        />
      </div>
      <div className="asb-form-actions">
        <Button variant="secondary" disabled={busy} onClick={() => void search()}>
          {sourceKind === "github" ? "解析来源" : "扫描来源"}
        </Button>
      </div>
      {error && (
        <p className="asb-warn-text" role="alert">
          {error}
        </p>
      )}
      {candidates && (
        <div className="asb-ext-section" aria-label="Skill 来源候选">
          {candidates.length === 0 ? (
            <p className="asb-empty">来源中未发现可导入的 Skill</p>
          ) : (
            <Table
              columns={candidateColumns}
              rows={candidates}
              rowKey={(candidate) => candidate.digest}
              ariaLabel="Skill 来源候选"
            />
          )}
        </div>
      )}
    </section>
  );
}
