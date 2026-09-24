import { useEffect, useMemo, useState } from "react";
import type {
  ExtensionListItem,
  SkillDependency,
  SkillEditorView,
  SkillVersion,
} from "../../api/client";
import { useI18n } from "../../i18n";
import { Button } from "../Button";
import { Input } from "../Input";
import { ExtensionLoading } from "./ExtensionLoading";
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
  const { t } = useI18n();
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
    { value: "", label: t("extensions.workbench.unlinked") },
    ...mcpOptions.map((option) => ({ value: option.id, label: option.name })),
  ];

  return (
    <section className="asb-skill-workbench" aria-label={t("extensions.workbench.aria", { name: item.name })}>
      <div className="asb-skill-workbench-main">
        <header className="asb-skill-workbench-head">
          <h3 className="asb-section-title">{t("extensions.workbench.title", { name: item.name })}</h3>
          <Button variant="secondary" onClick={handlers.onClose}>
            {t("extensions.workbench.close")}
          </Button>
        </header>

        {editor === null ? (
          <ExtensionLoading />
        ) : !editable ? (
          <div className="asb-ext-section">
            <p className="asb-scope-note">
              {t("extensions.workbench.forkNote")}
            </p>
            <Button variant="primary" disabled={busy} onClick={() => handlers.onFork(item.id)}>
              {t("extensions.workbench.fork")}
            </Button>
          </div>
        ) : (
          <>
            <p className="asb-scope-note">
              {t("extensions.workbench.revisionLead", { revision: editor.revision })}
              <span className="asb-code">{editor.contentDigest.slice(0, 12)}</span>
              {t("extensions.workbench.revisionTail")}
            </p>
            <div className="asb-ext-section">
              <h4 className="asb-section-title">{t("extensions.workbench.files")}</h4>
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
                          {t("extensions.workbench.binary", { size: formatBytes(stored.size) })}
                        </span>
                      )}
                    </li>
                  );
                })}
              </ul>
              <div className="asb-ext-actions">
                <Input
                  value={newPath}
                  placeholder={t("extensions.workbench.newPathPlaceholder")}
                  aria-label={t("extensions.workbench.newPathAria")}
                  disabled={busy}
                  onChange={(event) => setNewPath(event.target.value)}
                />
                <Button variant="secondary" disabled={busy || newPath.trim() === ""} onClick={addFile}>
                  {t("extensions.workbench.addFile")}
                </Button>
              </div>
              {activeFile !== null && drafts[activeFile] !== undefined && (
                <Textarea
                  value={drafts[activeFile]}
                  aria-label={t("extensions.workbench.editFileAria", { file: activeFile })}
                  rows={18}
                  disabled={busy}
                  onChange={(event) =>
                    setDrafts((previous) => ({ ...previous, [activeFile]: event.target.value }))
                  }
                />
              )}
              {/* The save follows the content it commits: directly below the editor. */}
              <div className="asb-ext-actions">
                <Button variant="primary" disabled={busy} onClick={save}>
                  {t("extensions.workbench.saveVersion")}
                </Button>
              </div>
            </div>
          </>
        )}

        <div className="asb-ext-section">
          <h4 className="asb-section-title">{t("extensions.workbench.versions")}</h4>
          <Button variant="secondary" disabled={busy} onClick={reloadVersions}>
            {t("extensions.workbench.loadVersions")}
          </Button>
          {versions !== null && (
            <ul className="asb-ext-binding-list">
              {versions.map((version) => (
                <li key={version.digest} className="asb-ext-binding">
                  <span className="asb-code">{version.digest.slice(0, 12)}</span>
                  <span>
                    {t("extensions.workbench.versionMeta", {
                      count: version.fileCount,
                      size: formatBytes(version.totalBytes),
                    })}
                  </span>
                  {version.isCurrent ? (
                    <span className="asb-pill-status">{t("extensions.workbench.currentVersion")}</span>
                  ) : (
                    <Button
                      variant="secondary"
                      disabled={busy}
                      onClick={() => handlers.onRestoreVersion(item.id, version.digest)}
                    >
                      {t("extensions.workbench.restoreVersion")}
                    </Button>
                  )}
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="asb-ext-section">
          <h4 className="asb-section-title">{t("extensions.workbench.dependencies")}</h4>
          <p className="asb-scope-note">
            {t("extensions.workbench.depsNote")}
          </p>
          <ul className="asb-ext-binding-list">
            {dependencies.map((dependency, index) => (
              <li key={index} className="asb-ext-binding">
                <Input
                  value={dependency.name}
                  aria-label={t("extensions.workbench.depNameAria")}
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
                  ariaLabel={t("extensions.workbench.depSelectAria", { row: index + 1 })}
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
                  {t("extensions.workbench.removeDep")}
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
              {t("extensions.workbench.addDep")}
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
              {t("extensions.workbench.saveDeps")}
            </Button>
          </div>
        </div>
      </div>
    </section>
  );
}
