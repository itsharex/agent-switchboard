import { beforeEach, expect, vi } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import type userEvent from "@testing-library/user-event";
import type { ExtensionListItem, SkillCandidateDto } from "../../api/client";
import * as api from "../../api/extensions/skill-sources";

vi.mock("../../api/extensions/skill-sources", () => ({
  listSkillRepositories: vi.fn(), saveSkillRepository: vi.fn(), removeSkillRepository: vi.fn(),
  scanSkillRepositories: vi.fn(), searchSkillDirectory: vi.fn(), resolveDirectorySkill: vi.fn(), scanSkillZip: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn().mockResolvedValue(undefined) }));

export const sourceApi = vi.mocked(api);
export const candidate: SkillCandidateDto = {
  digest: "a".repeat(64), name: "release-notes", description: "整理版本说明", fileCount: 3, diagnostics: [],
};
export const second: SkillCandidateDto = { ...candidate, digest: "b".repeat(64), name: "api-spec", description: "API 文档" };
export const repository: api.SkillRepository = { id: "repo-a", repo: "example/skills", subpath: "", refName: null, enabled: true };
export const otherRepository: api.SkillRepository = { ...repository, id: "repo-b", repo: "another/tools" };
export const directoryEntry: api.SkillDirectoryEntry = {
  id: "entry-a", name: candidate.name, repo: repository.repo, subpath: "skills/release-notes",
  installs: 1234, readmeUrl: "https://skills.sh/example/skills/release-notes",
};
export const otherDirectoryEntry: api.SkillDirectoryEntry = {
  ...directoryEntry, id: "entry-b", name: second.name, subpath: "skills/api-spec", installs: 42,
};

export function catalogCandidate(skill: SkillCandidateDto, repo = repository): api.SkillCatalogCandidate {
  return { ...skill, repositoryId: repo.id, repo: repo.repo, subpath: `skills/${skill.name}`,
    refName: repo.refName, readmeUrl: `https://github.com/${repo.repo}/blob/main/skills/${skill.name}/SKILL.md` };
}

beforeEach(() => {
  for (const mock of Object.values(sourceApi)) mock.mockReset();
  sourceApi.listSkillRepositories.mockResolvedValue([repository]);
  sourceApi.saveSkillRepository.mockImplementation(async (input) => ({ ...input, id: input.id ?? "new-repo" }));
  sourceApi.removeSkillRepository.mockResolvedValue(undefined);
  sourceApi.scanSkillRepositories.mockResolvedValue({ candidates: [catalogCandidate(candidate), catalogCandidate(second)], failures: [] });
  sourceApi.searchSkillDirectory.mockResolvedValue({ items: [directoryEntry, otherDirectoryEntry], total: 2, hasMore: false });
  sourceApi.resolveDirectorySkill.mockResolvedValue([candidate]);
  sourceApi.scanSkillZip.mockResolvedValue([candidate, second]);
});

export function sourceProps() {
  return {
    busy: false, items: [] as ExtensionListItem[],
    onScanLocal: vi.fn().mockResolvedValue([candidate]),
    onImport: vi.fn().mockResolvedValue({ id: "added", name: candidate.name, revision: 1 }),
    onPickDirectory: vi.fn().mockResolvedValue(null), onPickZip: vi.fn().mockResolvedValue(null),
  };
}

export function libraryItem(installed: boolean, clients: ("codex" | "claude")[] = ["codex", "claude"]): ExtensionListItem {
  return {
    schemaVersion: 1, id: "added", name: candidate.name, revision: 1, createdAt: "now", updatedAt: "now",
    kind: "skill", contentDigest: candidate.digest, manifest: { name: candidate.name, description: candidate.description },
    source: null, hostScoped: null, compatibility: [], dependencies: [], dependencyStates: [], lastCheck: null,
    bindings: installed ? clients.map((client) => ({
      schemaVersion: 1, id: `binding-${client}`, resourceId: "added", target: { scope: "app", client },
      desired: "enabled", lastAppliedRevision: 1, fileState: "inSync", warnings: [], updatedAt: "now",
    })) : [],
  };
}

export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

export async function refreshCatalog(user: ReturnType<typeof userEvent.setup>) {
  await waitFor(() => expect(screen.getByRole("button", { name: "刷新仓库" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "刷新仓库" }));
  return screen.findByRole("region", { name: "Skill 来源候选" });
}

export async function searchDirectory(user: ReturnType<typeof userEvent.setup>, query = "release") {
  await user.click(screen.getByRole("radio", { name: "skills.sh" }));
  await user.type(screen.getByRole("searchbox", { name: "搜索 skills.sh" }), query);
  await user.click(screen.getByRole("button", { name: "搜索" }));
  return screen.findByRole("region", { name: "skills.sh 搜索结果" });
}
