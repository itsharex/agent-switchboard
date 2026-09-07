//! Skill update diff tests: added, modified, and removed file
//! projections between content versions.
#![cfg(test)]

use super::*;
use asb_core::extensions::validate::ContentEntryKind;

fn file(path: &str, bytes: &[u8]) -> ContentEntry {
    ContentEntry {
        relative_path: path.to_string(),
        kind: ContentEntryKind::File,
        bytes: bytes.to_vec(),
        mode: 0o644,
    }
}

fn dir(path: &str) -> ContentEntry {
    ContentEntry {
        relative_path: path.to_string(),
        kind: ContentEntryKind::Dir,
        bytes: Vec::new(),
        mode: 0o755,
    }
}

fn actions(changes: &[SkillFileChange]) -> Vec<(String, SkillFileChangeAction)> {
    changes
        .iter()
        .map(|change| (change.relative_path.clone(), change.action))
        .collect()
}

#[test]
fn file_diff_reports_added_modified_and_removed_files() {
    let current = vec![
        file("SKILL.md", b"old"),
        file("references/old.md", b"x"),
        file("references/keep.md", b"same"),
        dir("assets"),
    ];
    let candidate = vec![
        file("SKILL.md", b"new"),
        file("references/keep.md", b"same"),
        file("references/new.md", b"y"),
        dir("assets"),
    ];
    assert_eq!(
        actions(&skill_file_changes(&current, &candidate)),
        vec![
            ("SKILL.md".to_string(), SkillFileChangeAction::Modified),
            (
                "references/new.md".to_string(),
                SkillFileChangeAction::Added
            ),
            (
                "references/old.md".to_string(),
                SkillFileChangeAction::Removed
            ),
        ]
    );
}

#[test]
fn identical_content_yields_no_file_changes() {
    let entries = vec![file("SKILL.md", b"same"), dir("assets")];
    assert!(skill_file_changes(&entries, &entries).is_empty());
}
