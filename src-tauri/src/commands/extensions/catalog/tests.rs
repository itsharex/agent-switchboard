use super::*;
use crate::extensions::skill_catalog::{CatalogScan, RepositoryScan};

fn group(id: &str, repo: &str) -> RepositoryScan {
    let root = tempfile::tempdir().unwrap();
    let name = format!("catalog-{}", uuid::Uuid::new_v4());
    std::fs::write(
        root.path().join("SKILL.md"),
        format!("---\nname: {name}\ndescription: Catalog fixture\n---\n"),
    )
    .unwrap();
    let mut candidate = sources::scan_local_source(root.path(), None)
        .unwrap()
        .remove(0);
    candidate.source_identity = repo.into();
    candidate.subpath = "skills/test skill".into();
    candidate.ref_name = Some("feature/skills".into());
    candidate.resolved_commit = Some("0123456789abcdef0123456789abcdef01234567".into());
    RepositoryScan {
        repository: SkillRepository {
            id: id.into(),
            repo: repo.into(),
            subpath: "skills".into(),
            ref_name: candidate.ref_name.clone(),
            enabled: true,
        },
        candidates: vec![candidate],
    }
}

#[test]
fn catalog_projection_matches_the_typed_api_without_leaking_file_contents() {
    let scanned = group("repository-1", "org/repo");
    let digest = scanned.candidates[0].content_digest.clone();
    let result = cache_catalog_scan(CatalogScan {
        repositories: vec![scanned],
        failures: vec![],
    });
    let value = serde_json::to_value(result).unwrap();
    let candidate = &value["candidates"][0];
    assert_eq!(candidate["digest"], digest);
    assert_eq!(candidate["repositoryId"], "repository-1");
    assert_eq!(candidate["repo"], "org/repo");
    assert_eq!(candidate["subpath"], "skills/test skill");
    assert_eq!(candidate["refName"], "feature/skills");
    assert_eq!(candidate["fileCount"], 1);
    assert_eq!(candidate["readmeUrl"], "https://github.com/org/repo/blob/0123456789abcdef0123456789abcdef01234567/skills/test%20skill/SKILL.md");
    assert!(candidate.get("entries").is_none());
    assert!(candidate.get("sourceIdentity").is_none());
    assert!(value["failures"].as_array().unwrap().is_empty());
}

#[test]
fn one_repository_cache_conflict_keeps_other_healthy_repository_results() {
    let original = group("original", "org/original");
    let mut conflicting = group("conflicting", "org/conflicting");
    conflicting.candidates = vec![SkillCandidate {
        source_identity: "org/conflicting".into(),
        ..original.candidates[0].clone()
    }];
    let healthy = group("healthy", "org/healthy");
    let result = cache_catalog_scan(CatalogScan {
        repositories: vec![original, conflicting, healthy],
        failures: vec![],
    });
    assert_eq!(result.candidates.len(), 2);
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].repository_id, "conflicting");
    assert_eq!(result.failures[0].repo, "org/conflicting");
    assert!(result.failures[0]
        .message
        .contains("existing source was retained"));
}

#[test]
fn command_input_contracts_accept_camel_case_and_reject_unknown_fields() {
    let input = serde_json::json!({"repo":"org/repo","subpath":"","refName":null,"enabled":true});
    assert!(
        serde_json::from_value::<SkillRepositoryInput>(input.clone())
            .unwrap()
            .id
            .is_none()
    );
    let mut unknown = input;
    unknown["command"] = "do not execute".into();
    assert!(serde_json::from_value::<SkillRepositoryInput>(unknown).is_err());
    let entry = serde_json::json!({"id":"skill","name":"Skill","repo":"org/repo","subpath":"skill","installs":1,"readmeUrl":null});
    assert_eq!(
        serde_json::from_value::<SkillDirectoryEntry>(entry)
            .unwrap()
            .subpath,
        "skill"
    );
}
