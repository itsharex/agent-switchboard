use super::*;

fn source(identity: &str, subpath: &str) -> SourceRef {
    SourceRef {
        source_id: identity.into(),
        subpath: subpath.into(),
        ref_name: Some("feature/docs".into()),
        resolved_commit: Some("a".repeat(40)),
    }
}

#[test]
fn github_source_exposes_only_validated_commit_pinned_links() {
    let view =
        serde_json::to_value(skill_source_view(source("org/skills", "skills/read docs"))).unwrap();
    assert_eq!(view["repo"], "org/skills");
    assert_eq!(view["subpath"], "skills/read docs");
    assert_eq!(view["refName"], "feature/docs");
    assert_eq!(
        view["readmeUrl"],
        format!(
            "https://github.com/org/skills/blob/{}/skills/read%20docs/SKILL.md",
            "a".repeat(40)
        )
    );
    let root = serde_json::to_value(skill_source_view(source("org/skills", ""))).unwrap();
    assert_eq!(
        root["readmeUrl"],
        format!(
            "https://github.com/org/skills/blob/{}/SKILL.md",
            "a".repeat(40)
        )
    );
}

#[test]
fn local_or_unvalidated_source_identity_never_becomes_a_link() {
    for identity in [
        "C:\\private\\skills",
        "/home/user/skills",
        "$HOME/skills",
        "org/repo/extra",
        "org/repo?token=secret",
        "https://user:password@github.com/org/repo",
        "-owner/repo",
        "owner/..",
    ] {
        let view =
            serde_json::to_value(skill_source_view(source(identity, "skills/docs"))).unwrap();
        assert!(view.get("repo").is_none(), "{identity}");
        assert!(view.get("subpath").is_none());
        assert!(view.get("refName").is_none());
        assert!(view.get("readmeUrl").is_none());
        assert!(!view.to_string().contains(identity));
    }
}

#[test]
fn malformed_paths_and_missing_commits_do_not_expose_provenance() {
    for path in [
        "/private/skills",
        "C:/private/skills",
        "../skills",
        "skills/../private",
        "$HOME/skills",
        "skills\\private",
    ] {
        let view = serde_json::to_value(skill_source_view(source("org/skills", path))).unwrap();
        assert!(view.get("repo").is_none());
        assert!(view.get("readmeUrl").is_none());
    }
    for commit in [
        None,
        Some("".into()),
        Some("short".into()),
        Some("/private/path".into()),
    ] {
        let view = serde_json::to_value(skill_source_view(SourceRef {
            resolved_commit: commit,
            ..source("org/skills", "")
        }))
        .unwrap();
        assert!(view.get("repo").is_none());
        assert!(view.get("readmeUrl").is_none());
        assert!(!view.to_string().contains("/private/path"));
    }
}

#[test]
fn private_refs_never_leak_into_docs_links() {
    for name in [
        "C:\\private\\branch",
        "/home/user",
        "$TOKEN",
        "sk-private-credential",
        "bad\nbranch",
    ] {
        let view = serde_json::to_value(skill_source_view(SourceRef {
            ref_name: Some(name.into()),
            ..source("org/skills", "docs")
        }))
        .unwrap();
        assert!(view.get("refName").is_none());
        assert!(view.get("readmeUrl").is_some());
        assert!(!view.to_string().contains(name));
    }
}
