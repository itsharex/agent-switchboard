import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SkillWorkbench } from "./SkillWorkbench";
import type {
  ExtensionListItem,
  SkillEditorView,
  SkillVersion,
} from "../../api/client";

const skillItem: Extract<ExtensionListItem, { kind: "skill" }> = {
  schemaVersion: 2,
  id: "ext-skill-1",
  name: "api-spec",
  revision: 2,
  createdAt: "2026-09-01T08:00:00Z",
  updatedAt: "2026-09-02T08:00:00Z",
  kind: "skill",
  contentDigest: "b".repeat(64),
  manifest: { name: "api-spec", description: "撰写并检查接口规范", unparsedKeys: [] },
  source: null,
  hostScoped: null,
  compatibility: [],
  dependencies: [],
  bindings: [],
  dependencyStates: [],
  lastCheck: null,
};

const editorView: SkillEditorView = {
  id: "ext-skill-1",
  revision: 2,
  contentDigest: "b".repeat(64),
  manifest: { name: "api-spec", description: "撰写并检查接口规范", unparsedKeys: [] },
  editable: true,
  files: [
    { relativePath: "SKILL.md", text: "---\nname: api-spec\n---\n\n# 规范\n", size: 40 },
    { relativePath: "assets/logo.png", text: null, size: 512 },
  ],
};

const versions: SkillVersion[] = [
  { digest: "a".repeat(64), fileCount: 1, totalBytes: 148, isCurrent: false },
  { digest: "b".repeat(64), fileCount: 2, totalBytes: 660, isCurrent: true },
];

function renderWorkbench(overrides?: {
  item?: Partial<typeof skillItem>;
  editor?: SkillEditorView;
}) {
  const onLoadEditor = vi.fn().mockResolvedValue(overrides?.editor ?? editorView);
  const onSaveFiles = vi.fn().mockResolvedValue(null);
  const onLoadVersions = vi.fn().mockResolvedValue(versions);
  const onRestoreVersion = vi.fn();
  const onSaveDependencies = vi.fn().mockResolvedValue(null);
  const onFork = vi.fn();
  const onClose = vi.fn();
  render(
    <SkillWorkbench
      item={{ ...skillItem, ...overrides?.item }}
      mcpOptions={[{ id: "ext-mcp-1", name: "docs" }]}
      busy={false}
      onLoadEditor={onLoadEditor}
      onSaveFiles={onSaveFiles}
      onLoadVersions={onLoadVersions}
      onRestoreVersion={onRestoreVersion}
      onSaveDependencies={onSaveDependencies}
      onFork={onFork}
      onClose={onClose}
    />,
  );
  return { onLoadEditor, onSaveFiles, onLoadVersions, onRestoreVersion, onSaveDependencies, onFork, onClose };
}

beforeEach(() => {
  vi.clearAllMocks();
});

describe("SkillWorkbench", () => {
  it("loads the current version and keeps binary files opaque", async () => {
    renderWorkbench();

    expect(await screen.findByLabelText("编辑 SKILL.md")).toHaveValue(
      "---\nname: api-spec\n---\n\n# 规范\n",
    );
    expect(
      screen.getByText(/二进制文件（512 B），保存时按原样保留/),
    ).toBeInTheDocument();
    // Binary files never open an editor surface.
    expect(screen.queryByLabelText("编辑 assets/logo.png")).not.toBeInTheDocument();
  });

  it("publishes an edit with the revision and digest it was prepared against", async () => {
    const user = userEvent.setup();
    const handlers = renderWorkbench();

    const editor = await screen.findByLabelText("编辑 SKILL.md");
    await user.clear(editor);
    await user.type(editor, "---\nname: api-spec\ndescription: 更新后的描述\n---\n\n正文\n");

    await user.click(screen.getByRole("button", { name: "保存为新版本" }));

    await waitFor(() => {
      expect(handlers.onSaveFiles).toHaveBeenCalledWith("ext-skill-1", {
        expectedRevision: 2,
        expectedDigest: "b".repeat(64),
        files: [
          {
            relativePath: "SKILL.md",
            text: "---\nname: api-spec\ndescription: 更新后的描述\n---\n\n正文\n",
          },
          // The binary file is never part of the text payload.
        ],
      });
    });
  });

  it("offers rollback through the stored version history", async () => {
    const user = userEvent.setup();
    const handlers = renderWorkbench();

    await user.click(await screen.findByRole("button", { name: "加载版本列表" }));

    const older = await screen.findByText("a".repeat(12));
    expect(older).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "回滚到此版本" }));
    expect(handlers.onRestoreVersion).toHaveBeenCalledWith(
      "ext-skill-1",
      "a".repeat(64),
    );
  });

  it("edits dependency links against library MCP definitions only", async () => {
    const user = userEvent.setup();
    const handlers = renderWorkbench();

    await user.click(await screen.findByRole("button", { name: "添加依赖" }));
    await user.type(await screen.findByLabelText("依赖名称"), "docs");
    await user.click(screen.getByRole("combobox", { name: "依赖第 1 行关联的 MCP" }));
    await user.click(screen.getByRole("option", { name: "docs" }));

    await user.click(screen.getByRole("button", { name: "保存依赖" }));
    await waitFor(() => {
      expect(handlers.onSaveDependencies).toHaveBeenCalledWith("ext-skill-1", {
        expectedRevision: 2,
        dependencies: [{ name: "docs", resourceId: "ext-mcp-1" }],
      });
    });
  });

  it("directs sourced skills to a local copy instead of editing", async () => {
    const handlers = renderWorkbench({
      item: { source: { resolvedCommit: "c".repeat(40) }, hostScoped: "claude" },
      editor: { ...editorView, editable: false },
    });

    const forkButton = await screen.findByRole("button", { name: "创建本地副本" });
    expect(forkButton).toBeInTheDocument();
    // No editor surface is offered for the sourced content.
    expect(screen.queryByLabelText("编辑 SKILL.md")).not.toBeInTheDocument();

    forkButton.click();
    expect(handlers.onFork).toHaveBeenCalledWith("ext-skill-1");
  });
});
