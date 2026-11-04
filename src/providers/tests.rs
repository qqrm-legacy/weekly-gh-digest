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
fn concise_cards_accept_empty_humour_and_bound_unicode_without_truncation() {
    let description = "Локально строит граф зависимостей исходных файлов.";
    let parsed = parse_project_description(
        &json!({"description":description,"joke":" \n\t "}).to_string(),
    )
    .unwrap();
    assert_eq!(parsed, (description.into(), String::new()));
    let boundary = "я".repeat(MAX_DESCRIPTION_CHARS);
    assert_eq!(
        parse_project_description(&json!({"description":boundary,"joke":""}).to_string())
            .unwrap()
            .0,
        boundary
    );
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
fn partial_enrichment_keeps_confirmed_descriptions_and_original_fallback_contract() {
    let clock = FakeClock::at("2026-10-14T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let description = "Локально разбирает исходные файлы и показывает граф их зависимостей.";
    let response =
        json!({"description":description,"joke":"","evidence_quote":README_EVIDENCE})
            .to_string();
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
fn completed_enrichment_uses_one_request_per_card_and_preserves_primary_evidence() {
    let clock = FakeClock::at("2026-10-14T07:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let description = "Локально разбирает исходные файлы и показывает граф их зависимостей.";
    let response =
        json!({"description":description,"joke":"","evidence_quote":README_EVIDENCE})
            .to_string();
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
    let server = TestServer::start(vec![(200, completion(&response)), (200, completion(&response))]);
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
fn daily_enrichment_resumes_checkpointed_cards_without_summary_or_duplicate_requests() {
    let clock = FakeClock::at("2026-11-03T15:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let entries = vec![ranked("org/ready"), ranked("org/later")];
    let card = json!({
        "description":"Локально разбирает исходные файлы и показывает граф их зависимостей.",
        "joke":"У зависимостей появилось семейное древо.",
        "evidence_quote":README_EVIDENCE
    }).to_string();
    let server = TestServer::start(vec![(200, completion(&card)), (400, "{}".into())]);
    let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let mut state = State {
        snapshots: entries.iter().map(|entry| entry.snapshot.clone()).collect(),
        ..State::default()
    };
    let first = groq.enrich_daily(&entries, &mut state, dir.path()).unwrap();
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
        .enrich_daily(&entries, &mut restored, dir.path())
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
        .enrich_daily(&entries, &mut restored, dir.path())
        .unwrap();
    assert!(repeated.complete);
    assert_eq!(repeated.budget["actual_requests"], 0);
    assert_eq!(repeated.budget["expected_requests"], 0);
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
        result.budget["reason"]
            .as_str()
            .unwrap()
            .contains("named product or integration")
    );
}
