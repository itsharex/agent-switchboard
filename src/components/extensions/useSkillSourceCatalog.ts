import {
  scanSkillRepositories, type SkillCatalogResult, type SkillRepository,
} from "../../api/extensions/skill-sources";
import { sameSkillRepository } from "./skill-repository-model";
import { useSkillSourceRequests } from "./useSkillSourceRequests";

interface CatalogSnapshot extends SkillCatalogResult { scanned: SkillRepository[]; configured: SkillRepository[] }

function mergeCatalog(next: SkillCatalogResult, previous: CatalogSnapshot | null, repositories: SkillRepository[]) {
  const failed = new Set(next.failures.map((failure) => failure.repositoryId));
  const retained = previous?.scanned.filter((repo) => failed.has(repo.id) &&
    repositories.some((current) => sameSkillRepository(current, repo))) ?? [];
  const retainedIds = new Set(retained.map((repo) => repo.id));
  return {
    ...next,
    candidates: [...next.candidates, ...(previous?.candidates.filter((row) => retainedIds.has(row.repositoryId)) ?? [])],
    scanned: [...repositories.filter((repo) => repo.enabled && !failed.has(repo.id)), ...retained],
    configured: repositories,
  };
}

export function useSkillSourceCatalog(busy: boolean) {
  const requests = useSkillSourceRequests<CatalogSnapshot>(busy);
  const refresh = async (repositories: SkillRepository[]) => {
    await requests.run(async () => mergeCatalog(await scanSkillRepositories(), requests.result, repositories));
  };
  const reconcile = (repositories: SkillRepository[] | null) => {
    requests.cancel();
    requests.update((previous) => {
      if (!previous || !repositories) return null;
      const allowed = new Set(previous.configured.filter((repo) =>
        repositories.some((current) => sameSkillRepository(current, repo))).map((repo) => repo.id));
      return {
        candidates: previous.candidates.filter((row) => allowed.has(row.repositoryId)),
        failures: previous.failures.filter((row) => allowed.has(row.repositoryId)),
        scanned: previous.scanned.filter((repo) => allowed.has(repo.id)),
        configured: previous.configured.filter((repo) => allowed.has(repo.id)),
      };
    });
  };
  const count = (id: string) => requests.result?.scanned.some((repo) => repo.id === id) ?
    requests.result.candidates.filter((candidate) => candidate.repositoryId === id).length : null;
  return { ...requests, refresh, reconcile, count };
}
