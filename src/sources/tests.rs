use super::*;
use crate::test_support::{FakeClock, TestServer};
use std::cell::RefCell;

struct Fixtures {
    replies: BTreeMap<String, Vec<u8>>,
    requests: RefCell<Vec<String>>,
}

impl SourceHttp for Fixtures {
    fn get(&self, url: &str, _raw: bool, _max_bytes: Option<usize>) -> Result<Vec<u8>> {
        self.requests.borrow_mut().push(url.to_owned());
        self.replies
            .get(url)
            .cloned()
            .ok_or_else(|| anyhow!("HTTP 403 fixture denied"))
    }
}

fn trending(names: &[(&str, i64)]) -> Vec<u8> {
    names.iter().map(|(name, stars)| format!("<article class='Box-row'><h2><a href='/{name}'>{name}</a></h2><a href='/{name}/stargazers'>{stars}</a><span class='d-inline-block float-sm-right'>123 stars this week</span></article>")).collect::<String>().into_bytes()
}

#[test]
fn collection_keeps_young_projects_and_source_provenance_without_external_http() {
    let clock = FakeClock::at("2026-10-14T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let fixtures = Fixtures { replies: BTreeMap::from([
        ("https://github.com/trending?since=weekly".into(), trending(&[("a/covered", 9000), ("b/small", 0), ("c/fresh", 6000)])),
        ("https://github.com/trending?since=daily".into(), trending(&[("c/fresh", 6000)])),
        ("https://api.github.com/search/repositories?q=created%3A%3E%3D2026-10-07&sort=stars&order=desc&per_page=30".into(), br#"{"items":[],"total_count":0}"#.to_vec()),
        ("https://api.github.com/repos/b/small".into(), br#"{"stargazers_count":0,"default_branch":"main","description":"Young documented project"}"#.to_vec()),
        ("https://raw.githubusercontent.com/b/small/main/README.md".into(), b"# Young project".to_vec()),
        ("https://api.github.com/repos/c/fresh".into(), br#"{"stargazers_count":6000,"default_branch":"main","description":"Useful project"}"#.to_vec()),
        ("https://raw.githubusercontent.com/c/fresh/main/README.md".into(), b"# Fresh\nDocumented implementation".to_vec()),
    ]), requests: RefCell::new(vec![]) };
    let covered = BTreeMap::from([("a/covered".into(), "2026-10-13".into())]);
    let run = collect(clock.now(), dir.path(), &covered, &fixtures, &clock).unwrap();
    assert_eq!(run.snapshots.len(), 2);
    assert_eq!(run.snapshots[1].full_name, "c/fresh");
    assert_eq!(run.snapshots[1].sources.len(), 2);
    assert_eq!(
        run.snapshots[1].readme_excerpt.as_deref(),
        Some("# Fresh\nDocumented implementation")
    );
    assert_eq!(run.manifest["candidate_count"], 2);
    assert_eq!(
        run.manifest["excluded_candidates"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        run.manifest["sources"][0]["fetched_at"],
        "2026-10-14T07:00:00Z"
    );
    assert_eq!(run.manifest["sources"][2]["status"], "included");
    assert!(run.dir.join("repo-c_fresh.json").exists());
    assert_eq!(fixtures.requests.borrow().len(), 7);
    assert_eq!(*clock.sleeps.borrow(), [StdDuration::from_millis(100); 2]);
}

#[test]
fn rate_limit_remains_an_explicit_coverage_gap_and_skips_metadata() {
    let clock = FakeClock::at("2026-10-14T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let fixtures = Fixtures {
        replies: BTreeMap::from([
            (
                "https://github.com/trending?since=weekly".into(),
                trending(&[("c/fresh", 6000)]),
            ),
            (
                "https://github.com/trending?since=daily".into(),
                trending(&[("c/fresh", 6000)]),
            ),
        ]),
        requests: RefCell::new(vec![]),
    };
    let run = collect(clock.now(), dir.path(), &BTreeMap::new(), &fixtures, &clock).unwrap();
    assert!(run.snapshots.is_empty());
    assert_eq!(run.manifest["sources"][2]["status"], "error");
    assert_eq!(run.manifest["api_repositories"][0]["status"], "excluded");
    assert_eq!(fixtures.requests.borrow().len(), 3);
}

#[test]
fn http_retries_server_errors_but_raw_requests_never_carry_github_token() {
    let clock = FakeClock::at("2026-10-14T07:00:00Z");
    let server = TestServer::start(vec![(500, "temporary".into()), (200, "abcdef".into())]);
    let http = GitHubHttp {
        client: crate::client().unwrap(),
        token: Some("fixture-token".into()),
        clock: &clock,
    };
    assert_eq!(http.get(&server.url, true, Some(3)).unwrap(), b"abc");
    let requests = server.finish();
    assert!(
        requests
            .iter()
            .all(|request| !request.to_ascii_lowercase().contains("authorization:"))
    );
    assert_eq!(*clock.sleeps.borrow(), [StdDuration::from_secs(1)]);
    let server = TestServer::start(vec![(429, "limited".into())]);
    assert!(
        http.get(&server.url, false, None)
            .unwrap_err()
            .to_string()
            .contains("429")
    );
    assert!(
        server.finish()[0]
            .to_ascii_lowercase()
            .contains("authorization: bearer fixture-token")
    );
    assert_eq!(clock.sleeps.borrow().len(), 1);
}
