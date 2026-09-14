use std::fs;
use std::io::{Cursor, Write};

use ::zip::write::SimpleFileOptions;
use ::zip::{CompressionMethod, ZipArchive, ZipWriter};

use super::tests::{ALPHA, BETA};
use super::{scan_zip_source, SourceError, MAX_SOURCE_ENTRIES};

fn archive(files: &[(&str, &[u8])], compression: CompressionMethod) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (path, bytes) in files {
        writer
            .start_file(
                *path,
                SimpleFileOptions::default().compression_method(compression),
            )
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn scan(bytes: &[u8]) -> Result<Vec<super::SkillCandidate>, SourceError> {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.zip");
    fs::write(&path, bytes).unwrap();
    let result = scan_zip_source(&path);
    assert_eq!(
        fs::read_dir(temp.path()).unwrap().count(),
        1,
        "scanning must not extract files"
    );
    result
}

#[test]
fn zip_scans_root_and_nested_skills_without_extracting_or_executing_content() {
    for compression in [CompressionMethod::Stored, CompressionMethod::Deflated] {
        let bytes = archive(
            &[
                ("SKILL.md", ALPHA),
                ("wrapper/skills/beta/SKILL.md", BETA),
                ("wrapper/skills/beta/run.sh", b"exit 99\n"),
            ],
            compression,
        );
        let candidates = scan(&bytes).unwrap();
        assert_eq!(candidates.len(), 2);
        assert!(candidates
            .iter()
            .all(|candidate| candidate.source_identity.starts_with("zip:")
                && candidate.ref_name.is_none()
                && candidate.resolved_commit.is_none()));
        let beta = candidates
            .iter()
            .find(|candidate| candidate.name == "beta")
            .unwrap();
        assert_eq!(beta.subpath, "wrapper/skills/beta");
        assert_eq!(beta.entries.len(), 2);
        assert!(beta.diagnostics.is_empty());
        assert_eq!(
            beta.content_digest,
            asb_core::extensions::skill::content_digest(&beta.entries)
        );
    }
}

#[test]
fn zip_candidates_remain_immutable_after_the_archive_changes_or_disappears() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("source.zip");
    fs::write(
        &path,
        archive(&[("alpha/SKILL.md", ALPHA)], CompressionMethod::Stored),
    )
    .unwrap();
    let candidate = scan_zip_source(&path).unwrap().remove(0);
    let canonical = fs::canonicalize(&path).unwrap();
    fs::write(&path, b"replaced after scanning").unwrap();
    assert_eq!(
        candidate.source_identity,
        format!("zip:{}", canonical.display())
    );
    assert_eq!(candidate.entries[0].bytes, ALPHA);
    assert!(scan_zip_source(&path).is_err());
}

#[test]
fn zip_rejects_unsafe_paths_even_when_they_are_outside_the_skill_directory() {
    for path in [
        "../escape",
        "/absolute",
        "C:/escape",
        "dir\\escape",
        "NUL.txt",
        "dir/../escape",
        "dir./bad",
    ] {
        let bytes = archive(
            &[("alpha/SKILL.md", ALPHA), (path, b"outside")],
            CompressionMethod::Stored,
        );
        assert!(scan(&bytes).is_err(), "{path}");
    }
}

#[test]
fn zip_rejects_links_and_special_file_modes() {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .add_symlink("link", "outside", SimpleFileOptions::default())
        .unwrap();
    assert!(scan(&writer.finish().unwrap().into_inner()).is_err());
    let mut bytes = archive(&[("SKILL.md", ALPHA)], CompressionMethod::Stored);
    let central = ZipArchive::new(Cursor::new(&bytes))
        .unwrap()
        .central_directory_start() as usize;
    bytes[central + 5] = 3;
    bytes[central + 38..central + 42].copy_from_slice(&(0o010644u32 << 16).to_le_bytes());
    assert!(scan(&bytes).is_err());
}

#[test]
fn zip_rejects_duplicate_case_conflicting_and_parent_file_paths() {
    let conflicting = archive(
        &[("SKILL.md", ALPHA), ("skill.md", BETA)],
        CompressionMethod::Stored,
    );
    assert!(scan(&conflicting).is_err());
    let parent_file = archive(
        &[
            ("SKILL.md", ALPHA),
            ("folder", b"file"),
            ("folder/child", b"child"),
        ],
        CompressionMethod::Stored,
    );
    assert!(scan(&parent_file).is_err());
    let mut duplicate = archive(
        &[("a/SKILL.md", ALPHA), ("b/SKILL.md", BETA)],
        CompressionMethod::Stored,
    );
    for offset in 0..duplicate.len() - 9 {
        if &duplicate[offset..offset + 10] == b"b/SKILL.md" {
            duplicate[offset] = b'a';
        }
    }
    assert!(scan(&duplicate).is_err());
}

#[test]
fn zip_rejects_truncated_headers_content_and_crc_mismatches() {
    let bytes = archive(&[("SKILL.md", ALPHA)], CompressionMethod::Stored);
    assert!(scan(&bytes[..bytes.len() - 4]).is_err());
    assert!(scan(b"not a ZIP").is_err());
    let mut corrupt = bytes.clone();
    let start = ZipArchive::new(Cursor::new(&bytes))
        .unwrap()
        .by_index(0)
        .unwrap()
        .data_start() as usize;
    corrupt[start] ^= 0xff;
    assert!(scan(&corrupt).is_err());
    let mut truncated_count = bytes;
    let footer = truncated_count.len() - 22;
    truncated_count[footer + 8..footer + 12].fill(0);
    assert!(scan(&truncated_count).is_err());
}

#[test]
fn zip_enforces_input_entry_count_and_expanded_file_limits_before_materialization() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("large.zip");
    fs::File::create(&path)
        .unwrap()
        .set_len(super::transport::MAX_ARCHIVE_DOWNLOAD + 1)
        .unwrap();
    assert!(scan_zip_source(&path).is_err());
    let mut count = archive(&[("SKILL.md", ALPHA)], CompressionMethod::Stored);
    let footer = count.len() - 22;
    for offset in [footer + 8, footer + 10] {
        count[offset..offset + 2].copy_from_slice(&((MAX_SOURCE_ENTRIES + 1) as u16).to_le_bytes());
    }
    assert!(scan(&count).is_err());
    let oversized = vec![0; asb_core::extensions::validate::MAX_CONTENT_FILE_BYTES as usize + 1];
    let bytes = archive(
        &[("SKILL.md", ALPHA), ("large.bin", &oversized)],
        CompressionMethod::Deflated,
    );
    assert!(scan(&bytes).is_err());
}

#[test]
fn zip_manifest_validation_preserves_diagnostics_and_requires_a_manifest_anchor() {
    let missing = archive(&[("README.md", b"not a skill")], CompressionMethod::Stored);
    assert!(scan(&missing).is_err());
    let malformed = archive(
        &[("broken/SKILL.md", b"---\nname: broken\n")],
        CompressionMethod::Stored,
    );
    let candidate = scan(&malformed).unwrap().remove(0);
    assert!(!candidate.diagnostics.is_empty());
    let binary = archive(
        &[("broken/SKILL.md", &[0xff, 0xfe])],
        CompressionMethod::Stored,
    );
    assert!(scan(&binary).unwrap()[0].diagnostics[0].contains("UTF-8"));
}
