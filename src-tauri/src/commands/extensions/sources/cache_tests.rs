use super::*;
use asb_core::extensions::skill::{content_digest, ContentEntry};
use asb_core::extensions::validate::ContentEntryKind;

fn candidate() -> SkillCandidate {
    let name = format!("cache-{}", uuid::Uuid::new_v4());
    let entries = vec![ContentEntry {
        relative_path: "SKILL.md".into(),
        kind: ContentEntryKind::File,
        bytes: format!("---\nname: {name}\ndescription: Cache fixture\n---\n").into_bytes(),
        mode: 0o644,
    }];
    SkillCandidate {
        source_identity: "org/original".into(),
        subpath: "skills/example".into(),
        name,
        description: Some("Cache fixture".into()),
        ref_name: Some("main".into()),
        resolved_commit: Some("0123456789abcdef0123456789abcdef01234567".into()),
        content_digest: content_digest(&entries),
        entries,
        diagnostics: Vec::new(),
    }
}

#[test]
fn digest_conflicts_never_rewrite_cached_or_installed_provenance() {
    let temp = tempfile::tempdir().unwrap();
    let store = ExtensionStore::from_root(temp.path().join("library"));
    let original = candidate();
    let digest = original.content_digest.clone();
    cache_candidates(vec![original.clone()]).unwrap();
    let installed =
        import_cached_skill_candidate(&store, &digest, original.name.clone(), None).unwrap();
    for alternate in [
        SkillCandidate {
            source_identity: "org/alternate".into(),
            ..original.clone()
        },
        SkillCandidate {
            subpath: "other/example".into(),
            ..original.clone()
        },
        SkillCandidate {
            ref_name: Some("other-branch".into()),
            ..original.clone()
        },
    ] {
        assert_eq!(
            cache_candidates(vec![alternate]).unwrap_err().code,
            "candidate-source-conflict"
        );
        assert_eq!(candidates().lock().unwrap().get(&digest), Some(&original));
    }
    let reused = import_cached_skill_candidate(&store, &digest, "renamed".into(), None).unwrap();
    assert_eq!(installed.id, reused.id);
    let definition = store.get_definition(&installed.id).unwrap().unwrap();
    let ExtensionPayload::Skill(skill) = definition.payload else {
        panic!("not a skill")
    };
    let source = skill.source.unwrap();
    assert_eq!(source.source_id, original.source_identity);
    assert_eq!(source.subpath, original.subpath);
    assert_eq!(source.ref_name, original.ref_name);
    assert_eq!(source.resolved_commit, original.resolved_commit);
}

#[test]
fn a_conflicting_batch_does_not_insert_its_earlier_healthy_candidates() {
    let original = candidate();
    let fresh = candidate();
    cache_candidates(vec![original.clone()]).unwrap();
    let alternate = SkillCandidate {
        source_identity: "org/alternate".into(),
        ..original
    };
    assert!(cache_candidates(vec![fresh.clone(), alternate]).is_err());
    assert!(!candidates()
        .lock()
        .unwrap()
        .contains_key(&fresh.content_digest));
}

#[test]
fn an_identified_local_observation_does_not_replace_or_conflict_with_remote_cache_provenance() {
    let temp = tempfile::tempdir().unwrap();
    let store = ExtensionStore::from_root(temp.path().join("library"));
    let original = candidate();
    cache_candidates(vec![original.clone()]).unwrap();
    let remote = import_cached_skill_candidate(
        &store,
        &original.content_digest,
        original.name.clone(),
        None,
    )
    .unwrap();
    let observed = SkillCandidate {
        source_identity: temp.path().join("observed").to_string_lossy().into(),
        ref_name: None,
        resolved_commit: None,
        ..original.clone()
    };
    let local = import_candidate_content(
        &store,
        &observed,
        observed.name.clone(),
        Some(AppKind::Codex),
    )
    .unwrap();
    assert_ne!(local.id, remote.id);
    assert_eq!(
        candidates().lock().unwrap().get(&original.content_digest),
        Some(&original)
    );
    let definition = store.get_definition(&local.id).unwrap().unwrap();
    let ExtensionPayload::Skill(skill) = definition.payload else {
        panic!("not a skill")
    };
    assert_eq!(skill.source.unwrap().source_id, observed.source_identity);
}

#[test]
fn same_source_rescans_do_not_change_the_commit_of_an_existing_immutable_candidate() {
    let original = candidate();
    cache_candidates(vec![original.clone()]).unwrap();
    let changed = SkillCandidate {
        resolved_commit: Some("fedcba9876543210fedcba9876543210fedcba98".into()),
        ..original.clone()
    };
    cache_candidates(vec![changed]).unwrap();
    assert_eq!(
        candidates().lock().unwrap().get(&original.content_digest),
        Some(&original)
    );
}

#[test]
fn unchanged_document_read_preserves_mcp_staleness_and_text_validation() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("mcp.json");
    let name = path.to_str().unwrap();
    assert_eq!(
        read_unchanged_mcp_document(name, None).unwrap_err().code,
        "observation-stale"
    );
    let bytes = br#"{"mcpServers":{}}"#;
    fs::write(&path, bytes).unwrap();
    assert_eq!(
        read_unchanged_mcp_document(name, Some(&sha_hex(bytes)))
            .unwrap()
            .as_bytes(),
        bytes
    );
    assert_eq!(
        read_unchanged_mcp_document(name, None).unwrap_err().code,
        "observation-stale"
    );
    fs::write(&path, [0xff]).unwrap();
    assert_eq!(
        read_unchanged_mcp_document(name, Some(&sha_hex(bytes)))
            .unwrap_err()
            .code,
        "observation-stale"
    );
    assert_eq!(
        read_unchanged_mcp_document(name, Some(&sha_hex(&[0xff])))
            .unwrap_err()
            .code,
        "source-rejected"
    );
}
