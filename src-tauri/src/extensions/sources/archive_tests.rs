use super::archive::extract_tar_gz_rooted;
use super::tests::{build_archive, gzip, ALPHA};
use super::*;

fn raw_archive(path: &str, kind: tar::EntryType, contents: &[u8]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    let mut header = tar::Header::new_gnu();
    header.set_size(contents.len() as u64);
    header.set_mode(0o644);
    header.set_entry_type(kind);
    header.as_mut_bytes()[..path.len()].copy_from_slice(path.as_bytes());
    header.set_cksum();
    builder.append(&header, contents).unwrap();
    builder.finish().unwrap();
    gzip(&builder.into_inner().unwrap())
}

#[test]
fn extraction_strips_exactly_one_root_and_honors_directory_boundaries() {
    let payload = build_archive(&[
        ("skills/alpha/SKILL.md", ALPHA),
        ("skills/alpha/helper.txt", b"helper"),
        ("skills/alpha-extra/SKILL.md", ALPHA),
    ]);
    let entries = extract_tar_gz_rooted(&payload, "skills/alpha").unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].relative_path, "SKILL.md");
    assert_eq!(entries[1].relative_path, "helper.txt");
}

#[test]
fn unsafe_paths_are_rejected_even_outside_a_requested_subtree() {
    for path in [
        "repo/../escape",
        "repo/../../escape",
        "/repo/absolute",
        "repo/C:/escape",
        "repo/dir\\escape",
        "repo/NUL.txt",
    ] {
        let payload = raw_archive(path, tar::EntryType::Regular, b"outside");
        assert!(
            extract_tar_gz_rooted(&payload, "skills/alpha").is_err(),
            "{path}"
        );
    }
}

#[test]
fn links_and_special_entries_are_never_followed() {
    for kind in [
        tar::EntryType::Symlink,
        tar::EntryType::Link,
        tar::EntryType::Fifo,
    ] {
        let payload = raw_archive("repo/skills/alpha/link", kind, b"");
        let error = extract_tar_gz_rooted(&payload, "skills/alpha").unwrap_err();
        assert!(error.to_string().contains("链接或特殊"));
    }
}

#[test]
fn gzip_truncation_is_detected_even_after_the_tar_end_marker() {
    let mut payload = build_archive(&[("skills/alpha/SKILL.md", ALPHA)]);
    payload.truncate(payload.len() - 8);
    assert!(extract_tar_gz_rooted(&payload, "").is_err());
}

#[test]
fn duplicate_paths_and_mixed_roots_are_rejected() {
    let duplicate = build_archive(&[
        ("skills/alpha/SKILL.md", ALPHA),
        ("skills/alpha/skill.md", ALPHA),
    ]);
    assert!(extract_tar_gz_rooted(&duplicate, "").is_err());
    let mut builder = tar::Builder::new(Vec::new());
    for path in ["repo-one/SKILL.md", "repo-two/helper"] {
        let mut header = tar::Header::new_gnu();
        header.set_size(ALPHA.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append_data(&mut header, path, ALPHA).unwrap();
    }
    builder.finish().unwrap();
    let payload = gzip(&builder.into_inner().unwrap());
    assert!(extract_tar_gz_rooted(&payload, "")
        .unwrap_err()
        .to_string()
        .contains("根目录"));
}

#[test]
fn response_byte_limit_is_enforced_for_injected_transports_too() {
    let fetch = |_url: &str| Ok((200, vec![0; 17]));
    let result = super::transport::fetch_bytes("https://api.github.com/test", 16, &fetch);
    assert!(matches!(result, Err(SourceError::Rejected(_))));
}
