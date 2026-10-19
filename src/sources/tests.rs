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
        ("https://api.github.com/repos/a/covered".into(), br#"{"stargazers_count":9000}"#.to_vec()),
        ("https://api.github.com/repos/b/small".into(), br#"{"stargazers_count":0,"default_branch":"main","description":"Young documented project"}"#.to_vec()),
        ("https://raw.githubusercontent.com/b/small/main/README.md".into(), b"# Young project".to_vec()),
        ("https://api.github.com/repos/c/fresh".into(), br#"{"stargazers_count":6000,"default_branch":"main","description":"Useful project"}"#.to_vec()),
        ("https://raw.githubusercontent.com/c/fresh/main/README.md".into(), b"# Fresh\nDocumented implementation".to_vec()),
    ]), requests: RefCell::new(vec![]) };
    let mut run = collect(clock.now(), dir.path(), &fixtures, &clock).unwrap();
    assert_eq!(run.snapshots.len(), 3);
    assert_eq!(run.snapshots[2].full_name, "c/fresh");
    assert_eq!(run.snapshots[2].sources.len(), 2);
    assert!(
        run.snapshots
            .iter()
            .all(|snapshot| snapshot.readme_excerpt.is_none())
    );
    assert_eq!(fixtures.requests.borrow().len(), 6);
    enrich_readmes(&mut run.snapshots[1..], &run.dir, &fixtures, &clock).unwrap();
    assert_eq!(
        run.snapshots[2].readme_excerpt.as_deref(),
        Some("# Fresh\nDocumented implementation")
    );
    assert_eq!(run.manifest["candidate_count"], 3);
    assert_eq!(
        run.manifest["excluded_candidates"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        run.manifest["sources"][0]["fetched_at"],
        "2026-10-14T07:00:00Z"
    );
    assert_eq!(run.manifest["sources"][2]["status"], "included");
    assert!(run.dir.join("repo-c_fresh.json").exists());
    assert_eq!(fixtures.requests.borrow().len(), 8);
    assert_eq!(*clock.sleeps.borrow(), [StdDuration::from_millis(100); 3]);
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
    let run = collect(clock.now(), dir.path(), &fixtures, &clock).unwrap();
    assert_eq!(run.snapshots.len(), 1);
    assert_eq!(run.snapshots[0].full_name, "c/fresh");
    assert!(run.dir.join("candidate-observations.json").exists());
    assert_eq!(run.manifest["sources"][2]["status"], "error");
    assert_eq!(run.manifest["api_repositories"][0]["status"], "excluded");
    assert_eq!(fixtures.requests.borrow().len(), 3);
}

#[test]
fn all_candidates_beyond_thirty_survive_missing_metadata_without_readme_requests() {
    let clock = FakeClock::at("2026-10-14T07:00:00Z");
    let directory = tempfile::tempdir().unwrap();
    let names = (0..45)
        .map(|index| format!("org/p{index:02}"))
        .collect::<Vec<_>>();
    let rows = names
        .iter()
        .map(|name| (name.as_str(), 0))
        .collect::<Vec<_>>();
    let fixtures = Fixtures { replies:BTreeMap::from([
        ("https://github.com/trending?since=weekly".into(), trending(&rows)),
        ("https://github.com/trending?since=daily".into(), trending(&rows)),
        ("https://api.github.com/search/repositories?q=created%3A%3E%3D2026-10-07&sort=stars&order=desc&per_page=30".into(), br#"{"items":[],"total_count":0}"#.to_vec())
    ]), requests:RefCell::new(vec![]) };
    let run = collect(clock.now(), directory.path(), &fixtures, &clock).unwrap();
    assert_eq!(run.snapshots.len(), 45);
    assert_eq!(run.manifest["candidate_count"], 45);
    assert_eq!(run.manifest["excluded_candidates"], json!([]));
    assert_eq!(
        run.manifest["api_repositories"].as_array().unwrap().len(),
        45
    );
    assert_eq!(fixtures.requests.borrow().len(), 4); // three feeds, first metadata rejected
    assert!(run.snapshots.iter().all(|row| row.sources.len() == 2));
    let persisted: Vec<Snapshot> =
        serde_json::from_slice(&fs::read(run.dir.join("candidate-observations.json")).unwrap())
            .unwrap();
    assert_eq!(persisted.len(), 45);
}

#[test]
fn collection_fetches_readme_evidence_after_the_fifteenth_project() {
    let clock = FakeClock::at("2026-10-19T07:00:00Z");
    let directory = tempfile::tempdir().unwrap();
    let names = (0..16)
        .map(|index| format!("org/p{index:02}"))
        .collect::<Vec<_>>();
    let rows = names
        .iter()
        .map(|name| (name.as_str(), 20))
        .collect::<Vec<_>>();
    let mut replies = BTreeMap::from([
        ("https://github.com/trending?since=weekly".into(), trending(&rows)),
        ("https://github.com/trending?since=daily".into(), trending(&rows)),
        ("https://api.github.com/search/repositories?q=created%3A%3E%3D2026-10-12&sort=stars&order=desc&per_page=30".into(),
            br#"{"items":[],"total_count":0}"#.to_vec()),
    ]);
    for name in &names {
        replies.insert(
            format!("https://api.github.com/repos/{name}"),
            br#"{"default_branch":"main","description":null,"stargazers_count":20}"#.to_vec(),
        );
        replies.insert(
            format!("https://raw.githubusercontent.com/{name}/main/README.md"),
            b"# Project\n\nDocumented implementation with useful source evidence.".to_vec(),
        );
    }
    let fixtures = Fixtures {
        replies,
        requests: RefCell::new(vec![]),
    };
    let mut result = collect(clock.now(), directory.path(), &fixtures, &clock).unwrap();
    assert_eq!(result.snapshots.len(), 16);
    assert_eq!(fixtures.requests.borrow().len(), 19);
    assert!(
        result
            .snapshots
            .iter()
            .all(|snapshot| snapshot.readme_excerpt.is_none())
    );
    enrich_readmes(&mut result.snapshots, &result.dir, &fixtures, &clock).unwrap();
    assert!(
        result
            .snapshots
            .iter()
            .all(|snapshot| snapshot.readme_excerpt.is_some())
    );
    assert_eq!(fixtures.requests.borrow().len(), 35);
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
