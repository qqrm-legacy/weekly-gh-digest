"""Exact source-only integration edits; validation exports the resulting blobs."""
from pathlib import Path


def edit(path, old, new, count=1):
    p = Path(path)
    text = p.read_text()
    if text.count(old) != count:
        raise RuntimeError('Source anchor changed: ' + path)
    p.write_text(text.replace(old, new))


edit('.github/scripts/daily_run.py', 'hour=7, minute=0', 'hour=15, minute=0')
edit('.github/scripts/daily_run.py', '    key = slot(now)\n', '    # Never turn morning collection into an early daily publication.\n    if now.astimezone(timezone.utc).hour < 15:\n        return ("collect", False, False), "daily-before-evening-deadline"\n    key = slot(now)\n')
edit('.github/scripts/run_mode.py', 'DAILY_CRON = "37 7 * * *"', 'DAILY_CRON = "0 15 * * *"')
edit('.github/workflows/run-bot.yml', "    - cron: '37 7 * * *'", "    # Daily discoveries at 18:00 Moscow (UTC+3).\n    - cron: '0 15 * * *'")
edit('src/editions.rs', '(now - Duration::hours(7)).date_naive()', '(now - Duration::hours(15)).date_naive()')
edit('src/editions.rs', 'Format::Daily.slot(time("2027-01-01T06:59:59Z"))', 'Format::Daily.slot(time("2027-01-01T14:59:59Z"))')
edit('src/editions.rs', 'Format::Daily.slot(time("2027-01-01T07:00:00Z"))', 'Format::Daily.slot(time("2027-01-01T15:00:00Z"))')
p = Path('.github/scripts/test_daily_run.py')
p.write_text(p.read_text().replace('2026, 10, 31, 8,', '2026, 10, 31, 16,').replace('2027, 1, 1, 6, 59', '2027, 1, 1, 14, 59').replace('2027, 1, 1, 7, tzinfo', '2027, 1, 1, 15, tzinfo'))
edit('.github/scripts/test_daily_run.py', '    def test_daily_requires_two_explicit_gates(self):', '''    def test_morning_collection_does_not_send_yesterday_or_today(self):
        env = {"DIGEST_DELIVERY_ENABLED": "true", "DIGEST_DAILY_DELIVERY_ENABLED": "true"}
        before = datetime(2026, 11, 3, 14, 59, 59, tzinfo=timezone.utc)
        self.assertEqual(daily_run.recovery_mode(env, before, {}, {})[0], ("collect", False, False))
        after = datetime(2026, 11, 3, 15, tzinfo=timezone.utc)
        self.assertEqual(daily_run.recovery_mode(env, after, {}, {})[0], ("daily-digest", True, True))
        sent = {"sent_slots": {daily_run.slot(after): 12}}
        self.assertEqual(daily_run.recovery_mode(env, after, {}, sent)[0], ("collect", False, False))
        self.assertEqual(run_mode.DAILY_CRON, "0 15 * * *")

    def test_daily_requires_two_explicit_gates(self):''')
edit('src/providers.rs', 'Начни с названия проекта и его задачи, без вводной «В этой подборке».', 'Начни с названия проекта и его задачи, без вводной «В этой подборке». Объясни человеческим языком: что это, кому и для какой конкретной работы пригодится, какой сценарий применения следует из источников и какое существенное ограничение явно указано. Не пересказывай структуру README; не перечисляй рекламные лозунги. Возможный сценарий помечай «можно рассмотреть для», а не выдавай его за проверенный результат.')
edit('src/providers.rs', 'Сохраняй точную терминологию;', 'Multi-agent переводится как «многоагентный», а не «многопользовательский»; multi-user — отдельное свойство, требующее отдельного подтверждения. Local-first не означает автономность без интернета; offline можно утверждать только при прямом подтверждении. Сохраняй точную терминологию;')
edit('src/editorial.rs', 'POLICY_VERSION: u32 = 2', 'POLICY_VERSION: u32 = 3')
edit('src/editorial.rs', 'pub(super) fn require_synthesis(digest: &Value) -> Result<()> {', r'''/// Language and observed terminology regressions only; not a factual audit.
pub(super) fn validate_text(text: &str, sources: &str) -> Result<()> {
    let letters = text.chars().filter(|ch| ch.is_alphabetic()).count();
    let cyrillic = text.chars().filter(|ch| matches!(ch, '\u{0400}'..='\u{04ff}')).count();
    ensure!(cyrillic >= 10 && cyrillic * 4 >= letters, "editorial text must be Russian");
    let text = text.to_lowercase();
    let sources = sources.to_lowercase();
    let multi_agent = sources.contains("multi-agent") || sources.contains("multi agent") || sources.contains("multiagent");
    let multi_user = sources.contains("multi-user") || sources.contains("multi user") || sources.contains("multiuser") || sources.contains("многопользовательск");
    ensure!(!(multi_agent && !multi_user && text.contains("многопользовательск")), "multi-agent is not multi-user");
    let local_first = sources.contains("local-first") || sources.contains("local first");
    let offline_claim = ["полностью офлайн", "без интернета", "не требует интернета", "не обращается к облаку"].iter().any(|claim| text.contains(claim));
    ensure!(!(local_first && offline_claim && !sources.contains("offline") && !sources.contains("офлайн")), "local-first does not prove offline operation");
    Ok(())
}

pub(super) fn require_synthesis(digest: &Value) -> Result<()> {''')
edit('src/editorial.rs', '    Ok(())\n}\n\n#[cfg(test)]', '''    if requested && digest["edition"] == "daily" {
        for entry in digest["entries"].as_array().into_iter().flatten() {
            ensure!(entry["llm_used"] == true, "daily source fallback must remain queued, not be published");
            validate_text(entry["description"].as_str().unwrap_or(""), &entry["latest"].to_string())?;
        }
        if has_entries { validate_text(digest["overview"].as_str().unwrap_or(""), &digest["entries"].to_string())?; }
    }
    Ok(())
}

#[cfg(test)]''')
edit('src/editorial.rs', '    #[test]\n    fn rate_limit_honors', '''    #[test]
    fn observed_translation_regressions_are_rejected_not_rewritten() {
        assert!(validate_text("Многопользовательский инструмент для автоматического анализа.", "A multi-agent framework").is_err());
        assert!(validate_text("Многоагентный инструмент для автоматического анализа.", "A multi-agent framework").is_ok());
        assert!(validate_text("Многопользовательский инструмент для автоматического анализа.", "multi-agent and multi-user").is_ok());
        assert!(validate_text("Работает полностью офлайн без интернета и внешних сервисов.", "local-first data store with cloud APIs").is_err());
        assert!(validate_text("This tool is used to process source files and build graphs.", "source").is_err());
    }

    #[test]
    fn daily_fallback_is_not_an_editorial_card() {
        let digest = json!({"edition":"daily", "llm_requested":true, "overview":"В этой подборке представлены инструменты анализа программ.", "entries":[{"llm_used":false,"description":"Raw README description"}]});
        assert!(require_synthesis(&digest).is_err());
    }

    #[test]
    fn rate_limit_honors''')
edit('src/providers.rs', '''    pub(super) fn enrich(
        &self,
        entries: &[Ranked],
        state: &mut State,
        do_llm: bool,
        run_dir: &Path,
    ) -> Result<Enrichment> {
''', '''    pub(super) fn enrich(
        &self,
        entries: &[Ranked],
        state: &mut State,
        do_llm: bool,
        run_dir: &Path,
    ) -> Result<Enrichment> {
        self.enrich_mode(entries, state, do_llm, run_dir, false)
    }

    pub(super) fn enrich_daily(
        &self,
        entries: &[Ranked],
        state: &mut State,
        run_dir: &Path,
    ) -> Result<Enrichment> {
        self.enrich_mode(entries, state, true, run_dir, true)
    }

    fn enrich_mode(
        &self,
        entries: &[Ranked],
        state: &mut State,
        do_llm: bool,
        run_dir: &Path,
        ready_cards_only: bool,
    ) -> Result<Enrichment> {
''')
edit('src/providers.rs', '''            let (description, project_joke, evidence_quote) = match parse_project_response(
                &response,
            ) {''', '''            let parse_card = |text: &str| -> Result<(String, String, String)> {
                let parsed = parse_project_response(text)?;
                crate::editorial::validate_text(&parsed.0, &payload)?;
                Ok(parsed)
            };
            let (description, project_joke, evidence_quote) = match parse_card(&response) {''')
edit('src/providers.rs', '.and_then(|repaired| parse_project_response(&repaired))', '.and_then(|repaired| parse_card(&repaired))')
edit('src/providers.rs', '''            evidence_quotes.insert(snapshot.full_name.clone(), evidence_quote);
            descriptions.insert(snapshot.full_name.clone(), description);
            jokes.insert(snapshot.full_name.clone(), project_joke);''', '''            // Persist completed work before the next provider request can fail.
            crate::editions::remember_analyses(state, &json!({"entries":[{
                "full_name":snapshot.full_name,"latest":snapshot,"llm_used":true,
                "description":description,"joke":project_joke,"evidence_quote":evidence_quote
            }]}));
            write_json(self.state_path, state)?;
            evidence_quotes.insert(snapshot.full_name.clone(), evidence_quote);
            descriptions.insert(snapshot.full_name.clone(), description);
            jokes.insert(snapshot.full_name.clone(), project_joke);''')
edit('src/providers.rs', '        let overview_payload = source_overview_payload(entries);', '''        let overview_entries = entries.iter()
            .filter(|entry| !ready_cards_only || descriptions.contains_key(&entry.snapshot.full_name))
            .cloned().collect::<Vec<_>>();
        let overview_payload = source_overview_payload(&overview_entries);''')
edit('src/providers.rs', '        Ok(safe_public_text(text))\n    }', '''        crate::editorial::validate_text(text, &payload.to_string())?;
        Ok(safe_public_text(text))
    }''')
edit('src/main.rs', '    let (mut entries, selection_excluded) = editions::select(current, state, format);', '    let (mut entries, mut selection_excluded) = editions::select(current, state, format);')
edit('src/main.rs', '''    } = provider.enrich(&entries, state, do_llm, &runs_dir().join(stamp(now)))?;
    let entries_json''', '''    } = if format == editions::Format::Daily && do_llm {
        provider.enrich_daily(&entries, state, &runs_dir().join(stamp(now)))?
    } else {
        provider.enrich(&entries, state, do_llm, &runs_dir().join(stamp(now)))?
    };
    if format == editions::Format::Daily && do_llm {
        let attempted = entries.len();
        entries.retain(|entry| {
            let ready = descriptions.contains_key(&entry.snapshot.full_name);
            if !ready { selection_excluded.push(json!({"full_name":entry.snapshot.full_name,"reason":"AI explanation unavailable; retained for a later daily issue"})); }
            ready
        });
        let ready_pool = pool_rows.into_iter().filter(|row| row["full_name"].as_str().is_some_and(|name| descriptions.contains_key(name))).collect::<Vec<_>>();
        write_json(&runs_dir().join(stamp(now)).join("selection.json"),
            &json!({"pool":ready_pool,"excluded":selection_excluded,"covered_repositories_tracked":state.covered.len()}))?;
        anyhow::ensure!(attempted == 0 || !entries.is_empty(), "no source-backed Russian explanations ready; findings remain queued");
    }
    let entries_json''')
edit('src/main.rs', '        "llm_used":complete,\n        "llm_requested":do_llm,', '        "llm_used":complete || (format == editions::Format::Daily && do_llm && !entries.is_empty()),\n        "llm_requested":do_llm,')
p = Path('src/providers/tests.rs')
p.write_text(p.read_text() + r'''

#[test]
fn daily_summary_uses_only_ready_cards_and_restart_reuses_checkpointed_analysis() {
    let clock = FakeClock::at("2026-11-03T15:00:00Z");
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("state.json");
    let entries = vec![ranked("org/ready"), ranked("org/later")];
    let card = json!({
        "description":"Инструмент разбирает исходные файлы и показывает связи между ними. Такой граф можно рассмотреть для знакомства с зависимостями незнакомого проекта. Описание возможностей опирается на README; работу программы мы не проверяли.",
        "joke":"Даже у зависимостей есть свой круг общения.",
        "evidence_quote":README_EVIDENCE
    }).to_string();
    let summary = completion(r#"{"overview":"В этой подборке есть инструмент для анализа связей между исходными файлами."}"#);
    let server = TestServer::start(vec![(200, completion(&card)), (400, "{}".into()), (200, summary.clone())]);
    let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let mut state = State::default();
    state.snapshots = entries.iter().map(|entry| entry.snapshot.clone()).collect();
    let first = groq.enrich_daily(&entries, &mut state, dir.path()).unwrap();
    let requests = server.finish();
    assert_eq!(first.descriptions.len(), 1);
    let payload: Value = serde_json::from_str(body(&requests[2])["messages"][1]["content"].as_str().unwrap()).unwrap();
    assert_eq!(payload["entries"].as_array().unwrap().len(), 1);
    assert_eq!(payload["entries"][0]["repository"], "org/ready");
    let mut restored: State = StateStore::new(&path).load().unwrap();
    assert!(crate::editions::cached_analysis(&restored, &entries[0].snapshot).is_some());
    assert!(restored.covered.is_empty());
    assert!(restored.editions.daily_covered.is_empty());
    assert_eq!(restored.snapshots.len(), 2);
    let server = TestServer::start(vec![(200, completion(&card)), (200, summary)]);
    let groq = provider(&clock, &path, &server.url, "openai/gpt-oss-120b");
    let second = groq.enrich_daily(&entries, &mut restored, dir.path()).unwrap();
    assert_eq!(server.finish().len(), 2);
    assert!(second.complete);
    assert_eq!(second.budget["actual_requests"], 2);
    assert!(restored.covered.is_empty());
    assert!(restored.editions.daily_covered.is_empty());
}
''')
p = Path('docs/daily-weekly-queue.md')
s = p.read_text().replace('starting at 07:00 UTC (10:00 Moscow)', 'starting at 15:00 UTC (18:00 Moscow)').replace('`37 7 * * *`', '`0 15 * * *`')
s += '''
## Evening daily edition

The daily deadline is 18:00 Moscow (15:00 UTC), without a test expiry in the normal
Run Bot workflow. Three-hour collection remains enabled so transient observations
are retained. Morning recovery ticks collect only; unsuccessful evening work can
retry at later collection ticks. A successful daily slot is never resent.

Daily LLM editions publish only source-backed Russian explanations that completed
validation. Raw README/metadata fallbacks stay in the daily queue; the synopsis
uses only the cards actually included. A failed synopsis prevents sending, not
collection. Completed analyses are checkpointed individually and reused when
sources and editorial policy match. Weekly eligibility remains independent.

Explanations cover what the repository does, its intended user, a source-supported
use case and documented limits. Language and known terminology regression checks
are not a complete factual audit. Free Groq is used only with explicit free-tier
confirmation; existing request caps, retry-after handling and no paid fallback
remain in place. Exact on-the-minute delivery cannot be guaranteed by Actions or
a free external provider. Capacity overflow remains queued, not discarded.
'''
p.write_text(s)
