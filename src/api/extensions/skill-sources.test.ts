import { beforeEach, describe, expect, it, vi } from "vitest";
import {
  listSkillRepositories,
  removeSkillRepository,
  resolveDirectorySkill,
  saveSkillRepository,
  scanSkillRepositories,
  scanSkillZip,
  searchSkillDirectory,
  type SkillCatalogResult,
  type SkillDirectoryEntry,
  type SkillDirectoryResult,
  type SkillRepository,
} from "./skill-sources";
import type { SkillCandidateDto } from "./types";

vi.mock("../client", () => ({ invoke: vi.fn() }));
import { invoke } from "../client";

const invokeMock = vi.mocked(invoke);
const repository: SkillRepository = {
  id: "repo-one", repo: "owner/skills", subpath: "skills", refName: "main", enabled: true,
};
const candidate: SkillCandidateDto = {
  digest: "a".repeat(64), name: "example", description: "Example skill", fileCount: 2,
  diagnostics: [],
};
const entry: SkillDirectoryEntry = {
  id: "owner/skills/example", name: "example", repo: "owner/skills", subpath: "example",
  installs: 25, readmeUrl: "https://github.com/owner/skills",
};

describe("Skill source API", () => {
  beforeEach(() => { invokeMock.mockReset(); });

  it("lists, creates, updates and removes repositories without starting a scan", async () => {
    invokeMock.mockResolvedValueOnce([repository]).mockResolvedValueOnce(repository)
      .mockResolvedValueOnce({ ...repository, enabled: false }).mockResolvedValueOnce(undefined);
    const input = { repo: repository.repo, subpath: "skills", refName: "main", enabled: true };
    expect(await listSkillRepositories()).toEqual([repository]);
    expect(await saveSkillRepository(input)).toEqual(repository);
    expect(await saveSkillRepository({ ...repository, enabled: false })).toEqual({
      ...repository, enabled: false,
    });
    expect(await removeSkillRepository(repository.id)).toBeUndefined();
    expect(invokeMock.mock.calls).toEqual([
      ["list_skill_repositories"], ["save_skill_repository", { input }],
      ["save_skill_repository", { input: { ...repository, enabled: false } }],
      ["remove_skill_repository", { id: repository.id }],
    ]);
  });

  it("keeps healthy repository candidates together with per-repository failures", async () => {
    const result: SkillCatalogResult = {
      candidates: [{
        ...candidate, repositoryId: repository.id, repo: repository.repo,
        subpath: "skills/example", refName: "main", readmeUrl: null,
      }],
      failures: [{ repositoryId: "repo-offline", repo: "owner/offline", message: "HTTP 429" }],
    };
    invokeMock.mockResolvedValue(result);
    expect(await scanSkillRepositories()).toBe(result);
    expect(invokeMock).toHaveBeenCalledExactlyOnceWith("scan_skill_repositories");
  });

  it("submits only the requested directory page and preserves the pagination contract", async () => {
    const result: SkillDirectoryResult = { items: [entry], total: 42, hasMore: true };
    invokeMock.mockResolvedValue(result);
    expect(await searchSkillDirectory("example")).toBe(result);
    expect(await searchSkillDirectory("example", 20)).toBe(result);
    expect(invokeMock.mock.calls).toEqual([
      ["search_skill_directory", { query: "example", offset: 0 }],
      ["search_skill_directory", { query: "example", offset: 20 }],
    ]);
  });

  it("resolves directory and ZIP candidates without calling any installation endpoint", async () => {
    invokeMock.mockResolvedValue([candidate]);
    expect(await resolveDirectorySkill(entry)).toEqual([candidate]);
    expect(await scanSkillZip("D:\\fixtures\\skills.zip")).toEqual([candidate]);
    expect(invokeMock.mock.calls).toEqual([
      ["resolve_directory_skill", { entry }],
      ["scan_skill_zip", { path: "D:\\fixtures\\skills.zip" }],
    ]);
  });

  it("propagates actionable backend errors instead of converting them to empty results", async () => {
    const error = { code: "source-unreachable", message: "skills.sh returned HTTP 429" };
    invokeMock.mockRejectedValue(error);
    await expect(searchSkillDirectory("example")).rejects.toBe(error);
    await expect(scanSkillRepositories()).rejects.toBe(error);
    await expect(scanSkillZip("D:\\fixtures\\bad.zip")).rejects.toBe(error);
  });
});
