use std::cell::RefCell;

use super::test_support::*;
use super::*;

#[test]
fn explicit_scans_keep_healthy_results_and_report_each_failed_repository() {
    let healthy = github_fixture("org/healthy", &[("skills/alpha/SKILL.md", ALPHA)]);
    let second = github_fixture("org/second", &[("SKILL.md", BETA)]);
    let calls = RefCell::new(Vec::new());
    let fetch = |url: &str| {
        calls.borrow_mut().push(url.to_owned());
        if url.contains("/org/offline/") {
            return Ok((429, br#"{"message":"API rate limit exceeded"}"#.to_vec()));
        }
        if url.contains("/org/broken/") {
            return Ok((200, b"not JSON".to_vec()));
        }
        if url.contains("/org/healthy/") {
            healthy(url)
        } else {
            second(url)
        }
    };
    let scan = scan_repositories(
        &[
            repository("org/offline", true),
            repository("org/healthy", true),
            repository("org/disabled", false),
            repository("org/broken", true),
            repository("org/second", true),
        ],
        &fetch,
    );
    assert_eq!(scan.repositories.len(), 2);
    assert_eq!(scan.repositories[0].candidates[0].name, "alpha");
    assert_eq!(scan.repositories[1].candidates[0].name, "beta");
    assert_eq!(scan.failures.len(), 2);
    assert_eq!(scan.failures[0].repository_id, "org-offline");
    assert_eq!(scan.failures[0].repo, "org/offline");
    assert!(scan.failures[0].message.contains("HTTP 429"));
    assert!(scan.failures[1].message.contains("GitHub"));
    assert!(!calls.borrow().iter().any(|url| url.contains("disabled")));
}

#[test]
fn scans_honor_each_enabled_repository_subpath_and_ref() {
    let fixture = github_fixture(
        "org/repo",
        &[
            ("skills/alpha/SKILL.md", ALPHA),
            ("plugins/beta/SKILL.md", BETA),
        ],
    );
    let calls = RefCell::new(Vec::new());
    let fetch = |url: &str| {
        calls.borrow_mut().push(url.to_owned());
        fixture(url)
    };
    let mut repo = repository("org/repo", true);
    repo.subpath = "skills".into();
    repo.ref_name = Some("feature/skills".into());
    let scan = scan_repositories(&[repo], &fetch);
    assert!(scan.failures.is_empty());
    assert_eq!(scan.repositories[0].candidates.len(), 1);
    let candidate = &scan.repositories[0].candidates[0];
    assert_eq!(candidate.subpath, "skills/alpha");
    assert_eq!(candidate.ref_name.as_deref(), Some("feature/skills"));
    assert_eq!(candidate.resolved_commit.as_deref(), Some(COMMIT));
    assert!(calls.borrow()[0].ends_with("/commits/feature%2Fskills"));
}

#[test]
fn scanning_an_empty_or_disabled_catalog_never_calls_http() {
    let fetch =
        |_url: &str| -> Result<(u16, Vec<u8>), String> { panic!("unexpected HTTP request") };
    for repositories in [vec![], vec![repository("org/disabled", false)]] {
        let scan = scan_repositories(&repositories, &fetch);
        assert!(scan.repositories.is_empty() && scan.failures.is_empty());
    }
}

#[test]
fn aggregate_candidate_limits_reject_only_the_excess_repository() {
    let files: Vec<_> = (0..257)
        .map(|index| {
            (
                format!("skill-{index}/SKILL.md"),
                format!("---\nname: skill-{index}\ndescription: Fixture\n---\n"),
            )
        })
        .collect();
    let slices: Vec<_> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_bytes()))
        .collect();
    let first = github_fixture("org/first", &slices[..200]);
    let excess = github_fixture("org/excess", &slices[200..]);
    let fetch = |url: &str| {
        if url.contains("/org/first/") {
            first(url)
        } else {
            excess(url)
        }
    };
    let scan = scan_repositories(
        &[
            repository("org/first", true),
            repository("org/excess", true),
        ],
        &fetch,
    );
    assert_eq!(scan.repositories.len(), 1);
    assert_eq!(scan.repositories[0].candidates.len(), 200);
    assert_eq!(scan.failures.len(), 1);
    assert_eq!(scan.failures[0].repo, "org/excess");
    assert!(scan.failures[0].message.contains("256"));
}
