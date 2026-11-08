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

const README_EVIDENCE: &str =
    "This tool parses source files locally and reports their dependency graph.";

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
        [StdDuration::from_secs(60), StdDuration::from_secs(2)]
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
            readme_excerpt: Some(format!("# Project\n\n{README_EVIDENCE}")),
            observation: None,
        },
        baseline: None,
        delta: 10,
        period: "day".into(),
        trending_source: true,
    }
}

#[test]
fn project_cards_accept_empty_humour_and_bound_unicode_without_truncation() {
    let description = "Локально строит граф зависимостей исходных файлов.";
    let parsed =
        parse_project_description(&json!({"description":description,"joke":" \n\t "}).to_string())
            .unwrap();
    assert_eq!(parsed, (description.into(), String::new()));
    // The prompt requests at most 1500 characters; modest overshoot is accepted.
    // In particular, the two production responses that blocked quiche are valid.
    for length in [1500, 1501, 1601, 1659, 1668, 2000] {
        let description = "я".repeat(length);
        assert_eq!(
            parse_project_description(&json!({"description":description,"joke":""}).to_string())
                .unwrap()
                .0,
            description
        );
    }
    let boundary = "я".repeat(2000);
    for invalid in ["Коротко".to_string(), format!("{boundary}я")] {
        assert!(
            parse_project_description(&json!({"description":invalid,"joke":""}).to_string())
                .is_err()
        );
    }
    for invalid in [
        json!({"description":description}),
        json!({"description":description,"joke":null}),
        json!({"description":description,"joke":"","metadata":"extra"}),
    ] {
        assert!(parse_project_description(&invalid.to_string()).is_err());
    }
    // Optional overlong humour does not reject or shorten a useful card.
    let parsed = parse_project_description(
        &json!({"description":description,"joke":"я".repeat(MAX_JOKE_CHARS + 1)}).to_string(),
    )
    .unwrap();
    assert_eq!(parsed, (description.into(), String::new()));
}

#[test]
fn project_descriptions_preserve_paragraphs_and_normalize_line_wrapping() {
    let raw = " \r\n  Локально  строит граф\r\nзависимостей исходных файлов.\r\n \t \r\n\r\nПоказывает связи  между модулями.\n\t\nПомогает оценить\n  объём изменений.  \n\n";
    let expected = "Локально строит граф зависимостей исходных файлов.\n\nПоказывает связи между модулями.\n\nПомогает оценить объём изменений.";
    let parsed =
        parse_project_description(&json!({"description":raw,"joke":""}).to_string()).unwrap();
    assert_eq!(parsed, (expected.into(), String::new()));
}

#[test]
fn partial_enrichment_keeps_confirmed_descriptions_and_original_fallback_contract() {
    let clock = FakeClock::at("2026-10-14T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let description = "Локально разбирает исходные файлы и показывает граф их зависимостей.";
    let response =
        json!({"description":description,"joke":"","evidence_quote":README_EVIDENCE}).to_string();
    let server = TestServer::start(vec![(200, completion(&response)), (400, "{}".into())]);
    let provider = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let result = provider
        .enrich(
            &[ranked("a/one"), ranked("b/two")],
            &mut State::default(),
            true,
            dir.path(),
        )
        .unwrap();
    assert_eq!(server.finish().len(), 2);
    assert_eq!(result.descriptions.len(), 1);
    assert_eq!(result.descriptions["a/one"], description);
    assert!(!result.descriptions.contains_key("b/two"));
    assert!(!result.complete);
    assert_eq!(result.budget["expected_requests"], 2);
    assert_eq!(result.budget["actual_requests"], 2);
    assert_eq!(result.budget["max_requests_per_run"], 40);
    assert_eq!(*clock.sleeps.borrow(), [StdDuration::from_secs(30)]);
    assert!(dir.path().join("llm-bundle.json").exists());
}

#[test]
fn generation_and_repair_request_1500_but_accept_up_to_2000_characters() {
    for (length, repair) in [(1659, false), (1668, false), (2000, false), (2000, true)] {
        let clock = FakeClock::at("2026-11-08T15:00:00Z");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let description = "я".repeat(length);
        let response = json!({
            "description":description,"joke":"","evidence_quote":README_EVIDENCE
        })
        .to_string();
        let mut responses = Vec::new();
        if repair {
            responses.push((200, completion("not JSON")));
        }
        responses.push((200, completion(&response)));
        let server = TestServer::start(responses);
        let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
        let result = groq
            .enrich_daily(&[ranked("a/one")], &mut State::default(), dir.path(), &[])
            .unwrap();
        let requests = server.finish();
        assert_eq!(requests.len(), if repair { 2 } else { 1 });
        for request in requests {
            assert!(
                body(&request)["messages"][0]["content"]
                    .as_str()
                    .unwrap()
                    .contains("максимум 1500")
            );
        }
        assert_eq!(result.descriptions["a/one"], description);
        assert!(result.complete);
        assert_eq!(result.budget["card_failures"], json!([]));
    }
}

#[test]
fn completed_enrichment_uses_one_request_per_card_and_preserves_primary_evidence() {
    let clock = FakeClock::at("2026-10-14T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let description = "Локально разбирает исходные файлы и показывает граф их зависимостей.";
    let response =
        json!({"description":description,"joke":"","evidence_quote":README_EVIDENCE}).to_string();
    let server = TestServer::start(vec![(200, completion(&response))]);
    let provider = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let mut entry = ranked("a/one");
    let primary_readme = format!(
        "# Project\n\n{README_EVIDENCE}\n\n{}\n\nThe backend is not included.",
        "Source context. ".repeat(100)
    );
    entry.snapshot.readme_excerpt = Some(primary_readme.clone());
    let result = provider
        .enrich(&[entry], &mut State::default(), true, dir.path())
        .unwrap();
    let requests = server.finish();
    assert_eq!(requests.len(), 1);
    assert!(result.complete);
    assert_eq!(result.budget["expected_requests"], 1);
    assert_eq!(result.budget["actual_requests"], 1);
    let request = body(&requests[0]);
    assert_eq!(
        request["response_format"]["json_schema"]["name"],
        "project_description"
    );
    let payload: Value =
        serde_json::from_str(request["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(payload["readme_excerpt"], primary_readme);
    assert_eq!(payload["evidence_quote"], README_EVIDENCE);
    assert_eq!(payload["avoid_jokes"], json!([]));
    let artifact: Value =
        serde_json::from_slice(&std::fs::read(dir.path().join("llm-bundle.json")).unwrap())
            .unwrap();
    assert_eq!(artifact["provider_reviews"][0]["id"], "a/one");
    assert_eq!(artifact["provider_reviews"][0]["evidence"], README_EVIDENCE);
    assert_eq!(result.evidence_quotes["a/one"], README_EVIDENCE);
    assert_eq!(result.budget["factual_review_performed"], false);
    assert_eq!(
        request["response_format"]["json_schema"]["schema"]["required"],
        json!(["description", "joke", "evidence_quote"])
    );
    assert_eq!(artifact["budget"], result.budget);
    assert_eq!(artifact["prompts"].as_array().unwrap().len(), 1);
    assert!(artifact.get("overview_system").is_none());
}

#[test]
fn missing_evidence_skips_only_that_project_without_requesting_a_summary() {
    let clock = FakeClock::at("2026-10-19T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let response = json!({
        "description":"Локально разбирает исходные файлы и показывает граф их зависимостей.",
        "joke":"",
        "evidence_quote":README_EVIDENCE
    })
    .to_string();
    let server = TestServer::start(vec![
        (200, completion(&response)),
        (200, completion(&response)),
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
    assert_eq!(server.finish().len(), 2);
    assert_eq!(result.descriptions.len(), 2);
    assert!(!result.complete);
    assert_eq!(result.budget["source_gaps"], json!(["b/unknown"]));
    assert_eq!(result.budget["expected_requests"], 2);
    assert_eq!(result.budget["actual_requests"], 2);
}

#[test]
fn source_gaps_are_recorded_without_llm_key_or_free_tier_confirmation() {
    for (do_llm, has_key, free_tier_confirmed) in [
        (false, true, true),
        (true, false, true),
        (true, true, false),
    ] {
        let clock = FakeClock::at("2026-10-25T07:00:00Z");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut groq = provider(&clock, &path, "http://127.0.0.1:1", "openai/gpt-oss-120b");
        groq.token = has_key.then(|| "fixture-key".into());
        groq.free_tier_confirmed = free_tier_confirmed;
        let mut missing = ranked("a/missing");
        missing.snapshot.description = None;
        missing.snapshot.readme_excerpt = None;
        let mut metadata_only = ranked("b/metadata-only");
        metadata_only.snapshot.readme_excerpt = None;
        let mut markup_only = ranked("c/markup-only");
        markup_only.snapshot.readme_excerpt = Some("<img src=\"badge.png\" />".into());
        let mut readme_only = ranked("d/readme-only");
        readme_only.snapshot.description = None;
        let mut state = State::default();
        let result = groq
            .enrich(
                &[missing, metadata_only, markup_only, readme_only],
                &mut state,
                do_llm,
                dir.path(),
            )
            .unwrap();
        assert_eq!(result.budget["source_gaps"], json!(["a/missing"]));
        assert_eq!(
            result.budget["readme_gaps"],
            json!(["b/metadata-only", "c/markup-only"])
        );
        assert_eq!(result.budget["actual_requests"], 0);
        assert_eq!(result.budget["expected_requests"], 0);
        assert!(result.descriptions.is_empty());
        assert!(result.evidence_quotes.is_empty());
        assert!(state.llm_daily_requests.is_empty());
        assert!(!path.exists());
        assert!(clock.sleeps.borrow().is_empty());
        let artifact: Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("llm-bundle.json")).unwrap())
                .unwrap();
        assert_eq!(artifact["items"][0]["evidence_source"], "missing");
        assert_eq!(artifact["items"][1]["evidence_source"], "metadata_only");
        assert_eq!(artifact["items"][3]["evidence_source"], "readme");
        assert_eq!(artifact["budget"], result.budget);
    }
}

#[test]
fn source_gaps_after_a_provider_failure_are_not_hidden() {
    let clock = FakeClock::at("2026-10-25T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let server = TestServer::start(vec![(400, "{}".into())]);
    let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let mut missing = ranked("b/missing");
    missing.snapshot.description = None;
    missing.snapshot.readme_excerpt = None;
    let mut metadata_only = ranked("c/metadata-only");
    metadata_only.snapshot.readme_excerpt = None;
    let result = groq
        .enrich(
            &[ranked("a/failure"), missing, metadata_only],
            &mut State::default(),
            true,
            dir.path(),
        )
        .unwrap();
    assert_eq!(server.finish().len(), 1);
    assert_eq!(result.budget["source_gaps"], json!(["b/missing"]));
    assert_eq!(result.budget["readme_gaps"], json!(["c/metadata-only"]));
    assert_eq!(result.budget["expected_requests"], 1);
    assert_eq!(result.budget["actual_requests"], 1);
    assert!(result.descriptions.is_empty());
}

#[test]
fn metadata_only_project_keeps_source_fallback_without_any_provider_request() {
    let clock = FakeClock::at("2026-10-25T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let groq = provider(&clock, &path, "http://127.0.0.1:1", "openai/gpt-oss-120b");
    let mut entry = ranked("org/metadata-only");
    entry.snapshot.readme_excerpt = None;
    let mut state = State::default();
    let result = groq.enrich(&[entry], &mut state, true, dir.path()).unwrap();
    assert!(result.descriptions.is_empty());
    assert!(result.evidence_quotes.is_empty());
    assert!(!result.complete);
    assert_eq!(result.budget["source_gaps"], json!([]));
    assert_eq!(result.budget["readme_gaps"], json!(["org/metadata-only"]));
    assert_eq!(result.budget["expected_requests"], 0);
    assert_eq!(result.budget["actual_requests"], 0);
    assert!(state.llm_daily_requests.is_empty());
    assert!(!path.exists());
}

#[test]
fn all_missing_sources_spend_no_requests_and_plan_no_overview() {
    let clock = FakeClock::at("2026-10-25T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let groq = provider(&clock, &path, "http://127.0.0.1:1", "openai/gpt-oss-120b");
    let mut entry = ranked("org/missing");
    entry.snapshot.description = None;
    entry.snapshot.readme_excerpt = None;
    let mut state = State::default();
    let result = groq.enrich(&[entry], &mut state, true, dir.path()).unwrap();
    assert_eq!(result.budget["source_gaps"], json!(["org/missing"]));
    assert_eq!(result.budget["expected_requests"], 0);
    assert_eq!(result.budget["actual_requests"], 0);
    assert!(state.llm_daily_requests.is_empty());
}

#[test]
fn quote_matching_metadata_only_is_rejected_without_repair_or_losing_other_cards() {
    let clock = FakeClock::at("2026-10-25T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let description =
        "По README инструмент разбирает исходные файлы локально и строит граф зависимостей. "
            .repeat(4);
    let valid = json!({
        "description":description,"joke":"В графе зависимостей даже баги ходят парами.",
        "evidence_quote":README_EVIDENCE
    });
    let metadata_quote = "Metadata claims the service provides anonymous telemetry.";
    let invalid = json!({
        "description":description,"joke":"В графе зависимостей даже баги ходят парами.",
        "evidence_quote":metadata_quote
    });
    let server = TestServer::start(vec![
        (200, completion(&valid.to_string())),
        (200, completion(&invalid.to_string())),
        (200, completion(&valid.to_string())),
    ]);
    let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let mut invalid_entry = ranked("b/unmatched");
    invalid_entry.snapshot.description = Some(metadata_quote.into());
    let result = groq
        .enrich(
            &[ranked("a/accepted"), invalid_entry, ranked("c/accepted")],
            &mut State::default(),
            true,
            dir.path(),
        )
        .unwrap();
    assert_eq!(server.finish().len(), 3);
    assert_eq!(result.descriptions.len(), 2);
    assert!(!result.descriptions.contains_key("b/unmatched"));
    assert!(!result.jokes.contains_key("b/unmatched"));
    assert_eq!(result.evidence_quotes.len(), 2);
    assert_eq!(result.evidence_quotes["a/accepted"], README_EVIDENCE);
    assert_eq!(result.evidence_quotes["c/accepted"], README_EVIDENCE);
    assert_eq!(result.budget["actual_requests"], 3);
    assert_eq!(result.budget["grounding_failures"][0]["id"], "b/unmatched");
    assert_eq!(
        result.budget["grounding_failures"][0]["additional_requests"],
        0
    );
    assert!(!result.complete);
    assert_eq!(*clock.sleeps.borrow(), [StdDuration::from_secs(30); 2]);
}

#[test]
fn readme_quote_matching_preserves_exact_unicode_and_whitespace() {
    let readme = "# Проект\n\nАвторегрессионный Transformer\nработает с OHLCV-данными.";
    assert!(
        validate_readme_quote(
            readme,
            "Авторегрессионный Transformer\nработает с OHLCV-данными."
        )
        .is_ok()
    );
    assert!(
        validate_readme_quote(
            readme,
            "Авторегрессионный Transformer работает с OHLCV-данными."
        )
        .is_err()
    );
    assert!(
        validate_readme_quote(
            readme,
            "Автокорреляционный Transformer работает с OHLCV-данными."
        )
        .is_err()
    );
    assert!(validate_readme_quote(readme, "Transformer").is_err());
}

#[test]
fn omitted_evidence_does_not_spend_a_schema_repair_request() {
    let clock = FakeClock::at("2026-10-25T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let ungrounded = json!({
        "description":"Локально разбирает исходные файлы и показывает граф их зависимостей.",
        "joke":""
    });
    let server = TestServer::start(vec![(200, completion(&ungrounded.to_string()))]);
    let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let result = groq
        .enrich(
            &[ranked("org/no-quote")],
            &mut State::default(),
            true,
            dir.path(),
        )
        .unwrap();
    let requests = server.finish();
    assert_eq!(requests.len(), 1);
    assert!(result.descriptions.is_empty());
    assert_eq!(result.budget["grounding_failures"][0]["id"], "org/no-quote");
    assert_eq!(result.budget["actual_requests"], 1);
    assert!(clock.sleeps.borrow().is_empty());
}

#[test]
fn usable_readme_skips_a_short_intro_and_rejects_markup_only() {
    let readme = format!("# Project\n\nShort introduction.\n\n{README_EVIDENCE}");
    assert_eq!(usable_readme(Some(&readme)), Some(readme.as_str()));
    assert!(usable_readme(Some("<img src=\"long-name-badge.svg\" />")).is_none());
    assert!(usable_readme(Some("# A sufficiently long heading with no prose")).is_none());
}

#[test]
fn crlf_readme_paragraphs_keep_exact_multiline_evidence() {
    let paragraph = "This tool parses source files locally.\r\nIt reports their dependency graph.";
    let readme = format!(
        "<div align=\"center\">\r\nProject logo\r\n</div>\r\n\r\n# Project\r\n\r\n[![Build](build.svg)](https://example.com/build)\r\n\r\nShort introduction.\r\n\r\n{paragraph}"
    );
    assert_eq!(usable_readme(Some(&readme)), Some(readme.as_str()));
    let quote = readme_evidence_quote(&readme).unwrap();
    assert_eq!(quote, paragraph);
    assert!(readme.contains(&quote));
    validate_readme_quote(&readme, &quote).unwrap();
}

#[test]
fn twelve_local_rejects_are_checkpointed_and_leave_the_next_daily_candidate_eligible() {
    let clock = FakeClock::at("2026-11-08T15:00:00Z");
    let retry_at = clock.now() + chrono::Duration::hours(24);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let mut entries: Vec<_> = (0..12).map(|i| ranked(&format!("bad/p{i:02}"))).collect();
    entries[0].snapshot.readme_excerpt = None;
    entries[1].snapshot.readme_excerpt = None;
    entries[2].snapshot.readme_excerpt = Some("<img src=\"project-banner.svg\" />".into());
    entries[3].snapshot.readme_excerpt = Some("# A project heading without any evidence".into());
    entries.push(ranked("later/ready"));
    let mut state = State {
        snapshots: entries.iter().map(|entry| entry.snapshot.clone()).collect(),
        ..State::default()
    };
    let mut sources: Vec<_> = entries
        .iter()
        .map(|entry| json!({"candidate":entry.snapshot.full_name,"status":"included"}))
        .collect();
    for (i, status) in [(0, 404), (1, 410)] {
        sources[i]["status"] = json!("error");
        sources[i]["http_status"] = json!(status);
    }
    let description = "Локально разбирает исходные файлы и показывает граф их зависимостей.";
    let ungrounded = json!({"description":description,"joke":"",
        "evidence_quote":"This unsupported quotation does not occur in the README."})
    .to_string();
    let mut replies = vec![(200, completion("not JSON")); 8];
    replies.extend(vec![(200, completion(&ungrounded)); 4]);
    let server = TestServer::start(replies);
    let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let candidates = crate::editions::pending(&state, clock.now(), crate::editions::Format::Daily);
    let (selected, _) = crate::editions::select(
        &candidates,
        &state,
        crate::editions::Format::Daily,
        clock.now(),
    );
    assert_eq!(selected.len(), 12);
    let result = groq
        .enrich_daily(&selected, &mut state, dir.path(), &sources)
        .unwrap();
    assert_eq!(server.finish().len(), 12);
    assert!(result.descriptions.is_empty());
    assert_eq!(result.budget["card_failures"].as_array().unwrap().len(), 4);
    assert_eq!(
        result.budget["grounding_failures"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
    assert_eq!(
        result.budget["daily_deferred"].as_array().unwrap().len(),
        12
    );

    // This is the checkpoint available even when the caller rejects an empty issue.
    let mut restored: State = StateStore::new(&path).load().unwrap();
    for entry in &entries[..12] {
        assert_eq!(
            crate::editions::daily_retry_at(&restored, &entry.snapshot.full_name),
            Some(retry_at)
        );
    }
    assert!(restored.covered.is_empty());
    assert!(restored.editions.daily_covered.is_empty());
    assert!(restored.sent_slots.is_empty());
    assert_eq!(
        crate::editions::pending(&restored, clock.now(), crate::editions::Format::Daily).len(),
        13
    );
    let (next, excluded) = crate::editions::select(
        &candidates,
        &restored,
        crate::editions::Format::Daily,
        clock.now(),
    );
    assert_eq!(next.len(), 1);
    assert_eq!(next[0].snapshot.full_name, "later/ready");
    assert_eq!(excluded.len(), 12);
    let (before_due, _) = crate::editions::select(
        &candidates,
        &restored,
        crate::editions::Format::Daily,
        retry_at - chrono::Duration::seconds(1),
    );
    assert_eq!(before_due.len(), 1);
    let (at_due, excluded) = crate::editions::select(
        &candidates,
        &restored,
        crate::editions::Format::Daily,
        retry_at,
    );
    assert_eq!(at_due.len(), 12);
    assert_eq!(excluded.len(), 1);
    assert!(excluded[0].get("retry_at").is_none());

    let card =
        json!({"description":description,"joke":"","evidence_quote":README_EVIDENCE}).to_string();
    let server = TestServer::start(vec![(200, completion(&card))]);
    let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let next_result = groq
        .enrich_daily(&next, &mut restored, dir.path(), &sources)
        .unwrap();
    assert_eq!(server.finish().len(), 1);
    assert!(next_result.descriptions.contains_key("later/ready"));
    assert_eq!(next_result.budget["daily_deferred"], json!([]));
    assert!(restored.editions.daily_covered.is_empty());
}

#[test]
fn transient_readme_failures_configuration_and_budget_stops_do_not_defer() {
    for (status, configured, used) in [
        (Some(403), true, 0),
        (Some(429), true, 0),
        (Some(500), true, 0),
        (None, true, 0),
        (Some(404), false, 0),
        (Some(404), true, DAILY_LLM_REQUEST_CAP),
    ] {
        let clock = FakeClock::at("2026-11-08T15:00:00Z");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut entry = ranked("org/missing");
        entry.snapshot.readme_excerpt = Some("# Stale heading without usable evidence".into());
        let mut groq = provider(&clock, &path, "http://127.0.0.1:1", "openai/gpt-oss-120b");
        groq.free_tier_confirmed = configured;
        let mut state = State::default();
        state.llm_daily_requests.insert("2026-11-08".into(), used);
        let before = serde_json::to_value(&state).unwrap();
        let sources = [json!({"candidate":"org/missing","status":"error","http_status":status})];
        let result = groq
            .enrich_daily(&[entry], &mut state, dir.path(), &sources)
            .unwrap();
        assert_eq!(result.budget["actual_requests"], 0);
        assert_eq!(result.budget["daily_deferred"], json!([]));
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }
}

#[test]
fn weekly_and_offline_enrichment_leave_daily_retry_positions_unchanged() {
    for do_llm in [false, true] {
        let clock = FakeClock::at("2026-11-08T15:00:00Z");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let entries = vec![ranked("org/cached"), ranked("org/later")];
        let mut state = State::default();
        crate::editions::remember_analyses(
            &mut state,
            &json!({"entries":[{
                "full_name":"org/cached","latest":entries[0].snapshot,"llm_used":true,
                "description":"Локально разбирает исходные файлы и показывает граф зависимостей.",
                "joke":"","evidence_quote":README_EVIDENCE
            }]}),
        );
        let retry_at = crate::editions::defer_daily(&mut state, "org/cached", clock.now());
        let mut entries = entries;
        entries[1].snapshot.readme_excerpt = None;
        let groq = provider(&clock, &path, "http://127.0.0.1:1", "openai/gpt-oss-120b");
        let result = groq
            .enrich(&entries, &mut state, do_llm, dir.path())
            .unwrap();
        assert_eq!(result.budget["actual_requests"], 0);
        assert_eq!(result.budget["daily_deferred"], json!([]));
        assert_eq!(
            crate::editions::daily_retry_at(&state, "org/cached"),
            Some(retry_at)
        );
        assert!(crate::editions::daily_retry_at(&state, "org/later").is_none());
    }
}

#[test]
fn daily_enrichment_resumes_checkpointed_cards_without_summary_or_duplicate_requests() {
    let clock = FakeClock::at("2026-11-03T15:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let entries = vec![ranked("org/ready"), ranked("org/later")];
    let card = json!({
        "description":"Локально разбирает исходные файлы и показывает граф их зависимостей.",
        "joke":"У зависимостей появилось семейное древо.",
        "evidence_quote":README_EVIDENCE
    })
    .to_string();
    let server = TestServer::start(vec![(200, completion(&card)), (400, "{}".into())]);
    let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let mut state = State {
        snapshots: entries.iter().map(|entry| entry.snapshot.clone()).collect(),
        ..State::default()
    };
    let first = groq
        .enrich_daily(&entries, &mut state, dir.path(), &[])
        .unwrap();
    assert_eq!(server.finish().len(), 2);
    assert_eq!(first.descriptions.len(), 1);
    assert!(!first.complete);
    let mut restored: State = StateStore::new(&path).load().unwrap();
    assert!(crate::editions::cached_analysis(&restored, &entries[0].snapshot).is_some());
    assert!(restored.covered.is_empty());
    assert!(restored.editions.daily_covered.is_empty());
    assert_eq!(restored.snapshots.len(), 2);
    let server = TestServer::start(vec![(200, completion(&card))]);
    let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let second = groq
        .enrich_daily(&entries, &mut restored, dir.path(), &[])
        .unwrap();
    let requests = server.finish();
    assert_eq!(requests.len(), 1);
    assert!(second.complete);
    assert_eq!(second.budget["actual_requests"], 1);
    assert_eq!(second.budget["expected_requests"], 1);
    assert!(!second.jokes["org/ready"].is_empty());
    assert!(second.jokes["org/later"].is_empty());
    let payload: Value = serde_json::from_str(
        body(&requests[0])["messages"][1]["content"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(payload["avoid_jokes"], json!([second.jokes["org/ready"]]));
    assert!(restored.covered.is_empty());
    assert!(restored.editions.daily_covered.is_empty());
    // Once both cards are cached, even an unavailable endpoint costs nothing.
    let groq = provider(&clock, &path, "http://127.0.0.1:1", "openai/gpt-oss-120b");
    let repeated = groq
        .enrich_daily(&entries, &mut restored, dir.path(), &[])
        .unwrap();
    assert!(repeated.complete);
    assert_eq!(repeated.budget["actual_requests"], 0);
    assert_eq!(repeated.budget["expected_requests"], 0);
}

#[test]
fn rejected_daily_cards_do_not_block_later_cards_or_acknowledge_findings() {
    let description = "Локально разбирает исходные файлы и показывает граф их зависимостей.";
    let card = |description: &str| {
        json!({"description":description,"joke":"","evidence_quote":README_EVIDENCE}).to_string()
    };
    let invented = "Инструмент отправляет граф зависимостей в Salesforce для обработки заявок.";
    for (initial, repaired, expected_error) in [
        (
            card(&"я".repeat(2068)),
            card(&"я".repeat(2059)),
            "description length",
        ),
        (
            card(invented),
            card(invented),
            "named product or integration",
        ),
        ("not JSON".into(), "still not JSON".into(), "valid JSON"),
    ] {
        let clock = FakeClock::at("2026-11-08T15:00:00Z");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let entries = vec![ranked("org/rejected"), ranked("org/ready")];
        let mut state = State {
            snapshots: entries.iter().map(|entry| entry.snapshot.clone()).collect(),
            ..State::default()
        };
        crate::editions::defer_daily(
            &mut state,
            "org/ready",
            clock.now() - chrono::Duration::hours(48),
        );
        let server = TestServer::start(vec![
            (200, completion(&initial)),
            (200, completion(&repaired)),
            (200, completion(&card(description))),
        ]);
        let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
        let result = groq
            .enrich_daily(&entries, &mut state, dir.path(), &[])
            .unwrap();
        assert_eq!(server.finish().len(), 3);
        assert_eq!(result.descriptions.len(), 1);
        assert_eq!(result.descriptions["org/ready"], description);
        assert!(!result.descriptions.contains_key("org/rejected"));
        assert!(!result.complete);
        assert_eq!(result.budget["actual_requests"], 3);
        assert_eq!(result.budget["expected_requests"], 2);
        assert!(
            result.budget["reason"]
                .as_str()
                .unwrap()
                .contains(expected_error)
        );
        let failures = result.budget["card_failures"].as_array().unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0]["id"], "org/rejected");
        assert_eq!(failures[0]["disposition"], "retained_for_later_issue");
        for field in ["initial_error", "repair_error"] {
            assert!(
                failures[0][field]
                    .as_str()
                    .unwrap()
                    .contains(expected_error)
            );
        }
        let artifact: Value =
            serde_json::from_slice(&std::fs::read(dir.path().join("llm-bundle.json")).unwrap())
                .unwrap();
        assert_eq!(
            artifact["budget"]["card_failures"],
            result.budget["card_failures"]
        );
        let restored: State = StateStore::new(&path).load().unwrap();
        assert!(crate::editions::cached_analysis(&restored, &entries[0].snapshot).is_none());
        assert!(crate::editions::cached_analysis(&restored, &entries[1].snapshot).is_some());
        assert_eq!(
            crate::editions::daily_retry_at(&restored, "org/rejected"),
            crate::parse_time("2026-11-09T15:00:00Z")
        );
        assert!(crate::editions::daily_retry_at(&restored, "org/ready").is_none());
        assert!(restored.covered.is_empty());
        assert!(restored.editions.daily_covered.is_empty());
        assert!(restored.sent_slots.is_empty());
        assert_eq!(
            crate::editions::pending(&restored, clock.now(), crate::editions::Format::Daily).len(),
            2,
            "generating or rejecting a card must not acknowledge a finding"
        );
    }
}

#[test]
fn provider_or_budget_failure_during_repair_stops_requests_but_preserves_cached_cards() {
    let invalid = completion("not JSON");
    for (responses, prior_requests, expected_requests, expected_error) in [
        (
            vec![(200, invalid.clone()), (401, "{}".into())],
            0,
            2,
            "HTTP 401",
        ),
        (
            vec![
                (200, invalid.clone()),
                (429, "{}".into()),
                (429, "{}".into()),
                (429, "{}".into()),
            ],
            0,
            4,
            "HTTP 429",
        ),
        (
            vec![(200, invalid)],
            DAILY_LLM_REQUEST_CAP - 1,
            1,
            "budget exhausted",
        ),
    ] {
        let clock = FakeClock::at("2026-11-08T15:00:00Z");
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.json");
        let mut entries = vec![
            ranked("org/rejected"),
            ranked("org/deferred"),
            ranked("org/cached"),
            ranked("org/missing"),
        ];
        entries[3].snapshot.readme_excerpt = None;
        let mut state = State::default();
        state
            .llm_daily_requests
            .insert("2026-11-08".into(), prior_requests);
        crate::editions::remember_analyses(
            &mut state,
            &json!({"entries":[{
                "full_name":"org/cached","latest":entries[2].snapshot,"llm_used":true,
                "description":"Локально разбирает исходные файлы и показывает граф их зависимостей.",
                "joke":"","evidence_quote":README_EVIDENCE
            }]}),
        );
        crate::editions::defer_daily(&mut state, "org/cached", clock.now());
        let server = TestServer::start(responses);
        let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
        let sources = [json!({"candidate":"org/missing","status":"included"})];
        let result = groq
            .enrich_daily(&entries, &mut state, dir.path(), &sources)
            .unwrap();
        assert_eq!(server.finish().len(), expected_requests);
        assert_eq!(result.budget["actual_requests"], expected_requests);
        assert_eq!(result.budget["card_failures"], json!([]));
        assert!(
            result.budget["reason"]
                .as_str()
                .unwrap()
                .contains(expected_error)
        );
        assert_eq!(result.descriptions.len(), 1);
        assert!(result.descriptions.contains_key("org/cached"));
        assert!(!result.descriptions.contains_key("org/deferred"));
        assert!(!result.complete);
        let restored: State = StateStore::new(&path).load().unwrap();
        assert_eq!(
            restored.llm_daily_requests["2026-11-08"],
            prior_requests + expected_requests
        );
        assert!(restored.editions.daily_covered.is_empty());
        assert_eq!(result.budget["daily_deferred"], json!([]));
        for entry in &entries {
            assert!(
                crate::editions::daily_retry_at(&restored, &entry.snapshot.full_name).is_none()
            );
        }
    }
}

#[test]
fn humour_context_is_bounded_and_old_duplicates_and_templates_are_omitted() {
    let clock = FakeClock::at("2026-11-04T15:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let mut jokes = vec![
        "У зависимостей появилось семейное древо.".to_string(),
        "Импорты наконец познакомились со своими соседями.".to_string(),
        "Клубок исходников получил собственную карту.".to_string(),
        "Стрелочки нашли объяснение вашим архитектурным решениям.".to_string(),
        "Круговую зависимость теперь можно обвести пальцем.".to_string(),
        "Развязку этой истории ищите на соседнем ребре.".to_string(),
        "Модуль думал, что живёт один. Граф показал родню.".to_string(),
        "Исходники сдали адреса всех знакомых.".to_string(),
        "В проекте обнаружились связи, о которых он предпочитал молчать.".to_string(),
    ];
    // The duplicate is outside the bounded prompt context and differs in case
    // and punctuation. Local suppression still covers the entire issue.
    jokes.push(jokes[0].to_uppercase().replace('.', "!!!"));
    jokes.push("Растёт быстрее, чем его тудушка.".to_string());
    let responses = jokes
        .iter()
        .map(|joke| {
            (
                200,
                completion(
                    &json!({
                        "description":"Локально разбирает исходные файлы и показывает граф их зависимостей.",
                        "joke":joke,"evidence_quote":README_EVIDENCE
                    })
                    .to_string(),
                ),
            )
        })
        .collect();
    let entries = (0..jokes.len())
        .map(|index| ranked(&format!("org/project-{index}")))
        .collect::<Vec<_>>();
    let server = TestServer::start(responses);
    let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let result = groq
        .enrich(&entries, &mut State::default(), true, dir.path())
        .unwrap();
    let requests = server.finish();
    assert_eq!(requests.len(), jokes.len());
    assert!(result.complete);
    assert_eq!(result.budget["actual_requests"], jokes.len());
    for (index, joke) in jokes.iter().take(9).enumerate() {
        assert_eq!(result.jokes[&format!("org/project-{index}")], *joke);
    }
    assert!(result.jokes["org/project-9"].is_empty());
    assert!(result.jokes["org/project-10"].is_empty());
    for request in &requests {
        let payload: Value =
            serde_json::from_str(body(request)["messages"][1]["content"].as_str().unwrap())
                .unwrap();
        assert!(payload["avoid_jokes"].as_array().unwrap().len() <= JOKE_CONTEXT_LIMIT);
    }
    let payload: Value = serde_json::from_str(
        body(&requests[9])["messages"][1]["content"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(payload["avoid_jokes"], json!(&jokes[1..9]));
}

#[test]
fn observed_joke_templates_are_dropped_without_a_global_word_blacklist() {
    for joke in [
        "Почему модель не генерирует токены? Потому что ей нравится решать задачи без лишних слов!",
        "Подготовка к интервью — как отладка кода: иногда баги появляются там, где их не ждёшь.",
        "DeepOpen умеет классифицировать запросы быстрее, чем я успеваю решить кроссворд.",
        "Сменить фон на iPhone проще, чем поменять настроение утром!",
    ] {
        assert!(select_joke(joke, &[]).is_empty(), "{joke}");
    }
    let specific = "Счётчик кофе признаёт только натуральные числа.";
    assert_eq!(select_joke(specific, &[]), specific);
}

#[test]
fn previous_jokes_cannot_ground_invented_integrations_in_a_later_card() {
    let clock = FakeClock::at("2026-11-04T15:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let first_joke = "У Salesforce появился семейный граф зависимостей.";
    let valid = json!({
        "description":"Локально разбирает исходные файлы и показывает граф их зависимостей.",
        "joke":first_joke,"evidence_quote":README_EVIDENCE
    });
    let invented = json!({
        "description":"Инструмент отправляет граф зависимостей в Salesforce для обработки заявок.",
        "joke":"","evidence_quote":README_EVIDENCE
    });
    let server = TestServer::start(vec![
        (200, completion(&valid.to_string())),
        (200, completion(&invented.to_string())),
        (200, completion(&invented.to_string())),
    ]);
    let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let mut first = ranked("org/first");
    first.snapshot.description = Some("Shows dependency graphs for Salesforce projects.".into());
    let result = groq
        .enrich(
            &[first, ranked("org/unrelated")],
            &mut State::default(),
            true,
            dir.path(),
        )
        .unwrap();
    let requests = server.finish();
    assert_eq!(requests.len(), 3);
    assert_eq!(result.descriptions.len(), 1);
    assert!(!result.descriptions.contains_key("org/unrelated"));
    assert!(!result.complete);
    let payload: Value = serde_json::from_str(
        body(&requests[1])["messages"][1]["content"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(payload["avoid_jokes"], json!([first_joke]));
    assert!(
        result.budget["card_failures"][0]["repair_error"]
            .as_str()
            .unwrap()
            .contains("named product or integration")
    );
}
