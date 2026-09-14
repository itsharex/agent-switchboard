import { invoke } from "../client";
import type { SkillCandidateDto } from "./types";

export interface SkillRepository {
  id: string;
  repo: string;
  subpath: string;
  refName: string | null;
  enabled: boolean;
}

export type SkillRepositoryInput = Omit<SkillRepository, "id"> & { id?: string };

export interface SkillCatalogCandidate extends SkillCandidateDto {
  repositoryId: string;
  repo: string;
  subpath: string;
  refName: string | null;
  readmeUrl: string | null;
}

export interface SkillCatalogResult {
  candidates: SkillCatalogCandidate[];
  failures: { repositoryId: string; repo: string; message: string }[];
}

export interface SkillDirectoryEntry {
  id: string;
  name: string;
  repo: string;
  /** Directory search may return a slug; resolution finds its actual source path. */
  subpath: string;
  installs: number;
  readmeUrl: string | null;
}

export interface SkillDirectoryResult {
  items: SkillDirectoryEntry[];
  total: number;
  hasMore: boolean;
}

export function listSkillRepositories(): Promise<SkillRepository[]> {
  return invoke<SkillRepository[]>("list_skill_repositories");
}

export function saveSkillRepository(input: SkillRepositoryInput): Promise<SkillRepository> {
  return invoke<SkillRepository>("save_skill_repository", { input });
}

export function removeSkillRepository(id: string): Promise<void> {
  return invoke<void>("remove_skill_repository", { id });
}

/** Explicit network refresh. Saving or listing repositories never fetches them. */
export function scanSkillRepositories(): Promise<SkillCatalogResult> {
  return invoke<SkillCatalogResult>("scan_skill_repositories");
}

/** Pages contain up to 20 entries; offsets are bounded by the backend. */
export function searchSkillDirectory(query: string, offset = 0): Promise<SkillDirectoryResult> {
  return invoke<SkillDirectoryResult>("search_skill_directory", { query, offset });
}

export function resolveDirectorySkill(entry: SkillDirectoryEntry): Promise<SkillCandidateDto[]> {
  return invoke<SkillCandidateDto[]>("resolve_directory_skill", { entry });
}

/** Reads candidates only; import and deployment use the existing library/executor. */
export function scanSkillZip(path: string): Promise<SkillCandidateDto[]> {
  return invoke<SkillCandidateDto[]>("scan_skill_zip", { path });
}
