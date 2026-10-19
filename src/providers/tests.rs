use super::*;
use crate::{
    state_store::StateStore,
    test_support::{FakeClock, TestServer},
};

fn provider<'a>(
    clock: &'a FakeClock,
    state_path: &'a Path,
    endpoint: &str,
    model: &str,
) -> Groq<'a> {
    Groq {
        http: crate::client().unwrap(),
        token: Some("fixture-key".into()),
        model: model.into(),
        free_tier_confirmed: true,
        endpoint: endpoint.into(),
        state_path,
        clock,
    }
}

fn completion(text: &str) -> String {
    json!({"choices":[{"message":{"content":text}}]}).to_string()
}

fn body(request: &str) -> Value {
    serde_json::from_str(request.split_once("\r\n\r\n").unwrap().1).unwrap()
}

#[test]
fn retries_reserve_each_attempt_and_use_utc_day_at_each_attempt() {
    let clock = FakeClock::at("2026-10-14T23:59:59Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let server = TestServer::start(vec![
        (429, "{}".into()),
        (500, "{}".into()),
        (200, completion("result")),
    ]);
    let provider = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let mut state = State::default();
    let mut calls = 0;
    assert_eq!(
        provider
            .complete(
                Completion {
                    payload: "input",
                    system: "system",
                    max_tokens: 100,
                    schema_name: "test",
                    schema: &json!({})
                },
                &mut state,
                &mut calls
            )
            .unwrap(),
        "result"
    );
    let requests = server.finish();
    assert_eq!(calls, 3);
    assert_eq!(state.llm_daily_requests["2026-10-14"], 1);
    assert_eq!(state.llm_daily_requests["2026-10-15"], 2);
    let stored: State = StateStore::new(&path).load().unwrap();
    assert_eq!(stored.llm_daily_requests, state.llm_daily_requests);
    assert_eq!(
        *clock.sleeps.borrow(),
        [StdDuration::from_secs(1), StdDuration::from_secs(2)]
    );
    for request in requests {
        let body = body(&request);
        assert_eq!(body["response_format"]["type"], "json_schema");
        assert_eq!(body["reasoning_effort"], "low");
    }
}

#[test]
fn json_mode_fallback_is_one_budgeted_request_and_removes_format_only() {
    let clock = FakeClock::at("2026-10-14T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let server = TestServer::start(vec![
        (
            400,
            json!({"error":{"code":"json_validate_failed"}}).to_string(),
        ),
        (200, completion("fallback")),
    ]);
    let provider = provider(&clock, &path, &server.url, "other-model");
    let mut state = State::default();
    let mut calls = 0;
    assert_eq!(
        provider
            .complete(
                Completion {
                    payload: "input",
                    system: "system",
                    max_tokens: 100,
                    schema_name: "test",
                    schema: &json!({})
                },
                &mut state,
                &mut calls
            )
            .unwrap(),
        "fallback"
    );
    let requests = server.finish();
    assert_eq!(body(&requests[0])["response_format"]["type"], "json_object");
    assert!(body(&requests[1]).get("response_format").is_none());
    assert_eq!(
        body(&requests[0])["messages"],
        body(&requests[1])["messages"]
    );
    assert_eq!(calls, 2);
    assert!(clock.sleeps.borrow().is_empty());
}

#[test]
fn caps_and_failed_budget_checkpoint_stop_before_http() {
    let clock = FakeClock::at("2026-10-14T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let provider = provider(&clock, &path, "http://127.0.0.1:1", "other-model");
    let mut state = State::default();
    let mut calls = MAX_LLM_REQUESTS;
    assert!(
        provider
            .complete(
                Completion {
                    payload: "",
                    system: "",
                    max_tokens: 1,
                    schema_name: "x",
                    schema: &json!({})
                },
                &mut state,
                &mut calls
            )
            .unwrap_err()
            .to_string()
            .contains("per-run")
    );
    calls = 0;
    state
        .llm_daily_requests
        .insert("2026-10-14".into(), DAILY_LLM_REQUEST_CAP);
    assert!(
        provider
            .complete(
                Completion {
                    payload: "",
                    system: "",
                    max_tokens: 1,
                    schema_name: "x",
                    schema: &json!({})
                },
                &mut state,
                &mut calls
            )
            .unwrap_err()
            .to_string()
            .contains("per-day")
    );
    assert_eq!(calls, 0);
    state.llm_daily_requests.clear();
    std::fs::create_dir(&path).unwrap();
    let error = provider
        .complete(
            Completion {
                payload: "",
                system: "",
                max_tokens: 1,
                schema_name: "x",
                schema: &json!({}),
            },
            &mut state,
            &mut calls,
        )
        .unwrap_err();
    assert!(
        !error.to_string().contains("request"),
        "must fail at checkpoint, not network: {error}"
    );
    assert!(clock.sleeps.borrow().is_empty());
}

fn ranked(name: &str) -> Ranked {
    Ranked {
        snapshot: crate::Snapshot {
            full_name: name.into(),
            captured_at: "2026-10-14T07:00:00Z".into(),
            stars: Some(6000),
            forks: None,
            language: None,
            description: Some("Documented project description".into()),
            html_url: None,
            topics: vec![],
            created_at: None,
            updated_at: None,
            pushed_at: None,
            license: None,
            open_issues: None,
            default_branch: None,
            sources: vec![],
            readme_excerpt: None,
            observation: None,
        },
        baseline: None,
        delta: 10,
        period: "day".into(),
        trending_source: true,
    }
}

#[test]
fn partial_enrichment_keeps_confirmed_descriptions_and_original_fallback_contract() {
    let clock = FakeClock::at("2026-10-14T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let description =
        "Подтверждённое источником описание проекта и его практического применения. ".repeat(4);
    let response =
        json!({"description":description.trim(), "joke":"Шутка про открытый код и документацию."})
            .to_string();
    let server = TestServer::start(vec![
        (200, completion(&response)),
        (400, "{}".into()),
        (
            200,
            completion(r#"{"overview":"Итог по доступным источникам."}"#),
        ),
    ]);
    let provider = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let result = provider
        .enrich(
            &[ranked("a/one"), ranked("b/two")],
            &mut State::default(),
            true,
            dir.path(),
        )
        .unwrap();
    assert_eq!(server.finish().len(), 3);
    assert_eq!(result.descriptions.len(), 1);
    assert!(result.descriptions.contains_key("a/one"));
    assert!(!result.descriptions.contains_key("b/two"));
    assert!(!result.complete);
    assert_eq!(result.overview, "Итог по доступным источникам.");
    assert_eq!(result.budget["actual_requests"], 3);
    assert_eq!(result.budget["max_requests_per_run"], 40);
    assert_eq!(*clock.sleeps.borrow(), [StdDuration::from_secs(30)]);
    assert!(dir.path().join("llm-bundle.json").exists());
}

#[test]
fn completed_enrichment_preserves_overview_schema_and_artifact_budget() {
    let clock = FakeClock::at("2026-10-14T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let description =
        "Подтверждённое источником описание проекта и его практического применения. ".repeat(4);
    let response =
        json!({"description":description.trim(), "joke":"Шутка про открытый код и документацию."})
            .to_string();
    let server = TestServer::start(vec![
        (200, completion(&response)),
        (
            200,
            completion(r#"{"overview":"Подтверждённый итог выпуска."}"#),
        ),
    ]);
    let provider = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let result = provider
        .enrich(&[ranked("a/one")], &mut State::default(), true, dir.path())
        .unwrap();
    let requests = server.finish();
    assert!(result.complete);
    assert_eq!(result.overview, "Подтверждённый итог выпуска.");
    assert_eq!(result.budget["expected_requests"], 2);
    assert_eq!(result.budget["actual_requests"], 2);
    assert_eq!(
        body(&requests[1])["response_format"]["json_schema"]["name"],
        "issue_overview"
    );
    let artifact: Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("llm-bundle.json")).unwrap())
            .unwrap();
    assert_eq!(artifact["provider_reviews"][0]["id"], "a/one");
    assert_eq!(artifact["budget"], result.budget);
}

#[test]
fn missing_evidence_skips_only_that_project_and_keeps_the_weekly_overview() {
    let clock = FakeClock::at("2026-10-19T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let response = json!({
        "description":"Подтверждённое источниками описание полезного проекта. ".repeat(6),
        "joke":"Шутка про открытую документацию."
    })
    .to_string();
    let server = TestServer::start(vec![
        (200, completion(&response)),
        (200, completion(&response)),
        (
            200,
            completion(r#"{"overview":"Общий итог по двум подтверждённым проектам."}"#),
        ),
    ]);
    let provider = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let mut unknown = ranked("b/unknown");
    unknown.snapshot.description = None;
    unknown.snapshot.readme_excerpt = None;
    let result = provider
        .enrich(
            &[ranked("a/one"), unknown, ranked("c/three")],
            &mut State::default(),
            true,
            dir.path(),
        )
        .unwrap();
    assert_eq!(server.finish().len(), 3);
    assert_eq!(result.descriptions.len(), 2);
    assert!(!result.complete);
    assert_eq!(result.budget["source_gaps"], json!(["b/unknown"]));
    assert_eq!(
        result.overview,
        "Общий итог по двум подтверждённым проектам."
    );
}
