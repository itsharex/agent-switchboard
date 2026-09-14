use std::fs;

use super::tests::{ALPHA, BETA};
use super::*;

#[test]
fn local_source_accepts_a_skill_or_a_directory_of_skills() {
    let temp = tempfile::tempdir().unwrap();
    let alpha = temp.path().join("alpha");
    let beta = temp.path().join("beta");
    fs::create_dir(&alpha).unwrap();
    fs::create_dir(&beta).unwrap();
    fs::write(alpha.join("SKILL.md"), ALPHA).unwrap();
    fs::write(beta.join("SKILL.md"), BETA).unwrap();
    let candidates = scan_local_source(temp.path(), None).unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(candidates[0].subpath, "alpha");
    assert_eq!(candidates[1].subpath, "beta");
    let single = scan_local_source(&alpha, None).unwrap();
    assert_eq!(single.len(), 1);
    assert_eq!(single[0].name, "alpha");
    assert_eq!(single[0].content_digest, candidates[0].content_digest);
    assert!(single[0].subpath.is_empty());
}

#[test]
fn malformed_local_manifests_are_reported_instead_of_omitted() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(
        temp.path().join("SKILL.md"),
        b"---\ndescription: missing name\n---\n",
    )
    .unwrap();
    let candidates = scan_local_source(temp.path(), None).unwrap();
    assert_eq!(candidates.len(), 1);
    assert!(candidates[0].diagnostics[0].contains("name"));
}

#[test]
fn local_read_errors_keep_the_path_and_oversized_files_are_not_read() {
    let temp = tempfile::tempdir().unwrap();
    let missing = temp.path().join("missing");
    let error = scan_local_source(&missing, None).unwrap_err().to_string();
    assert!(error.contains("missing"));
    fs::write(temp.path().join("SKILL.md"), ALPHA).unwrap();
    fs::File::create(temp.path().join("large.bin"))
        .unwrap()
        .set_len(asb_core::extensions::validate::MAX_CONTENT_FILE_BYTES + 1)
        .unwrap();
    let error = scan_local_source(temp.path(), None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("large.bin") && error.contains("上限"));
}

#[cfg(unix)]
#[test]
fn local_links_are_rejected_without_reading_the_target() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("SKILL.md"), ALPHA).unwrap();
    std::os::unix::fs::symlink("/does-not-exist", temp.path().join("link")).unwrap();
    let error = scan_local_source(temp.path(), None)
        .unwrap_err()
        .to_string();
    assert!(error.contains("链接"));
}
