use std::io::Write;

use super::{SkillDirectoryEntry, SkillRepository};

pub(super) const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
pub(super) const ALPHA: &[u8] = b"---\nname: alpha\ndescription: Alpha skill\n---\n";
pub(super) const BETA: &[u8] = b"---\nname: beta\ndescription: Beta skill\n---\n";

pub(super) fn repository(repo: &str, enabled: bool) -> SkillRepository {
    SkillRepository {
        id: repo.replace('/', "-"),
        repo: repo.into(),
        subpath: String::new(),
        ref_name: None,
        enabled,
    }
}

pub(super) fn directory_entry(subpath: &str) -> SkillDirectoryEntry {
    SkillDirectoryEntry {
        id: format!("org/repo/{subpath}"),
        name: "Directory label".into(),
        repo: "org/repo".into(),
        subpath: subpath.into(),
        installs: 1,
        readme_url: Some("https://untrusted.invalid/do-not-fetch".into()),
    }
}

pub(super) fn github_fixture(
    repo: &str,
    files: &[(&str, &[u8])],
) -> impl Fn(&str) -> Result<(u16, Vec<u8>), String> {
    let tree: Vec<_> = files
        .iter()
        .map(|(path, bytes)| {
            serde_json::json!({
                "path": path, "type": "blob", "mode": "100644", "size": bytes.len(),
            })
        })
        .collect();
    let tree =
        serde_json::to_vec(&serde_json::json!({ "truncated": false, "tree": tree })).unwrap();
    let archive = build_archive(files);
    let repo = repo.to_owned();
    move |url| {
        if url.starts_with(&format!("https://api.github.com/repos/{repo}/commits/")) {
            Ok((
                200,
                serde_json::to_vec(&serde_json::json!({"sha": COMMIT})).unwrap(),
            ))
        } else if url
            == format!("https://api.github.com/repos/{repo}/git/trees/{COMMIT}?recursive=1")
        {
            Ok((200, tree.clone()))
        } else if url == format!("https://codeload.github.com/{repo}/tar.gz/{COMMIT}") {
            Ok((200, archive.clone()))
        } else {
            panic!("Unexpected fixture HTTP request: {url}")
        }
    }
}

fn build_archive(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for (path, bytes) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder
            .append_data(&mut header, format!("repo-{COMMIT}/{path}"), *bytes)
            .unwrap();
    }
    let raw = builder.into_inner().unwrap();
    let mut gzip = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    gzip.write_all(&raw).unwrap();
    gzip.finish().unwrap()
}
