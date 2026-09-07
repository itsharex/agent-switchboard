import { useEffect, useMemo, useState } from "react";
import type {
  ExtensionListItem,
  SkillDependency,
  SkillEditorView,
  SkillVersion,
} from "../../api/client";
import { Button } from "../Button";
import { Input } from "../Input";
import { Select, type SelectOption } from "../Select";
import { Textarea } from "../Textarea";

export interface SkillWorkbenchHandlers {
  onLoadEditor: (definitionId: string) => Promise<SkillEditorView | null>;
  onSaveFiles: (
    definitionId: string,
    update: {
      expectedRevision: number;
      expectedDigest: string;
      files: Array<{ relativePath: string; text: string }>;
    },
  ) => Promise<unknown>;
  onLoadVersions: (definitionId: string) => Promise<SkillVersion[] | null>;
  onRestoreVersion: (definitionId: string, digest: string) => void;
  onSaveDependencies: (
    definitionId: string,
    update: { expectedRevision: number; dependencies: SkillDependency[] },
  ) => Promise<unknown>;
  onFork: (definitionId: string) => void;
  onClose: () => void;
}

interface Props extends SkillWorkbenchHandlers {
  item: Extract<ExtensionListItem, { kind: "skill" }>;
  mcpOptions: Array<{ id: string; name: string }>;
  busy: boolean;
}

function formatBytes(size: number): string {
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

/** The static editor for one skill's local content: text files, immutable
 * version history, and dependency links. Binary files are carried over
 * verbatim on save; nothing here executes skill content. */
export function SkillWorkbench({ item, mcpOptions, busy, onLoadEditor, ...handlers }: Props) {
  const [editor, setEditor] = useState<SkillEditorView | null>(null);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [newPath, setNewPath] = useState("");
  const [versions, setVersions] = useState<SkillVersion[] | null>(null);
  const [dependencies, setDependencies] = useState<SkillDependency[]>(
    item.dependencies.map((dependency) => ({ ...dependency })),
  );
  const [selectedFile, setSelectedFile] = useState<string | null>(null);

  useEffect(() => {
    setDependencies(item.dependencies.map((dependency) => ({ ...dependency })));
  }, [item.dependencies]);

  useEffect(() => {
    let current = true;
    void onLoadEditor(item.id).then((view) => {
      if (!current || view === null) return;
      setEditor(view);
      const initial: Record<string, string> = {};
      for (const file of view.files) {
        if (file.text !== null) initial[file.relativePath] = file.text;
      }
      setDrafts(initial);
      setSelectedFile(view.files.some((file) => file.relativePath === "SKILL.md")
        ? "SKILL.md"
        : (view.files[0]?.relativePath ?? null));
    });
    return () => {
      current = false;
    };
    // Reload only when the definition revision advances (a saved edit), not
    // on every parent render; unsaved drafts survive unrelated re-renders.
  }, [item.id, item.revision, onLoadEditor]);

  const reloadVersions = () => {
    void handlers.onLoadVersions(item.id).then((list) => {
      if (list !== null) setVersions(list);
    });
  };

  const editable = editor?.editable ?? false;
  const allPaths = useMemo(() => {
    const stored = (editor?.files ?? []).map((file) => file.relativePath);
    const added = Object.keys(drafts).filter((path) => !stored.includes(path));
    return [...stored, ...added].sort();
  }, [editor, drafts]);
  const activeFile =
    selectedFile !== null && allPaths.includes(selectedFile) ? selectedFile : allPaths[0] ?? null;

  const save = () => {
    if (editor === null) return;
    const files = allPaths
      .filter((path) => drafts[path] !== undefined)
      .map((path) => ({ relativePath: path, text: drafts[path] }));
    void handlers.onSaveFiles(item.id, {
      expectedRevision: editor.revision,
      expectedDigest: editor.contentDigest,
      files,
    });
  };

  const addFile = () => {
    const path = newPath.trim();
    if (path === "") return;
    setDrafts((previous) => ({ ...previous, [path]: "" }));
    setSelectedFile(path);
    setNewPath("");
  };

  const mcpSelectOptions: SelectOption[] = [
    { value: "", label: "（未关联）" },
    ...mcpOptions.map((option) => ({ value: option.id, label: option.name })),
  ];

  return (
    <section className="asb-ext-detail" aria-label={`Skill 编辑 ${item.name}`}>
      <div className="asb-ext-detail-main">
        <header className="asb-ext-detail-head">
          <h3>编辑 Skill：{item.name}</h3>
          <Button variant="secondary" onClick={handlers.onClose}>
            关闭编辑器
          </Button>
        </header>

        {editor === null ? (
          <p className="asb-empty">正在加载内容版本…</p>
        ) : !editable ? (
          <div className="asb-ext-section">
            <p className="asb-scope-note">
              该 Skill 带来源记录或宿主限定，不能直接编辑；创建本地副本后即可修改，
              副本不再跟随来源更新。
            </p>
            <Button variant="primary" disabled={busy} onClick={() => handlers.onFork(item.id)}>
              创建本地副本
            </Button>
          </div>
        ) : (
          <>
            <p className="asb-scope-note">
              修订 r{editor.revision} · 内容摘要{" "}
              <span className="asb-code">{editor.contentDigest.slice(0, 12)}</span> ·
              每次保存生成新的不可变版本，旧版本可随时恢复。
            </p>
            <div className="asb-ext-section">
              <h4>文件</h4>
              <ul className="asb-ext-binding-list">
                {allPaths.map((path) => {
                  const stored = editor.files.find((file) => file.relativePath === path);
                  const isBinary = stored !== undefined && stored.text === null;
                  return (
                    <li key={path} className="asb-ext-binding">
                      <Button
                        variant="secondary"
                        disabled={busy || isBinary}
                        onClick={() => setSelectedFile(path)}
                      >
                        {activeFile === path ? "▸ " : ""}
                        <span className="asb-code">{path}</span>
                      </Button>
                      {isBinary && (
                        <span className="asb-pill-status">
                          二进制文件（{formatBytes(stored.size)}），保存时按原样保留
                        </span>
                      )}
                    </li>
                  );
                })}
              </ul>
              <div className="asb-ext-actions">
                <Input
                  value={newPath}
                  placeholder="新文件路径，如 references/style.md"
                  aria-label="新文件路径"
                  disabled={busy}
                  onChange={(event) => setNewPath(event.target.value)}
                />
                <Button variant="secondary" disabled={busy || newPath.trim() === ""} onClick={addFile}>
                  添加文件
                </Button>
                <Button variant="primary" disabled={busy} onClick={save}>
                  保存为新版本
                </Button>
              </div>
              {activeFile !== null && drafts[activeFile] !== undefined && (
                <Textarea
                  value={drafts[activeFile]}
                  aria-label={`编辑 ${activeFile}`}
                  rows={18}
                  disabled={busy}
                  onChange={(event) =>
                    setDrafts((previous) => ({ ...previous, [activeFile]: event.target.value }))
                  }
                />
              )}
            </div>
          </>
        )}

        <div className="asb-ext-section">
          <h4>版本历史</h4>
          <Button variant="secondary" disabled={busy} onClick={reloadVersions}>
            加载版本列表
          </Button>
          {versions !== null && (
            <ul className="asb-ext-binding-list">
              {versions.map((version) => (
                <li key={version.digest} className="asb-ext-binding">
                  <span className="asb-code">{version.digest.slice(0, 12)}</span>
                  <span>
                    {version.fileCount} 个文件 · {formatBytes(version.totalBytes)}
                  </span>
                  {version.isCurrent ? (
                    <span className="asb-pill-status">当前版本</span>
                  ) : (
                    <Button
                      variant="secondary"
                      disabled={busy}
                      onClick={() => handlers.onRestoreVersion(item.id, version.digest)}
                    >
                      回滚到此版本
                    </Button>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="asb-ext-section">
          <h4>依赖关联</h4>
          <p className="asb-scope-note">
            依赖指向库内 MCP 定义；部署该 Skill 时可在同一预览中选择是否一并部署。
          </p>
          <ul className="asb-ext-binding-list">
            {dependencies.map((dependency, index) => (
              <li key={index} className="asb-ext-binding">
                <Input
                  value={dependency.name}
                  aria-label="依赖名称"
                  disabled={busy}
                  onChange={(event) =>
                    setDependencies((previous) =>
                      previous.map((row, position) =>
                        position === index ? { ...row, name: event.target.value } : row,
                      ),
                    )
                  }
                />
                <Select
                  value={dependency.resourceId ?? ""}
                  options={mcpSelectOptions}
                  ariaLabel={`依赖第 ${index + 1} 行关联的 MCP`}
                  disabled={busy}
                  onChange={(value) =>
                    setDependencies((previous) =>
                      previous.map((row, position) =>
                        position === index
                          ? { ...row, resourceId: value === "" ? null : value }
                          : row,
                      ),
                    )
                  }
                />
                <Button
                  variant="secondary"
                  disabled={busy}
                  onClick={() =>
                    setDependencies((previous) => previous.filter((_, position) => position !== index))
                  }
                >
                  移除依赖
                </Button>
              </li>
            ))}
          </ul>
          <div className="asb-ext-actions">
            <Button
              variant="secondary"
              disabled={busy}
              onClick={() => setDependencies((previous) => [...previous, { name: "", resourceId: null }])}
            >
              添加依赖
            </Button>
            <Button
              variant="primary"
              disabled={busy || editor === null}
              onClick={() => {
                if (editor === null) return;
                void handlers.onSaveDependencies(item.id, {
                  expectedRevision: editor.revision,
                  dependencies: dependencies.filter((dependency) => dependency.name.trim() !== ""),
                });
              }}
            >
              保存依赖
            </Button>
          </div>
        </div>
      </div>
    </section>
  );
}
