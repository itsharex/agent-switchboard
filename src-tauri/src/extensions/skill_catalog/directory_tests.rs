use std::cell::RefCell;

use super::test_support::*;
use super::*;

fn response(count: usize, skills: serde_json::Value) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({ "query": "example", "count": count, "skills": skills }))
        .unwrap()
}

fn item(id: &str, source: &str, skill_id: &str) -> serde_json::Value {
    serde_json::json!({"id": id, "name": "Example", "source": source, "skillId": skill_id, "installs": 25})
}

#[test]
fn searches_only_the_explicit_page_with_encoded_parameters() {
    let calls = RefCell::new(Vec::new());
    let fetch = |url: &str| {
        calls.borrow_mut().push(reqwest::Url::parse(url).unwrap());
        Ok((
            200,
            response(
                45,
                serde_json::json!([item("one", "Owner/repo", "example")]),
            ),
        ))
    };
    let result = search_directory("  git & rust  ", 20, &fetch).unwrap();
    assert_eq!(result.total, 45);
    assert!(result.has_more);
    assert_eq!(result.items[0].repo, "owner/repo");
    assert_eq!(result.items[0].subpath, "example");
    assert_eq!(
        result.items[0].readme_url.as_deref(),
        Some("https://github.com/owner/repo")
    );
    let calls = calls.borrow();
    assert_eq!(calls.len(), 1);
    let url = &calls[0];
    assert_eq!(url.host_str(), Some("skills.sh"));
    assert_eq!(url.path(), "/api/search");
    assert_eq!(
        url.query_pairs().into_owned().collect::<Vec<_>>(),
        [
            ("q".into(), "git & rust".into()),
            ("limit".into(), "20".into()),
            ("offset".into(), "20".into()),
        ]
    );
}

#[test]
fn invalid_queries_and_offsets_never_call_http() {
    let fetch =
        |_url: &str| -> Result<(u16, Vec<u8>), String> { panic!("unexpected HTTP request") };
    for query in ["", "a", " \t ", "bad\nquery", &"x".repeat(201)] {
        assert!(search_directory(query, 0, &fetch).is_err());
    }
    for offset in [1, 21, MAX_DIRECTORY_RESULTS, usize::MAX] {
        assert!(search_directory("valid", offset, &fetch).is_err());
    }
    let mut entry = directory_entry("../outside");
    assert!(resolve_directory_skill(&entry, &fetch).is_err());
    entry.subpath = "example".into();
    entry.repo = "https://token@github.com/org/repo".into();
    assert!(resolve_directory_skill(&entry, &fetch).is_err());
}

#[test]
fn directory_pagination_stops_at_the_bound_or_an_empty_provider_page() {
    let fetch = |_url: &str| {
        Ok((
            200,
            response(5000, serde_json::json!([item("one", "owner/repo", "one")])),
        ))
    };
    let last = search_directory("example", 980, &fetch).unwrap();
    assert_eq!(last.total, MAX_DIRECTORY_RESULTS);
    assert!(!last.has_more);
    let empty = |_url: &str| Ok((200, response(500, serde_json::json!([]))));
    assert!(!search_directory("example", 20, &empty).unwrap().has_more);
}

#[test]
fn directory_filters_unsafe_coordinates_and_deduplicates_without_hiding_later_pages() {
    let fetch = |_url: &str| {
        Ok((
            200,
            response(
                42,
                serde_json::json!([
                    item("good", "owner/repo.with.dots", "skills/good"),
                    item("good", "owner/repo", "good"),
                    item("external", "skills.volces.com/repo", "one"),
                    item("deep", "owner/repo/extra", "one"),
                    item("unsafe", "owner/repo", "../outside"),
                    item("absolute", "owner/repo", "C:/bad"),
                ]),
            ),
        ))
    };
    let result = search_directory("example", 0, &fetch).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].id, "good");
    assert!(result.has_more);
}

#[test]
fn response_errors_and_limits_are_actionable_and_never_silently_truncated() {
    let limited = |_url: &str| Ok((429, br#"{"message":"Rate limit exceeded"}"#.to_vec()));
    let error = search_directory("example", 0, &limited)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("skills.sh")
            && error.contains("HTTP 429")
            && error.contains("Rate limit exceeded")
    );
    let malformed = |_url: &str| Ok((200, b"not json".to_vec()));
    assert!(search_directory("example", 0, &malformed)
        .unwrap_err()
        .to_string()
        .contains("parsed"));
    let too_large = |_url: &str| {
        Ok((
            200,
            vec![0; crate::extensions::sources::MAX_DIRECTORY_DOWNLOAD as usize + 1],
        ))
    };
    assert!(search_directory("example", 0, &too_large).is_err());
    let too_many = |_url: &str| {
        Ok((
            200,
            response(
                21,
                serde_json::Value::Array(
                    (0..21)
                        .map(|index| item(&index.to_string(), "owner/repo", "one"))
                        .collect(),
                ),
            ),
        ))
    };
    assert!(search_directory("example", 0, &too_many).is_err());
}

#[test]
fn directory_slugs_resolve_nested_manifests_at_one_immutable_commit() {
    let fetch = github_fixture(
        "org/repo",
        &[
            ("plugins/tools/alpha/SKILL.md", ALPHA),
            ("other/SKILL.md", BETA),
        ],
    );
    let candidates = resolve_directory_skill(&directory_entry("alpha"), &fetch).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].subpath, "plugins/tools/alpha");
    assert_eq!(candidates[0].source_identity, "org/repo");
    assert_eq!(candidates[0].resolved_commit.as_deref(), Some(COMMIT));
    assert!(candidates[0].diagnostics.is_empty());
}

#[test]
fn explicit_paths_win_and_ambiguous_slug_matches_require_a_subpath() {
    let fetch = github_fixture(
        "org/repo",
        &[
            ("skills/alpha/SKILL.md", ALPHA),
            ("plugins/alpha/SKILL.md", BETA),
        ],
    );
    let exact = resolve_directory_skill(&directory_entry("skills/alpha"), &fetch).unwrap();
    assert_eq!(exact[0].subpath, "skills/alpha");
    assert!(resolve_directory_skill(&directory_entry("alpha"), &fetch)
        .unwrap_err()
        .to_string()
        .contains("Several manifest paths"));
}

#[test]
fn a_root_skill_resolves_but_a_wrapper_or_missing_skill_is_not_installed() {
    let root = github_fixture("org/repo", &[("SKILL.md", ALPHA)]);
    assert!(
        resolve_directory_skill(&directory_entry("root-skill-slug"), &root).unwrap()[0]
            .subpath
            .is_empty()
    );
    let wrapper = github_fixture("org/repo", &[("alpha/readme.md", b"not a manifest")]);
    assert!(resolve_directory_skill(&directory_entry("alpha"), &wrapper).is_err());
    let unrelated = github_fixture("org/repo", &[("beta/SKILL.md", BETA)]);
    assert!(resolve_directory_skill(&directory_entry("missing"), &unrelated).is_err());
}
