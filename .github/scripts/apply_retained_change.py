"""One-shot, exact-anchor integration on the reviewed feature branch only."""
from pathlib import Path
import os

EXPECTED_BRANCH = "feat/retained-daily-weekly-digests-2026-10-31"
if os.environ.get("GITHUB_REF_NAME") != EXPECTED_BRANCH:
    raise SystemExit("Refusing to edit any other branch")


def edit(path, old, new, count=1):
    p = Path(path)
    text = p.read_text()
    if text.count(old) != count:
        raise RuntimeError(f"Anchor mismatch in {path}: expected {count}, found {text.count(old)}: {old[:120]!r}")
    p.write_text(text.replace(old, new))


MAIN = "src/main.rs"
edit(MAIN, "mod delivery;\n", "mod delivery;\nmod editions;\n")
edit(MAIN, "    schema: u32,\n", "    schema: u32,\n    #[serde(default)]\n    editions: editions::Ledger,\n")
edit(MAIN, '''    let state: State = state_store::StateStore::new(&state_path()).load()?;
    anyhow::ensure!(
        state.uncertain_delivery.is_none(),
        "unconfirmed Telegram delivery; operator reconciliation required"
    );
    Ok(state)''', '''    let mut state: State = state_store::StateStore::new(&state_path()).load()?;
    editions::migrate(&mut state)?;
    // Collection and previews may continue during reconciliation. The sender
    // independently refuses every side effect while delivery is uncertain.
    Ok(state)''')
edit(MAIN, '''    Ok(())
}

fn exact_evidence_quote''', '''    editions::validate_catalogue(digest, &selection)?;
    Ok(())
}

fn exact_evidence_quote''')
edit(MAIN, '''    let (mut entries, selection_excluded) = select_candidates(current, &state.covered);''', '''    make_digest_for(state, current, manifest, now, do_llm, editions::Format::Weekly)
}

fn make_digest_for(
    state: &mut State,
    current: &[Snapshot],
    manifest: &Value,
    now: DateTime<Utc>,
    do_llm: bool,
    format: editions::Format,
) -> Result<(Value, String)> {
    let (mut entries, selection_excluded) = editions::select(current, state, format);''')
p = Path(MAIN)
s = p.read_text()
a, b = s.index("fn make_digest_for("), s.index("fn safe_public_text(")
part = s[a:b]
part = part.replace("source_priority(&entry.snapshot)", "editions::priority(&entry.snapshot, format)")
part = part.replace('"delivery_slot":slot_key_of(now)', '"delivery_slot":format.slot(now),\n        "edition":format.name()')
s = s[:a] + part + s[b:]
p.write_text(s)
edit("src/providers.rs", '''            bundles.push(bundle.clone());
            if !do_llm''', '''            bundles.push(bundle.clone());
            if let Some(cached) = crate::editions::cached_analysis(state, snapshot) {
                descriptions.insert(snapshot.full_name.clone(), cached.description);
                jokes.insert(snapshot.full_name.clone(), cached.joke);
                evidence_quotes.insert(snapshot.full_name.clone(), cached.evidence_quote);
                provider_reviews.push(json!({"id":snapshot.full_name,"stage":"cached_analysis", "primary_sources_unchanged":true}));
                continue;
            }
            if !do_llm''')
edit(MAIN, '''    let header = format!(
        "*🚀 GitHub — итоги недели*\\nСрез: {when}\\n{} с прошлого выпуска — всё опубликованное ранее не повторяется\\n",
        markdown_v2_escape(&counted)
    );''', '''    let header = if digest["edition"] == "daily" {
        format!("*🚀 GitHub — находки дня*\\nСрез: {when}\\n{} из очереди находок\\n{}",
            markdown_v2_escape(&counted), markdown_v2_escape("Дневная публикация сохраняет проект для недельного обзора. Остаток очереди переносится."))
    } else if digest.get("edition").is_some() {
        format!("*🚀 GitHub — итоги недели*\\nСрез: {when}\\n{} с подробным разбором\\n{}",
            markdown_v2_escape(&counted), markdown_v2_escape("Включены находки дневных выпусков и недоставленный остаток. Остальные ссылки — в указателе выпуска."))
    } else {
        format!("*🚀 GitHub — итоги недели*\\nСрез: {when}\\n{} с прошлого выпуска — всё опубликованное ранее не повторяется\\n", markdown_v2_escape(&counted))
    };''')
edit(MAIN, '''    let footer = format!(
        "_{}_",
        markdown_v2_escape(
            "Порядок: недельный Trending, затем дневный Trending, затем новые репозитории. Полные данные — в артефакте прогона."
        )
    );''', '''    let footer = format!("_{}_", markdown_v2_escape(if digest["edition"] == "daily" {
        "Порядок: старейшая дата находки, затем дневной Trending, недельный Trending и поиск. Не вошедшие проекты остаются в очереди."
    } else {
        "Порядок: недельный Trending, затем дневный Trending, затем новые репозитории. Полные данные — в артефакте прогона."
    }));''')
edit(MAIN, '''    chunks.push(current);
    Ok(chunks)
}

/// Joined representation''', '''    chunks.push(current);
    editions::append_catalogue(&mut chunks, digest)?;
    Ok(chunks)
}

/// Joined representation''')
edit(MAIN, '''        "# GitHub — итоги недели\\n\\nПериод наблюдений: {} — {}\\n\\n{}\\n",
        digest["period_start"]''', '''        "# GitHub — {}\\n\\nПериод обзора: {} — {}\\n\\n{}\\n",
        if digest["edition"] == "daily" { "находки дня" } else { "итоги недели" },
        digest["period_start"]''')
edit(MAIN, '''    text
}

fn telegram_send_to''', '''    text.push_str(&editions::catalogue_markdown(digest));
    text
}

fn telegram_send_to''')
edit(MAIN, '''fn trim_covered_entries(digest: &mut Value, state: &State) -> usize {
    let Some(entries)''', '''fn trim_covered_entries(digest: &mut Value, state: &State) -> usize {
    let format = if digest["edition"] == "daily" { editions::Format::Daily } else { editions::Format::Weekly };
    let Some(entries)''')
edit(MAIN, '''.is_none_or(|name| !state.covered.contains_key(name))''', '''.is_none_or(|name| !editions::was_published(state, format, name))''')
edit(MAIN, '''    for entry in digest["entries"].as_array().into_iter().flatten() {
        if let Some(name) = entry["full_name"].as_str() {
            state
                .covered
                .entry(name.to_string())
                .or_insert_with(|| day.to_string());
        }
    }
    state.sent_digests''', '''    editions::acknowledge(state, digest, day)?;
    state.sent_digests''')
p = Path(MAIN)
s = p.read_text()
a, b = s.index("/// Completes delivery of earlier issues"), s.index("fn main() -> Result<()>")
s = s[:a] + '''/// Resume the frozen weekly payload even when its original slot is overdue.
fn deliver_pending_digests(state: &mut State, now: DateTime<Utc>) -> Result<()> {
    editions::resume(state, now, editions::Format::Weekly)
}

''' + s[b:]
p.write_text(s)
edit(MAIN, '''fn main() -> Result<()> {
    checkpoint::require_hosted_configuration()?;
    let args: Vec<String> = env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("preview");''', '''fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("preview");
    if command == "preview-retained" {
        return editions::preview_retained(&args);
    }
    checkpoint::require_hosted_configuration()?;
    let format = if args.iter().any(|arg| arg == "--daily") { editions::Format::Daily } else { editions::Format::Weekly };''')
edit(MAIN, '''            "publication is disabled pending preview review"
        );
    }
    if command == "preview-stored"''', '''            "publication is disabled pending preview review"
        );
        anyhow::ensure!(format != editions::Format::Daily || schedule::delivery_enabled(env::var("DIGEST_DAILY_DELIVERY_ENABLED").ok().as_deref()),
            "daily publication is disabled pending preview review");
    }
    if command == "preview-stored"''')
edit(MAIN, '''        validate_digest_selection(&run_dir, &digest)?;
        if digest["entries"]''', '''        validate_digest_selection(&run_dir, &digest)?;
        anyhow::ensure!(editions::Format::from_digest(&digest)? != editions::Format::Daily || schedule::delivery_enabled(env::var("DIGEST_DAILY_DELIVERY_ENABLED").ok().as_deref()),
            "daily publication is disabled pending preview review");
        if digest["entries"]''')
edit(MAIN, '''    let digest_id = stamp(now);
    let slot_key = slot_key_of(now);
    if command == "digest" && !dry {
        deliver_pending_digests(&mut state, now)?;
    }''', '''    let digest_id = format!("{}-{}", format.name(), stamp(now));
    let slot_key = format.slot(now);
    if command == "digest" && !dry {
        if format == editions::Format::Weekly {
            deliver_pending_digests(&mut state, now)?;
        } else {
            editions::resume(&mut state, now, format)?;
        }
    }''')
edit(MAIN, '''    let (mut digest, output) = make_digest(&mut state, &weekly, &gathered.manifest, now, llm)?;
    record_coverage_summary(&digest["coverage"])?;
    save_preview_files(&gathered.dir, &digest, command == "preview" && llm)?;''', '''    let candidates = editions::pending(&state, now, format);
    let (mut digest, output) = if format == editions::Format::Weekly {
        make_digest(&mut state, &candidates, &gathered.manifest, now, llm)?
    } else {
        make_digest_for(&mut state, &candidates, &gathered.manifest, now, llm, format)?
    };
    editions::decorate(&mut digest, &candidates, now, format, &gathered.dir)?;
    record_coverage_summary(&digest["coverage"])?;
    save_preview_files(&gathered.dir, &digest, command == "preview" && llm)?;
    editions::remember_analyses(&mut state, &digest);
    write_json(&state_path(), &state)?;''')
edit("src/editions.rs", "Self::Weekly => schedule::slot(now),", "Self::Weekly => crate::slot_key_of(now),")
edit(MAIN, '''    if state.sent_digests.contains_key(digest_id) {
        println!("Digest {digest_id}''', '''    editions::Format::from_digest(digest)?;
    if state.sent_digests.contains_key(digest_id) {
        println!("Digest {digest_id}''')
edit("src/schedule.rs", '''pub(super) fn candidates(state: &State, now: DateTime<Utc>) -> Vec<Snapshot> {
    let cutoff = now - Duration::days(7);''', '''pub(super) fn candidates(state: &State, now: DateTime<Utc>) -> Vec<Snapshot> {
    candidates_since(state, now, Some(now - Duration::days(7)))
}

/// Pending findings do not expire with the reporting window.
pub(super) fn all_candidates(state: &State, now: DateTime<Utc>) -> Vec<Snapshot> {
    candidates_since(state, now, None)
}

fn candidates_since(state: &State, now: DateTime<Utc>, cutoff: Option<DateTime<Utc>>) -> Vec<Snapshot> {''')
edit("src/schedule.rs", "if captured < cutoff || captured > now {", "if cutoff.is_some_and(|cutoff| captured < cutoff) || captured > now {")
edit("src/schedule.rs", ".entry(snapshot.full_name.clone())", ".entry(snapshot.full_name.to_ascii_lowercase())")
edit(".github/scripts/weekly_run.py", "def validate_preview(directory, expected_slot):", "def validate_preview(directory, expected_slot, slot_resolver=slot):")
edit(".github/scripts/weekly_run.py", "or slot(generated) != expected_slot", "or slot_resolver(generated) != expected_slot")
edit(".github/scripts/test_run_mode.py", "from run_mode import COLLECTION_CRON, PUBLICATION_CRON, choose", "from run_mode import COLLECTION_CRON, PUBLICATION_CRON, DAILY_CRON, choose")
edit(".github/scripts/test_run_mode.py", "[COLLECTION_CRON, PUBLICATION_CRON])", "[COLLECTION_CRON, PUBLICATION_CRON, DAILY_CRON])")

WORKFLOW = ".github/workflows/run-bot.yml"
edit(WORKFLOW, "    - cron: '0 7 * * 1'", "    - cron: '0 7 * * 1'\n    - cron: '37 7 * * *'")
edit(WORKFLOW, "    inputs:\n      dry_run:", '''    inputs:
      digest_format:
        description: Select the daily discoveries or weekly summary
        type: choice
        options: [weekly, daily]
        default: weekly
      dry_run:''')
edit(WORKFLOW, "          DIGEST_DELIVERY_ENABLED: ${{ vars.DIGEST_DELIVERY_ENABLED }}", "          DIGEST_DELIVERY_ENABLED: ${{ vars.DIGEST_DELIVERY_ENABLED }}\n          DIGEST_DAILY_DELIVERY_ENABLED: ${{ vars.DIGEST_DAILY_DELIVERY_ENABLED }}", count=2)
edit(WORKFLOW, "          MANUAL_DRY_RUN: ${{ inputs.dry_run }}", "          MANUAL_DRY_RUN: ${{ inputs.dry_run }}\n          DIGEST_FORMAT: ${{ inputs.digest_format }}")
edit(WORKFLOW, '''          elif [[ "$RUN_MODE" == "digest" ]]; then
            "$bot" digest --llm''', '''          elif [[ "$RUN_MODE" == "daily-digest" ]]; then
            "$bot" digest --daily --llm
          elif [[ "$RUN_MODE" == "daily-preview" ]]; then
            args=(preview --daily --dry-run)
            if [[ "$USE_LLM" == "true" ]]; then args+=(--llm); fi
            "$bot" "${args[@]}"
          elif [[ "$RUN_MODE" == "digest" ]]; then
            "$bot" digest --llm''')
edit(WORKFLOW, "        if: steps.mode.outputs.mode == 'preview' || steps.mode.outputs.mode == 'digest'", "        if: steps.mode.outputs.mode == 'preview' || steps.mode.outputs.mode == 'digest' || steps.mode.outputs.mode == 'daily-preview' || steps.mode.outputs.mode == 'daily-digest'")
edit(WORKFLOW, '''        run: python3 .github/scripts/weekly_run.py verify''', '''          RUN_MODE: ${{ steps.mode.outputs.mode }}
        run: |
          if [[ "$RUN_MODE" == daily-* ]]; then
            python3 .github/scripts/daily_run.py verify
          else
            python3 .github/scripts/weekly_run.py verify
          fi''')
edit(WORKFLOW, '''        run: python3 .github/scripts/weekly_run.py record''', '''        run: |
          if [[ "$RUN_MODE" == daily-* ]]; then
            python3 .github/scripts/daily_run.py record
          else
            python3 .github/scripts/weekly_run.py record
          fi''')
p = Path("README.md")
s = p.read_text()
first, rest = s.split("\n", 1)
p.write_text(first + "\n\n> Daily and weekly editions now share a durable discovery queue, not a disposable seven-day selection. See [edition semantics and rollout](docs/daily-weekly-queue.md). Telegram gates remain opt-in; daily additionally requires `DIGEST_DAILY_DELIVERY_ENABLED=true`.\n\n" + rest)
Path(".github/scripts/apply_retained_change.py").unlink()
Path(".github/workflows/prepare-retained-change.yml").unlink()
