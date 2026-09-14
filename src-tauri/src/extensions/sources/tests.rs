use std::cell::RefCell;
use std::io::Write;

use super::*;

pub(super) const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
pub(super) const ALPHA: &[u8] = b"---\nname: alpha\ndescription: Alpha skill\n---\n";
pub(super) const BETA: &[u8] = b"---\nname: beta\ndescription: Beta skill\n---\n";

pub(super) fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(bytes).unwrap();
    encoder.finish().unwrap()
}

pub(super) fn build_archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buffer = Vec::new();
    {
        let mut builder = tar::Builder::new(&mut buffer);
        for (path, bytes) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(&mut header, format!("repo-{COMMIT}/{path}"), *bytes)
                .unwrap();
        }
        builder.finish().unwrap();
    }
    gzip(&buffer)
}

fn tree(files: &[(&str, &[u8])]) -> Vec<u8> {
    let entries: Vec<_> = files
        .iter()
        .map(|(path, bytes)| {
            serde_json::json!({
                "path": path, "type": "blob", "mode": "100644", "size": bytes.len(),
            })
        })
        .collect();
    serde_json::to_vec(&serde_json::json!({ "truncated": false, "tree": entries })).unwrap()
}

fn mock_fetch<'a>(
    tree: &'a [u8],
    payload: &'a [u8],
    calls: &'a RefCell<Vec<String>>,
) -> impl Fn(&str) -> Result<(u16, Vec<u8>), String> + 'a {
    move |url| {
        calls.borrow_mut().push(url.to_string());
        if url.starts_with("https://api.github.com/repos/org/repo/commits/") {
            return Ok((
                200,
                serde_json::to_vec(&serde_json::json!({ "sha": COMMIT })).unwrap(),
            ));
        }
        if url == format!("https://api.github.com/repos/org/repo/git/trees/{COMMIT}?recursive=1") {
            return Ok((200, tree.to_vec()));
        }
        if url == format!("https://codeload.github.com/org/repo/tar.gz/{COMMIT}") {
            return Ok((200, payload.to_vec()));
        }
        Err(format!("Unexpected mock request: {url}"))
    }
}

#[test]
fn repository_scan_returns_every_skill_at_one_fixed_commit() {
    let files: &[(&str, &[u8])] = &[
        ("skills/alpha/SKILL.md", ALPHA),
        ("skills/alpha/reference.md", b"reference"),
        ("plugins/tools/beta/SKILL.md", BETA),
        ("README.md", b"not a skill"),
    ];
    let tree = tree(files);
    let payload = build_archive(files);
    let calls = RefCell::new(Vec::new());
    let fetch = mock_fetch(&tree, &payload, &calls);
    let candidates = resolve_github_source(
        "https://github.com/ORG/REPO.git/",
        "",
        Some("feature/skills"),
        &fetch,
    )
    .unwrap();
    assert_eq!(candidates.len(), 2);
    assert_eq!(
        candidates
            .iter()
            .map(|item| item.name.as_str())
            .collect::<Vec<_>>(),
        vec!["beta", "alpha"]
    );
    assert!(candidates
        .iter()
        .all(|item| item.source_identity == "org/repo"
            && item.resolved_commit.as_deref() == Some(COMMIT)));
    assert!(candidates
        .iter()
        .all(|item| item.ref_name.as_deref() == Some("feature/skills")));
    let alpha = candidates.iter().find(|item| item.name == "alpha").unwrap();
    assert_eq!(alpha.subpath, "skills/alpha");
    assert_eq!(alpha.entries.len(), 2);
    assert_eq!(alpha.content_digest, content_digest(&alpha.entries));
    assert_eq!(calls.borrow().len(), 3);
    assert!(calls.borrow()[0].ends_with("/commits/feature%2Fskills"));
    assert!(calls.borrow()[1].contains(COMMIT));
    assert!(calls.borrow()[2].contains(COMMIT));
}

#[test]
fn subtree_can_be_a_collection_or_one_skill_and_root_skills_work() {
    let files: &[(&str, &[u8])] = &[("SKILL.md", BETA), ("skills/alpha/SKILL.md", ALPHA)];
    let tree = tree(files);
    let payload = build_archive(files);
    let calls = RefCell::new(Vec::new());
    let fetch = mock_fetch(&tree, &payload, &calls);
    for prefix in ["skills", "skills/alpha"] {
        let candidates = resolve_github_source("org/repo", prefix, None, &fetch).unwrap();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].subpath, "skills/alpha");
        assert_eq!(candidates[0].name, "alpha");
    }
    let all = resolve_github_source("org/repo", "", None, &fetch).unwrap();
    assert_eq!(all.len(), 2);
    assert!(all
        .iter()
        .any(|item| item.subpath.is_empty() && item.name == "beta"));
}

#[test]
fn malformed_manifests_remain_visible_with_actionable_diagnostics() {
    let files: &[(&str, &[u8])] = &[
        ("good/SKILL.md", ALPHA),
        ("broken/SKILL.md", b"---\nname: invalid\n"),
        ("missing/SKILL.md", b"# No frontmatter"),
        ("binary/SKILL.md", &[0xff, 0xfe]),
    ];
    let tree = tree(files);
    let payload = build_archive(files);
    let calls = RefCell::new(Vec::new());
    let candidates =
        resolve_github_source("org/repo", "", None, &mock_fetch(&tree, &payload, &calls)).unwrap();
    assert_eq!(candidates.len(), 4);
    assert!(candidates
        .iter()
        .find(|item| item.name == "alpha")
        .unwrap()
        .diagnostics
        .is_empty());
    assert!(candidates
        .iter()
        .find(|item| item.name == "broken")
        .unwrap()
        .diagnostics[0]
        .contains("未闭合"));
    assert!(candidates
        .iter()
        .find(|item| item.name == "missing")
        .unwrap()
        .diagnostics[0]
        .contains("frontmatter"));
    assert!(candidates
        .iter()
        .find(|item| item.name == "binary")
        .unwrap()
        .diagnostics[0]
        .contains("UTF-8"));
}

#[test]
fn empty_repositories_do_not_download_an_archive() {
    let files: &[(&str, &[u8])] = &[("README.md", b"empty")];
    let tree = tree(files);
    let calls = RefCell::new(Vec::new());
    let candidates =
        resolve_github_source("org/repo", "", None, &mock_fetch(&tree, &[], &calls)).unwrap();
    assert!(candidates.is_empty());
    assert_eq!(calls.borrow().len(), 2);
}

#[test]
fn truncated_or_unsafe_trees_are_rejected_before_downloading() {
    let unsafe_trees = [
        serde_json::json!({ "truncated": true, "tree": [] }),
        serde_json::json!({ "truncated": false, "tree": [{ "path": "../SKILL.md", "type": "blob", "mode": "100644", "size": 1 }] }),
        serde_json::json!({ "truncated": false, "tree": [{ "path": "link", "type": "blob", "mode": "120000", "size": 1 }] }),
        serde_json::json!({ "truncated": false, "tree": [{ "path": "module", "type": "commit", "mode": "160000" }] }),
    ];
    for tree in unsafe_trees {
        let bytes = serde_json::to_vec(&tree).unwrap();
        let calls = RefCell::new(Vec::new());
        assert!(
            resolve_github_source("org/repo", "", None, &mock_fetch(&bytes, &[], &calls)).is_err()
        );
        assert_eq!(calls.borrow().len(), 2);
    }
}

#[test]
fn missing_archive_files_and_duplicate_content_do_not_silently_drop_candidates() {
    let files: &[(&str, &[u8])] = &[("alpha/SKILL.md", ALPHA), ("beta/SKILL.md", BETA)];
    let tree = tree(files);
    let payload = build_archive(&files[..1]);
    let calls = RefCell::new(Vec::new());
    let error = resolve_github_source("org/repo", "", None, &mock_fetch(&tree, &payload, &calls))
        .unwrap_err();
    assert!(error.to_string().contains("不一致"));
    let duplicates: &[(&str, &[u8])] = &[("one/SKILL.md", ALPHA), ("two/SKILL.md", ALPHA)];
    let tree = self::tree(duplicates);
    let payload = build_archive(duplicates);
    let error = resolve_github_source("org/repo", "", None, &mock_fetch(&tree, &payload, &calls))
        .unwrap_err();
    assert!(error.to_string().contains("内容完全相同"));
}

#[test]
fn excessive_candidate_counts_are_rejected_before_archive_download() {
    let entries: Vec<_> = (0..=MAX_SOURCE_CANDIDATES).map(|index| serde_json::json!({
        "path": format!("skill-{index}/SKILL.md"), "type": "blob", "mode": "100644", "size": 1,
    })).collect();
    let tree =
        serde_json::to_vec(&serde_json::json!({ "truncated": false, "tree": entries })).unwrap();
    let calls = RefCell::new(Vec::new());
    let error =
        resolve_github_source("org/repo", "", None, &mock_fetch(&tree, &[], &calls)).unwrap_err();
    assert!(error.to_string().contains("256"));
    assert_eq!(calls.borrow().len(), 2);
}

#[test]
fn invalid_inputs_never_reach_the_http_boundary() {
    let fetch =
        |_url: &str| -> Result<(u16, Vec<u8>), String> { panic!("unexpected network call") };
    for repo in [
        "org",
        "org/repo/tree/main",
        "https://evil.invalid/org/repo",
        "https://token@github.com/org/repo",
        "org/../repo",
    ] {
        assert!(resolve_github_source(repo, "", None, &fetch).is_err());
    }
    assert!(resolve_github_source("org/repo", "../outside", None, &fetch).is_err());
    assert!(resolve_github_source("org/repo", "", Some("bad\nref"), &fetch).is_err());
}

#[test]
fn transport_and_response_errors_keep_their_reason() {
    let limit = |_url: &str| Ok((429, br#"{"message":"API rate limit exceeded"}"#.to_vec()));
    let error = resolve_github_commit("org/repo", "HEAD", &limit)
        .unwrap_err()
        .to_string();
    assert!(error.contains("HTTP 429") && error.contains("API rate limit exceeded"));
    let bad_json = |_url: &str| Ok((200, b"not JSON".to_vec()));
    assert!(resolve_github_commit("org/repo", "HEAD", &bad_json)
        .unwrap_err()
        .to_string()
        .contains("无法解析"));
    let bad_sha = |_url: &str| Ok((200, br#"{"sha":"../outside"}"#.to_vec()));
    assert!(resolve_github_commit("org/repo", "HEAD", &bad_sha).is_err());
    let offline = |_url: &str| Err("DNS lookup failed".to_string());
    assert!(resolve_github_commit("org/repo", "HEAD", &offline)
        .unwrap_err()
        .to_string()
        .contains("DNS lookup failed"));
}
