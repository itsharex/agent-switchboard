import { expect, it, vi } from "vitest";
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SkillSourceBrowser } from "./SkillSourceBrowser";
import type { ExtensionListItem, SkillCandidateDto } from "../../api/client";

const candidate: SkillCandidateDto = {
  digest: "a".repeat(64),
  name: "release-notes",
  description: "整理版本说明",
  fileCount: 3,
  diagnostics: [],
};

function props() {
  return {
    busy: false,
    items: [] as ExtensionListItem[],
    onScanLocal: vi.fn().mockResolvedValue([candidate]),
    onResolveGithub: vi.fn().mockResolvedValue([candidate]),
    onImport: vi.fn().mockResolvedValue({ id: "added", name: candidate.name, revision: 1 }),
  };
}

it("looks up a GitHub source only on request and keeps the candidates after adding one", async () => {
  const callbacks = props();
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await user.type(screen.getByRole("textbox", { name: "GitHub 仓库" }), "example/skills");
  await user.type(screen.getByRole("textbox", { name: "Skill 子目录" }), "skills/release-notes");
  expect(callbacks.onResolveGithub).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "解析来源" }));
  expect(callbacks.onResolveGithub).toHaveBeenCalledWith("example/skills", "skills/release-notes", null);
  const results = await screen.findByRole("region", { name: "Skill 来源候选" });
  const host = within(results).getByRole("combobox", { name: "Skill 兼容范围" });
  expect(host).toHaveTextContent("Codex 与 Claude");
  await user.click(host);
  await user.click(screen.getByRole("option", { name: "仅 Claude" }));
  await user.click(within(results).getByRole("button", { name: "加入扩展库 release-notes" }));
  expect(callbacks.onImport).toHaveBeenCalledWith(candidate.digest, candidate.name, "claude");
  expect(within(results).getByText("整理版本说明")).toBeInTheDocument();
});

it("keeps successful source results visible when a refresh fails", async () => {
  const callbacks = props();
  callbacks.onScanLocal.mockResolvedValueOnce([candidate]).mockResolvedValueOnce(null);
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await user.click(screen.getByRole("button", { name: "本地目录" }));
  await user.type(screen.getByRole("textbox", { name: "本地来源目录" }), "D:\\isolated-skills");
  await user.click(screen.getByRole("button", { name: "扫描来源" }));
  await screen.findByText("release-notes");
  await user.click(screen.getByRole("button", { name: "扫描来源" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("仍显示上次结果");
  expect(screen.getByText("release-notes")).toBeInTheDocument();
});

it("clears candidates when source fields change and filters candidates locally", async () => {
  const callbacks = props();
  const user = userEvent.setup();
  render(<SkillSourceBrowser {...callbacks} />);
  await user.click(screen.getByRole("button", { name: "本地目录" }));
  const root = screen.getByRole("textbox", { name: "本地来源目录" });
  await user.type(root, "D:\\isolated-skills");
  await user.click(screen.getByRole("button", { name: "扫描来源" }));
  await user.type(await screen.findByRole("searchbox", { name: "筛选发现的 Skills" }), "unknown");
  expect(screen.queryByRole("button", { name: "加入扩展库 release-notes" })).not.toBeInTheDocument();
  expect(callbacks.onScanLocal).toHaveBeenCalledTimes(1);
  await user.type(root, "-changed");
  expect(screen.queryByRole("region", { name: "Skill 来源候选" })).not.toBeInTheDocument();
});
