use super::{SkillRepository, SkillRepositoryFailure};
use crate::extensions::sources::{
    resolve_github_source, HttpFetch, SkillCandidate, SourceError, MAX_SOURCE_BYTES,
    MAX_SOURCE_CANDIDATES,
};

#[derive(Debug)]
pub struct RepositoryScan {
    pub repository: SkillRepository,
    pub candidates: Vec<SkillCandidate>,
}

#[derive(Debug, Default)]
pub struct CatalogScan {
    pub repositories: Vec<RepositoryScan>,
    pub failures: Vec<SkillRepositoryFailure>,
}

/// Each enabled repository is pinned independently. A failed source does not
/// discard other sources, and aggregate limits also bound a many-repo scan.
pub fn scan_repositories(repositories: &[SkillRepository], fetch: HttpFetch) -> CatalogScan {
    let mut result = CatalogScan::default();
    let (mut count, mut bytes) = (0usize, 0u64);
    for repo in repositories.iter().filter(|repo| repo.enabled) {
        let scanned = resolve_github_source(&repo.repo, &repo.subpath, repo.ref_name.as_deref(), fetch)
            .and_then(|candidates| {
                let source_bytes: u64 = candidates.iter().flat_map(|item| &item.entries)
                    .map(|entry| entry.size()).sum();
                if count + candidates.len() > MAX_SOURCE_CANDIDATES || bytes + source_bytes > MAX_SOURCE_BYTES {
                    return Err(SourceError::Rejected(format!(
                        "Catalog scan exceeds {MAX_SOURCE_CANDIDATES} candidates or {MAX_SOURCE_BYTES} bytes; disable repositories or narrow their subpaths"
                    )));
                }
                count += candidates.len();
                bytes += source_bytes;
                Ok(candidates)
            });
        match scanned {
            Ok(candidates) => result.repositories.push(RepositoryScan {
                repository: repo.clone(),
                candidates,
            }),
            Err(error) => result.failures.push(SkillRepositoryFailure {
                repository_id: repo.id.clone(),
                repo: repo.repo.clone(),
                message: error.to_string(),
            }),
        }
    }
    result
}
