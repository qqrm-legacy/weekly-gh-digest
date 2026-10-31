//! Durable discovery queues. Observations are never consumed by a preview or a cap.
//! Each format acknowledges only names actually included in a completed delivery.
use crate::{Ranked, Snapshot, State, parse_time, schedule};
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub(super) const DAILY_LIMIT: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Format {
    Daily,
    Weekly,
}

impl Format {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
        }
    }

    pub(super) fn slot(self, now: DateTime<Utc>) -> String {
        match self {
            Self::Weekly => crate::slot_key_of(now),
            Self::Daily => format!("daily:{}", (now - Duration::hours(7)).date_naive()),
        }
    }

    pub(super) fn from_digest(digest: &Value) -> Result<Self> {
        match digest.get("edition") {
            None => Ok(Self::Weekly),
            Some(Value::String(value)) if value == "weekly" => Ok(Self::Weekly),
            Some(Value::String(value)) if value == "daily" => Ok(Self::Daily),
            Some(_) => anyhow::bail!("unknown digest edition; refusing delivery"),
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Ledger {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    pub(super) daily_covered: BTreeMap<String, String>,
    #[serde(default)]
    analyses: BTreeMap<String, Analysis>,
}

#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Analysis {
    source_description: Option<String>,
    source_readme: Option<String>,
    pub(super) description: String,
    pub(super) joke: String,
    pub(super) evidence_quote: String,
}

/// Prior weekly publications predate the daily feed. Seed that feed once only.
pub(super) fn migrate(state: &mut State) -> Result<()> {
    ensure!(
        state.editions.version <= 1,
        "unsupported discovery ledger version"
    );
    if state.editions.version == 0 {
        for (name, date) in &state.covered {
            state
                .editions
                .daily_covered
                .entry(name.to_ascii_lowercase())
                .or_insert_with(|| date.clone());
        }
        state.editions.version = 1;
    }
    Ok(())
}

pub(super) fn was_published(state: &State, format: Format, name: &str) -> bool {
    let covered = match format {
        Format::Weekly => &state.covered,
        Format::Daily => &state.editions.daily_covered,
    };
    covered.keys().any(|key| key.eq_ignore_ascii_case(name))
}

/// This is a persistent queue, not a rolling seven-day eligibility filter.
/// The raw snapshots are its durable records; the format's receipts are its ACKs.
pub(super) fn pending(state: &State, now: DateTime<Utc>, format: Format) -> Vec<Snapshot> {
    schedule::all_candidates(state, now)
        .into_iter()
        .filter(|row| !was_published(state, format, &row.full_name))
        .collect()
}

fn first_seen(snapshot: &Snapshot) -> &str {
    snapshot
        .observation
        .as_ref()
        .map(|window| window.first_seen.as_str())
        .unwrap_or(&snapshot.captured_at)
}

pub(super) fn priority(snapshot: &Snapshot, format: Format) -> (u8, i64, &'static str) {
    if format == Format::Daily {
        daily_priority(snapshot)
    } else {
        crate::source_priority(snapshot)
    }
}

fn daily_priority(snapshot: &Snapshot) -> (u8, i64, &'static str) {
    for (priority, kind) in [(0, "trending_daily"), (1, "trending_weekly")] {
        if let Some(metric) = snapshot
            .sources
            .iter()
            .find(|source| source["type"] == kind)
            .and_then(|source| source["trending_stars_period"].as_i64())
        {
            return (priority, metric, kind);
        }
    }
    (2, snapshot.stars.unwrap_or(0), "new_repo")
}

pub(super) fn select(
    current: &[Snapshot],
    state: &State,
    format: Format,
) -> (Vec<Ranked>, Vec<Value>) {
    if format == Format::Weekly {
        return crate::select_candidates(current, &state.covered);
    }
    let mut pool: Vec<_> = current
        .iter()
        .filter(|row| !was_published(state, format, &row.full_name))
        .collect();
    // Oldest discovery date first prevents a sustained flood of new projects
    // from starving the backlog. Within a date, prefer daily Trending evidence.
    pool.sort_by(|a, b| {
        let a_day = first_seen(a).get(..10).unwrap_or(first_seen(a));
        let b_day = first_seen(b).get(..10).unwrap_or(first_seen(b));
        let pa = daily_priority(a);
        let pb = daily_priority(b);
        a_day
            .cmp(b_day)
            .then(pa.0.cmp(&pb.0))
            .then(pb.1.cmp(&pa.1))
            .then(a.full_name.cmp(&b.full_name))
    });
    let excluded = pool
        .iter()
        .skip(DAILY_LIMIT)
        .map(|row| {
            json!({
                "full_name": row.full_name, "reason": "daily capacity; retained in pending queue"
            })
        })
        .collect();
    let entries = pool
        .into_iter()
        .take(DAILY_LIMIT)
        .map(|snapshot| {
            let (_, delta, kind) = daily_priority(snapshot);
            Ranked {
                snapshot: snapshot.clone(),
                baseline: None,
                delta,
                period: crate::source_period(kind),
                trending_source: kind != "new_repo",
            }
        })
        .collect();
    (entries, excluded)
}

pub(super) fn cached_analysis(state: &State, snapshot: &Snapshot) -> Option<Analysis> {
    state
        .editions
        .analyses
        .get(&snapshot.full_name.to_ascii_lowercase())
        .filter(|analysis| {
            analysis.source_description == snapshot.description
                && analysis.source_readme == snapshot.readme_excerpt
        })
        .cloned()
}

pub(super) fn remember_analyses(state: &mut State, digest: &Value) {
    for entry in digest["entries"].as_array().into_iter().flatten() {
        if entry["llm_used"] != true {
            continue;
        }
        let Some(name) = entry["full_name"].as_str() else {
            continue;
        };
        state.editions.analyses.insert(
            name.to_ascii_lowercase(),
            Analysis {
                source_description: entry["latest"]["description"].as_str().map(str::to_owned),
                source_readme: entry["latest"]["readme_excerpt"]
                    .as_str()
                    .map(str::to_owned),
                description: entry["description"].as_str().unwrap_or("").to_string(),
                joke: entry["joke"].as_str().unwrap_or("").to_string(),
                evidence_quote: entry["evidence_quote"].as_str().unwrap_or("").to_string(),
            },
        );
    }
}

/// Freeze the entire weekly index alongside the capped detailed cards.
/// Every name in the index is also rendered into the actual Telegram payload.
pub(super) fn decorate(
    digest: &mut Value,
    current: &[Snapshot],
    now: DateTime<Utc>,
    format: Format,
    directory: &Path,
) -> Result<()> {
    let detailed: BTreeSet<_> = digest["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry["full_name"].as_str())
        .map(str::to_ascii_lowercase)
        .collect();
    let catalogue: Vec<Value> = if format == Format::Weekly {
        current.iter().filter(|row| !detailed.contains(&row.full_name.to_ascii_lowercase()))
            .map(|row| json!({"full_name":row.full_name,"first_seen":first_seen(row),
                "description":crate::safe_public_text(row.description.as_deref().unwrap_or("")).chars().take(180).collect::<String>(),
                "metadata_captured_at":row.captured_at})).collect()
    } else {
        Vec::new()
    };
    let days = if format == Format::Daily { 1 } else { 7 };
    let carried = current
        .iter()
        .filter(|row| {
            parse_time(first_seen(row)).is_some_and(|time| time < now - Duration::days(days))
        })
        .count();
    digest["edition"] = json!(format.name());
    digest["delivery_slot"] = json!(format.slot(now));
    digest["period_start"] =
        json!((now - Duration::days(days)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true));
    digest["catalogue"] = json!(catalogue);
    digest["queue"] = json!({"pending_before":current.len(),"detailed":detailed.len(),
        "catalogue":catalogue.len(),"carried_from_earlier_periods":carried,
        "pending_after_success":current.len() - detailed.len() - catalogue.len(),
        "ack_policy":"Only completed delivery acknowledges this format. Previews, failures and caps acknowledge nothing."});
    digest["ranking"] = json!(if format == Format::Daily {
        "Persistent discovery queue: oldest first-seen date first, then daily Trending peak, weekly peak, Search. Daily publication does not exclude weekly publication. Capacity overflow stays queued."
    } else {
        "Persistent weekly queue including daily discoveries and undelivered backlog. Up to 30 detailed cards ranked by source peaks; every remaining candidate appears in the published index. No dependency on still appearing in Trending."
    });
    digest["trend_summary"] = json!(if format == Format::Daily {
        "Находки дня из сохранённых наблюдений и недоставленного остатка. Сначала старейшая дата обнаружения, внутри даты — дневной Trending. Новые для нашей ленты, не обязательно недавно созданные. Дневная публикация не исключает проект из недельного обзора."
    } else {
        "Недельный обзор накопленных находок, включая дневные выпуски и недоставленный остаток прошлых периодов. Подробные карточки дополнены полным указателем остальных находок. Метрики Trending — максимальные наблюдавшиеся значения своих скользящих периодов, а не сумма срезов или измеренный прирост за календарную неделю."
    });
    let path = directory.join("selection.json");
    let mut selection: Value = serde_json::from_slice(&fs::read(&path)?)?;
    selection["catalogue"] = json!(catalogue);
    crate::write_json(&path, &selection)?;
    crate::write_json(
        &directory.join("queue.json"),
        &json!({"edition":format.name(),"summary":digest["queue"],"pending":current}),
    )?;
    Ok(())
}

pub(super) fn validate_catalogue(digest: &Value, selection: &Value) -> Result<()> {
    let empty = Vec::new();
    let catalogue = digest
        .get("catalogue")
        .map(|value| value.as_array().context("invalid catalogue"))
        .transpose()?
        .unwrap_or(&empty);
    if selection.get("catalogue").is_some() || digest.get("catalogue").is_some() {
        ensure!(
            selection.get("catalogue") == digest.get("catalogue"),
            "catalogue differs from recorded selection"
        );
    }
    if !catalogue.is_empty() {
        ensure!(
            Format::from_digest(digest)? == Format::Weekly,
            "daily issue cannot contain a weekly index"
        );
    }
    let mut names = BTreeSet::new();
    for entry in digest["entries"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(catalogue)
    {
        let name = entry["full_name"]
            .as_str()
            .context("entry has no repository name")?;
        ensure!(
            crate::project_github_url(name).is_some(),
            "invalid repository name"
        );
        ensure!(
            names.insert(name.to_ascii_lowercase()),
            "duplicate repository in issue"
        );
    }
    Ok(())
}

pub(super) fn append_catalogue(chunks: &mut Vec<String>, digest: &Value) -> Result<()> {
    let Some(catalogue) = digest["catalogue"]
        .as_array()
        .filter(|items| !items.is_empty())
    else {
        return Ok(());
    };
    let header = "*Остальные накопленные находки*";
    let mut current = header.to_string();
    for entry in catalogue {
        let name = entry["full_name"]
            .as_str()
            .context("catalogue entry without name")?;
        let url = crate::project_github_url(name).context("invalid catalogue repository")?;
        let item = format!(
            "[{}]({}) — {}\n{}",
            crate::markdown_v2_escape(name),
            crate::markdown_v2_url_escape(&url),
            crate::markdown_v2_escape(entry["description"].as_str().unwrap_or("")),
            crate::markdown_v2_escape(&format!(
                "Впервые замечен: {}",
                entry["first_seen"].as_str().unwrap_or("неизвестно")
            ))
        );
        ensure!(
            header.encode_utf16().count() + 2 + item.encode_utf16().count()
                <= crate::ISSUE_CHUNK_LIMIT,
            "catalogue item exceeds Telegram limit"
        );
        if current.encode_utf16().count() + 2 + item.encode_utf16().count()
            > crate::ISSUE_CHUNK_LIMIT
        {
            chunks.push(current);
            current = header.to_string();
        }
        current.push_str("\n\n");
        current.push_str(&item);
    }
    chunks.push(current);
    Ok(())
}

pub(super) fn catalogue_markdown(digest: &Value) -> String {
    let mut text = String::new();
    if let Some(queue) = digest.get("queue") {
        text.push_str(&format!("\nОчередь до выпуска: {}; подробных карточек: {}; остальных ссылок: {}; находок из более ранних периодов: {}.\n",
            queue["pending_before"],queue["detailed"],queue["catalogue"],queue["carried_from_earlier_periods"]));
    }
    if let Some(catalogue) = digest["catalogue"]
        .as_array()
        .filter(|items| !items.is_empty())
    {
        text.push_str("\n## Остальные накопленные находки\n");
        for entry in catalogue {
            let Some(name) = entry["full_name"].as_str() else {
                continue;
            };
            if let Some(url) = crate::project_github_url(name) {
                text.push_str(&format!(
                    "\n- [{name}]({url}) — {}. Впервые замечен: {}.\n",
                    entry["description"].as_str().unwrap_or(""),
                    entry["first_seen"].as_str().unwrap_or("?")
                ));
            }
        }
    }
    text
}

/// Called only after every card, index chunk and final overview was acknowledged.
pub(super) fn acknowledge(state: &mut State, digest: &Value, day: &str) -> Result<()> {
    let format = Format::from_digest(digest)?;
    let entries = digest["entries"]
        .as_array()
        .context("digest has no entries")?;
    let mut names: Vec<_> = entries
        .iter()
        .filter_map(|entry| entry["full_name"].as_str())
        .collect();
    if format == Format::Weekly {
        names.extend(
            digest["catalogue"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|entry| entry["full_name"].as_str()),
        );
    }
    let covered = match format {
        Format::Daily => &mut state.editions.daily_covered,
        Format::Weekly => &mut state.covered,
    };
    for name in names {
        covered
            .entry(name.to_ascii_lowercase())
            .or_insert_with(|| day.to_string());
    }
    Ok(())
}

/// Frozen pending payloads survive date/week changes. Never trim or regenerate them.
pub(super) fn pending_issues(
    state: &State,
    now: DateTime<Utc>,
    format: Format,
) -> Result<Vec<(String, Value, String)>> {
    let mut issues = Vec::new();
    for (id, digest) in &state.pending_digests {
        let stored_format = Format::from_digest(digest)?;
        if stored_format != format {
            continue;
        }
        let generated = digest["generated_at"]
            .as_str()
            .and_then(parse_time)
            .context("pending issue has no valid timestamp")?;
        ensure!(generated <= now, "pending issue is dated in the future");
        let slot = stored_format.slot(generated);
        ensure!(
            digest["delivery_slot"].as_str() == Some(slot.as_str()),
            "pending issue has inconsistent identity; reconciliation required"
        );
        issues.push((id.clone(), digest.clone(), slot));
    }
    Ok(issues)
}

pub(super) fn resume(state: &mut State, now: DateTime<Utc>, format: Format) -> Result<()> {
    for (id, digest, slot) in pending_issues(state, now, format)? {
        if state.sent_slots.contains_key(&slot) {
            state.pending_digests.remove(&id);
            crate::write_json(&crate::state_path(), state)?;
            continue;
        }
        ensure!(
            crate::publication_audit(&digest)?["approved"] == true,
            "stored delivery failed credential scan"
        );
        let generated = digest["generated_at"]
            .as_str()
            .and_then(parse_time)
            .context("invalid pending timestamp")?;
        crate::telegram_deliver_digest(
            &digest,
            &id,
            &generated.date_naive().to_string(),
            &slot,
            state,
        )?;
    }
    Ok(())
}

/// Explicit read-only replay. No HTTP clients, provider calls, state writes or sends.
pub(super) fn preview_retained(args: &[String]) -> Result<()> {
    let source = args.get(1).context(
        "usage: preview-retained <state.json> <new-output-dir> [--daily] [--at=RFC3339]",
    )?;
    let directory = Path::new(args.get(2).context("missing output directory")?);
    ensure!(!directory.exists(), "preview output directory must be new");
    let now = match args.iter().find_map(|arg| arg.strip_prefix("--at=")) {
        Some(value) => parse_time(value).context("invalid --at timestamp")?,
        None => Utc::now(),
    };
    let format = if args.iter().any(|arg| arg == "--daily") {
        Format::Daily
    } else {
        Format::Weekly
    };
    let mut state: State = serde_json::from_slice(&fs::read(source)?)?;
    migrate(&mut state)?;
    let candidates = pending(&state, now, format);
    let (selected, excluded) = select(&candidates, &state, format);
    let entries: Vec<_> = selected.iter().enumerate().map(|(index, entry)| {
        let snapshot = &entry.snapshot;
        let cached = cached_analysis(&state, snapshot);
        let kind = if format == Format::Daily { daily_priority(snapshot).2 } else { crate::source_priority(snapshot).2 };
        json!({"rank":index+1,"total":selected.len(),"full_name":snapshot.full_name,
            "latest":snapshot,"delta":entry.delta,"period":entry.period,"source_kind":kind,
            "metric_captured_at":snapshot.sources.iter().find(|source| source["type"] == kind).and_then(|source| source["captured_at"].as_str()).unwrap_or(&snapshot.captured_at),
            "description":cached.as_ref().map(|value| value.description.clone()).unwrap_or_else(|| crate::safe_public_text(snapshot.description.as_deref().unwrap_or("Описание не указано в сохранённых источниках."))),
            "joke":cached.as_ref().map(|value| value.joke.as_str()).unwrap_or(""),
            "evidence_quote":cached.as_ref().map(|value| value.evidence_quote.as_str()).unwrap_or(""),
            "llm_used":cached.is_some()})
    }).collect();
    let mut digest = json!({"format_version":3,"edition":format.name(),"generated_at":now.to_rfc3339_opts(chrono::SecondsFormat::Secs,true),
        "delivery_slot":format.slot(now),"period_end":now.to_rfc3339_opts(chrono::SecondsFormat::Secs,true),
        "entries":entries,"overview":"","llm_used":false,"preview_kind":"retained-offline",
        "llm_budget":{"reason":"Offline preview: saved source descriptions only; no new provider requests"},
        "manifest":{"sources":[],"note":"Read-only replay of retained state; no source collection or delivery"}});
    fs::create_dir_all(directory)?;
    crate::write_json(
        &directory.join("selection.json"),
        &json!({"pool":entries,"excluded":excluded}),
    )?;
    decorate(&mut digest, &candidates, now, format, directory)?;
    crate::save_preview_files(directory, &digest, false)?;
    println!(
        "{} preview: {} detailed, {} index entries; state unchanged; Telegram not contacted",
        format.name(),
        selected.len(),
        digest["catalogue"].as_array().map_or(0, Vec::len)
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(value: &str) -> DateTime<Utc> {
        parse_time(value).unwrap()
    }
    fn row(name: &str, at: &str, metric: i64) -> Snapshot {
        serde_json::from_value(json!({"full_name":name,"captured_at":at,"stars":metric,
            "description":"A retained source description for this project.",
            "sources":[{"type":"trending_daily","trending_stars_period":metric}]}))
        .unwrap()
    }
    fn fixture(count: usize) -> State {
        let mut state = State::default();
        migrate(&mut state).unwrap();
        for i in 0..count {
            schedule::record(
                &mut state,
                &[row(
                    &format!("org/p{i:03}"),
                    "2026-10-13T00:17:00Z",
                    i as i64,
                )],
            );
        }
        state
    }
    fn issue(names: Vec<String>, format: Format) -> Value {
        json!({"edition":format.name(),"generated_at":"2026-10-13T08:00:00Z","delivery_slot":format.slot(time("2026-10-13T08:00:00Z")),
            "entries":names.into_iter().map(|name| json!({"full_name":name})).collect::<Vec<_>>()})
    }

    #[test]
    fn transient_discovery_survives_long_outage_and_state_roundtrip() {
        let state = fixture(1);
        let restored: State = serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        let now = time("2026-11-30T08:00:00Z");
        assert!(schedule::candidates(&restored, now).is_empty());
        assert_eq!(pending(&restored, now, Format::Daily).len(), 1);
        assert_eq!(pending(&restored, now, Format::Weekly).len(), 1);
    }

    #[test]
    fn daily_ack_does_not_consume_weekly_and_weekly_ack_does_not_consume_daily() {
        let mut state = fixture(2);
        acknowledge(
            &mut state,
            &issue(vec!["org/p000".into()], Format::Daily),
            "2026-10-13",
        )
        .unwrap();
        acknowledge(
            &mut state,
            &issue(vec!["org/p001".into()], Format::Weekly),
            "2026-10-19",
        )
        .unwrap();
        let now = time("2026-10-20T08:00:00Z");
        assert_eq!(pending(&state, now, Format::Daily)[0].full_name, "org/p001");
        assert_eq!(
            pending(&state, now, Format::Weekly)[0].full_name,
            "org/p000"
        );
    }

    #[test]
    fn capped_queue_drains_only_acknowledged_names_without_starvation() {
        let mut state = fixture(45);
        let now = time("2026-10-31T08:00:00Z");
        let mut seen = BTreeSet::new();
        for expected in [45, 33, 21, 9] {
            let candidates = pending(&state, now, Format::Daily);
            assert_eq!(candidates.len(), expected);
            let before = serde_json::to_value(&state).unwrap();
            let (selected, _) = select(&candidates, &state, Format::Daily);
            assert_eq!(serde_json::to_value(&state).unwrap(), before);
            let names: Vec<_> = selected
                .iter()
                .map(|entry| entry.snapshot.full_name.clone())
                .collect();
            for name in &names {
                assert!(seen.insert(name.clone()));
            }
            acknowledge(&mut state, &issue(names, Format::Daily), "2026-10-31").unwrap();
        }
        assert_eq!(seen.len(), 45);
        assert!(pending(&state, now, Format::Daily).is_empty());
        assert_eq!(pending(&state, now, Format::Weekly).len(), 45);
    }

    #[test]
    fn old_backlog_precedes_fresh_popularity_and_case_changes_do_not_duplicate() {
        let mut state = fixture(1);
        schedule::record(
            &mut state,
            &[
                row("ORG/P000", "2026-10-14T00:17:00Z", 50),
                row("org/fresh", "2026-10-30T00:17:00Z", 99999),
            ],
        );
        let candidates = pending(&state, time("2026-10-31T08:00:00Z"), Format::Daily);
        assert_eq!(candidates.len(), 2);
        let (selected, _) = select(&candidates, &state, Format::Daily);
        assert!(
            selected[0]
                .snapshot
                .full_name
                .eq_ignore_ascii_case("org/p000")
        );
        acknowledge(
            &mut state,
            &issue(vec!["org/p000".into()], Format::Daily),
            "2026-10-31",
        )
        .unwrap();
        assert_eq!(
            pending(&state, time("2026-10-31T08:00:00Z"), Format::Daily).len(),
            1
        );
    }

    #[test]
    fn legacy_coverage_migration_happens_once() {
        let mut state: State =
            serde_json::from_value(json!({"covered":{"Org/Legacy":"2026-10-01"}})).unwrap();
        migrate(&mut state).unwrap();
        assert!(was_published(&state, Format::Daily, "org/legacy"));
        state
            .covered
            .insert("org/after-migration".into(), "2026-10-20".into());
        migrate(&mut state).unwrap();
        assert!(!was_published(&state, Format::Daily, "org/after-migration"));
    }

    #[test]
    fn daily_and_weekly_slot_boundaries_are_independent() {
        assert_eq!(
            Format::Daily.slot(time("2027-01-01T06:59:59Z")),
            "daily:2026-12-31"
        );
        assert_eq!(
            Format::Daily.slot(time("2027-01-01T07:00:00Z")),
            "daily:2027-01-01"
        );
        assert_eq!(
            Format::Weekly.slot(time("2026-10-26T07:00:00Z")),
            "weekly:2026-10-26"
        );
    }

    #[test]
    fn pending_issue_is_frozen_across_week_changes_and_never_expires() {
        let mut state = fixture(1);
        let payload = issue(vec!["org/p000".into()], Format::Daily);
        state
            .pending_digests
            .insert("daily-frozen".into(), payload.clone());
        state.pending_digests.insert(
            "weekly-frozen".into(),
            issue(vec!["org/p000".into()], Format::Weekly),
        );
        let pending = pending_issues(&state, time("2027-01-01T07:00:00Z"), Format::Daily).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1, payload);
        assert_eq!(state.pending_digests.len(), 2);
    }

    #[test]
    fn weekly_index_is_published_chunked_validated_and_acknowledged() {
        let mut state = fixture(85);
        let now = time("2026-10-31T08:00:00Z");
        let candidates = pending(&state, now, Format::Weekly);
        let (selected, _) = select(&candidates, &state, Format::Weekly);
        let mut digest = issue(
            selected
                .iter()
                .map(|entry| entry.snapshot.full_name.clone())
                .collect(),
            Format::Weekly,
        );
        let directory = tempfile::tempdir().unwrap();
        crate::write_json(
            &directory.path().join("selection.json"),
            &json!({"pool":digest["entries"]}),
        )
        .unwrap();
        decorate(
            &mut digest,
            &candidates,
            now,
            Format::Weekly,
            directory.path(),
        )
        .unwrap();
        assert_eq!(digest["catalogue"].as_array().unwrap().len(), 55);
        let selection: Value =
            serde_json::from_slice(&fs::read(directory.path().join("selection.json")).unwrap())
                .unwrap();
        validate_catalogue(&digest, &selection).unwrap();
        let mut chunks = Vec::new();
        append_catalogue(&mut chunks, &digest).unwrap();
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            crate::telegram_format::validate_message(chunk, crate::ISSUE_CHUNK_LIMIT).unwrap();
        }
        for entry in digest["catalogue"].as_array().unwrap() {
            let link = crate::project_github_url(entry["full_name"].as_str().unwrap()).unwrap();
            assert_eq!(
                chunks.iter().filter(|chunk| chunk.contains(&link)).count(),
                1
            );
        }
        assert_eq!(pending(&state, now, Format::Weekly).len(), 85);
        acknowledge(&mut state, &digest, "2026-10-31").unwrap();
        assert!(pending(&state, now, Format::Weekly).is_empty());
        assert_eq!(pending(&state, now, Format::Daily).len(), 85);
        digest["catalogue"][0]["full_name"] = json!("org/injected");
        assert!(validate_catalogue(&digest, &selection).is_err());
    }

    #[test]
    fn analysis_cache_requires_exact_primary_sources() {
        let mut state = fixture(1);
        let mut snapshot = state.snapshots[0].clone();
        snapshot.readme_excerpt = Some("Documented original README evidence.".into());
        remember_analyses(
            &mut state,
            &json!({"entries":[{"full_name":snapshot.full_name,"latest":snapshot,
            "llm_used":true,"description":"Saved analysis","joke":"","evidence_quote":"original README"}]}),
        );
        assert_eq!(
            cached_analysis(&state, &snapshot).unwrap().description,
            "Saved analysis"
        );
        snapshot.readme_excerpt = Some("Changed README".into());
        assert!(cached_analysis(&state, &snapshot).is_none());
    }

    #[test]
    fn uncertain_delivery_does_not_delete_or_hide_discoveries() {
        let mut state = fixture(1);
        state.uncertain_delivery = Some("chat/issue/chunk-0".into());
        schedule::record(&mut state, &[row("org/new", "2026-10-30T00:17:00Z", 30)]);
        assert_eq!(
            pending(&state, time("2026-10-31T08:00:00Z"), Format::Daily).len(),
            2
        );
        assert!(
            crate::delivery::send_chunks(
                &["text".into()],
                "issue",
                &mut state,
                |_| panic!("uncertain send"),
                |_| Ok(())
            )
            .is_err()
        );
    }
}
