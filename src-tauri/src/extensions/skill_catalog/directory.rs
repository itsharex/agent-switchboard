use std::collections::BTreeSet;

use serde::Deserialize;

use super::{
    SkillDirectoryEntry, SkillDirectoryResult, DIRECTORY_PAGE_SIZE, MAX_DIRECTORY_RESULTS,
};
use crate::extensions::sources::{
    fetch_source_bytes, normalize_github_repository, resolve_github_source,
    validate_source_subpath, HttpFetch, SkillCandidate, SourceError, MAX_DIRECTORY_DOWNLOAD,
};

#[derive(Deserialize)]
struct DirectoryResponse {
    skills: Vec<DirectoryItem>,
    count: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DirectoryItem {
    id: String,
    skill_id: String,
    name: String,
    installs: u64,
    source: String,
}

pub fn search_directory(
    query: &str,
    offset: usize,
    fetch: HttpFetch,
) -> Result<SkillDirectoryResult, SourceError> {
    let query = query.trim();
    if !(2..=200).contains(&query.chars().count()) || query.chars().any(char::is_control) {
        return Err(SourceError::Rejected(
            "Search requires 2 to 200 characters without control characters".into(),
        ));
    }
    if offset >= MAX_DIRECTORY_RESULTS || offset % DIRECTORY_PAGE_SIZE != 0 {
        return Err(SourceError::Rejected(format!(
            "Search offset must be a multiple of {DIRECTORY_PAGE_SIZE} below {MAX_DIRECTORY_RESULTS}"
        )));
    }
    let mut url =
        reqwest::Url::parse("https://skills.sh/api/search").expect("static directory URL");
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("limit", &DIRECTORY_PAGE_SIZE.to_string())
        .append_pair("offset", &offset.to_string());
    let bytes = fetch_source_bytes(url.as_str(), MAX_DIRECTORY_DOWNLOAD, fetch)?;
    let response: DirectoryResponse = serde_json::from_slice(&bytes).map_err(|error| {
        SourceError::Unreachable(format!("skills.sh response could not be parsed: {error}"))
    })?;
    if response.skills.len() > DIRECTORY_PAGE_SIZE {
        return Err(SourceError::Rejected(
            "skills.sh returned more entries than the requested page".into(),
        ));
    }
    let raw_count = response.skills.len();
    let total = response.count.min(MAX_DIRECTORY_RESULTS);
    let mut ids = BTreeSet::new();
    let items = response
        .skills
        .into_iter()
        .filter_map(directory_entry)
        .filter(|entry| ids.insert(entry.id.clone()))
        .collect();
    Ok(SkillDirectoryResult {
        items,
        total,
        has_more: raw_count > 0 && offset + DIRECTORY_PAGE_SIZE < total,
    })
}

fn directory_entry(item: DirectoryItem) -> Option<SkillDirectoryEntry> {
    if !valid_label(&item.id, 1024)
        || !valid_label(&item.name, 256)
        || item.skill_id.is_empty()
        || item.skill_id.len() > 1024
    {
        return None;
    }
    let repo = normalize_github_repository(&item.source).ok()?;
    validate_source_subpath(&item.skill_id).ok()?;
    Some(SkillDirectoryEntry {
        id: item.id,
        name: item.name,
        subpath: item.skill_id,
        installs: item.installs.min(9_007_199_254_740_991),
        readme_url: Some(format!("https://github.com/{repo}")),
        repo,
    })
}

fn valid_label(label: &str, limit: usize) -> bool {
    !label.trim().is_empty() && label.len() <= limit && !label.chars().any(char::is_control)
}

/// Directory entries are hints, not trusted download URLs. Resolve the real
/// manifest path at one immutable GitHub commit before caching any content.
pub fn resolve_directory_skill(
    entry: &SkillDirectoryEntry,
    fetch: HttpFetch,
) -> Result<Vec<SkillCandidate>, SourceError> {
    let repo = normalize_github_repository(&entry.repo)?;
    if !valid_label(&entry.id, 1024)
        || !valid_label(&entry.name, 256)
        || entry.subpath.is_empty()
        || entry.subpath.len() > 1024
    {
        return Err(SourceError::Rejected(
            "Directory entry is invalid; search again".into(),
        ));
    }
    validate_source_subpath(&entry.subpath)?;
    let candidates = resolve_github_source(&repo, "", None, fetch)?;
    if let Some(candidate) = candidates
        .iter()
        .find(|candidate| candidate.subpath == entry.subpath)
    {
        return Ok(vec![candidate.clone()]);
    }
    let name = entry
        .subpath
        .rsplit('/')
        .next()
        .expect("nonempty source path");
    let matches: Vec<_> = candidates
        .iter()
        .filter(|candidate| {
            candidate
                .subpath
                .rsplit('/')
                .next()
                .is_some_and(|part| part.eq_ignore_ascii_case(name))
        })
        .collect();
    match matches.as_slice() {
        [candidate] => Ok(vec![(*candidate).clone()]),
        [] if candidates.len() == 1 && candidates[0].subpath.is_empty() => Ok(candidates),
        [] => Err(SourceError::Rejected("Skill manifest was not found in the repository; refresh the search or use an explicit repository subpath".into())),
        _ => Err(SourceError::Rejected("Several manifest paths match this Skill; use an explicit repository subpath".into())),
    }
}
