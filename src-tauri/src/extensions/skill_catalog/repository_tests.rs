use std::fs;

use super::*;

fn input(repo: &str) -> SkillRepositoryInput {
    SkillRepositoryInput {
        id: None,
        repo: repo.into(),
        subpath: String::new(),
        ref_name: None,
        enabled: true,
    }
}

#[test]
fn defaults_match_the_reference_without_a_hidden_read_time_write() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("extensions");
    let catalog = SkillRepositoryCatalog::from_root(root.clone());
    let repositories = catalog.list().unwrap();
    let expected = [
        ("anthropics/skills", "main"),
        ("composiohq/awesome-claude-skills", "master"),
        ("cexll/myclaude", "master"),
        ("jimliu/baoyu-skills", "main"),
    ];
    assert_eq!(repositories.len(), expected.len());
    for (repository, (repo, branch)) in repositories.iter().zip(expected) {
        assert_eq!(repository.repo, repo);
        assert_eq!(repository.ref_name.as_deref(), Some(branch));
        assert!(repository.enabled && repository.subpath.is_empty());
    }
    assert_eq!(catalog.list().unwrap(), repositories);
    assert!(!root.exists());
}

#[test]
fn save_normalizes_coordinates_and_update_keeps_a_stable_id() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = SkillRepositoryCatalog::from_root(temp.path().to_path_buf());
    let mut new = input(" https://github.com/Owner/Skills.git/ ");
    new.subpath = " skills ".into();
    new.ref_name = Some(" feature/skills ".into());
    let saved = catalog.save(new).unwrap();
    assert_eq!(saved.repo, "owner/skills");
    assert_eq!(saved.subpath, "skills");
    assert_eq!(saved.ref_name.as_deref(), Some("feature/skills"));
    let changed = catalog
        .save(SkillRepositoryInput {
            id: Some(saved.id.clone()),
            repo: "owner/replacement".into(),
            subpath: "plugins/skills".into(),
            ref_name: None,
            enabled: false,
        })
        .unwrap();
    assert_eq!(changed.id, saved.id);
    let reopened = SkillRepositoryCatalog::from_root(temp.path().to_path_buf());
    assert!(reopened.list().unwrap().contains(&changed));
    assert!(!reopened.list().unwrap().contains(&saved));
}

#[test]
fn coordinate_upserts_preserve_identity_and_empty_catalogs_are_not_reseeded() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = SkillRepositoryCatalog::from_root(temp.path().to_path_buf());
    let first = catalog.save(input("owner/skills")).unwrap();
    let mut update = input("OWNER/SKILLS");
    update.enabled = false;
    let second = catalog.save(update).unwrap();
    assert_eq!(first.id, second.id);
    assert!(!second.enabled);
    for repo in catalog.list().unwrap() {
        catalog.remove(&repo.id).unwrap();
    }
    assert!(SkillRepositoryCatalog::from_root(temp.path().to_path_buf())
        .list()
        .unwrap()
        .is_empty());
    assert!(temp.path().join("skill-repositories.json").is_file());
}

#[test]
fn invalid_mutations_and_conflicting_updates_leave_the_saved_catalog_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = SkillRepositoryCatalog::from_root(temp.path().to_path_buf());
    let one = catalog.save(input("owner/one")).unwrap();
    catalog.save(input("owner/two")).unwrap();
    let before = fs::read(temp.path().join("skill-repositories.json")).unwrap();
    let mut conflict = input("owner/two");
    conflict.id = Some(one.id);
    assert!(matches!(
        catalog.save(conflict),
        Err(CatalogError::Conflict)
    ));
    let mut missing = input("owner/three");
    missing.id = Some("missing".into());
    assert!(matches!(catalog.save(missing), Err(CatalogError::NotFound)));
    assert!(catalog.remove("missing").is_err());
    for repo in [
        "owner",
        "owner/a/b",
        "https://token@github.com/owner/repo",
        "evil.invalid/repo",
    ] {
        assert!(catalog.save(input(repo)).is_err(), "{repo}");
    }
    for subpath in ["../escape", "/absolute", "C:/outside", "skills/CON", "a\\b"] {
        let mut invalid = input("owner/three");
        invalid.subpath = subpath.into();
        assert!(catalog.save(invalid).is_err(), "{subpath}");
    }
    let mut invalid = input("owner/three");
    invalid.ref_name = Some("bad\nref".into());
    assert!(catalog.save(invalid).is_err());
    assert_eq!(
        fs::read(temp.path().join("skill-repositories.json")).unwrap(),
        before
    );
}

#[test]
fn corrupt_or_foreign_catalogs_are_reported_not_reset_or_overwritten() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("skill-repositories.json");
    let catalog = SkillRepositoryCatalog::from_root(temp.path().to_path_buf());
    for bytes in [
        b"not json".to_vec(),
        br#"{"schemaVersion":2,"repositories":[]}"#.to_vec(),
        br#"{"schemaVersion":1,"repositories":[],"unknown":true}"#.to_vec(),
        vec![b' '; 128 * 1024 + 1],
    ] {
        fs::write(&path, &bytes).unwrap();
        assert!(matches!(catalog.list(), Err(CatalogError::Unsupported(_))));
        assert!(catalog.save(input("owner/repo")).is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn saved_duplicate_and_noncanonical_records_are_rejected() {
    let temp = tempfile::tempdir().unwrap();
    let catalog = SkillRepositoryCatalog::from_root(temp.path().to_path_buf());
    let repo = catalog.list().unwrap().remove(0);
    let mut noncanonical = repo.clone();
    noncanonical.repo = "Anthropics/skills".into();
    for repositories in [vec![repo.clone(), repo], vec![noncanonical]] {
        let value = serde_json::json!({"schemaVersion": 1, "repositories": repositories});
        fs::write(
            temp.path().join("skill-repositories.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
        assert!(matches!(catalog.list(), Err(CatalogError::Unsupported(_))));
    }
}

#[test]
fn catalog_is_bounded_and_concurrent_saves_do_not_lose_records() {
    let temp = tempfile::tempdir().unwrap();
    let handles: Vec<_> = (0..8)
        .map(|index| {
            let root = temp.path().to_path_buf();
            std::thread::spawn(move || {
                SkillRepositoryCatalog::from_root(root)
                    .save(input(&format!("owner/repo-{index}")))
                    .unwrap()
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let catalog = SkillRepositoryCatalog::from_root(temp.path().to_path_buf());
    assert_eq!(catalog.list().unwrap().len(), 12);
    for index in 12..MAX_REPOSITORIES {
        catalog.save(input(&format!("owner/repo-{index}"))).unwrap();
    }
    assert!(catalog.save(input("owner/excess")).is_err());
    assert_eq!(catalog.list().unwrap().len(), MAX_REPOSITORIES);
}
