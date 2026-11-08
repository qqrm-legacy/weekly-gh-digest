mod checkpoint;
mod clock;
mod coverage;
mod delivery;
mod editions;
mod editorial;
mod providers;
mod schedule;
mod sources;
mod state_store;
mod telegram_format;
mod test_delivery;
mod test_delivery_review;
#[cfg(test)]
mod test_support;

use anyhow::{Context, Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration as StdDuration;

/// Selection is bounded only after the whole weekly observation union is built.
const MAX_DIGEST_ENTRIES: usize = 30;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    full_name: String,
    captured_at: String,
    stars: Option<i64>,
    forks: Option<i64>,
    language: Option<String>,
    description: Option<String>,
    html_url: Option<String>,
    #[serde(default)]
    topics: Vec<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    pushed_at: Option<String>,
    license: Option<String>,
    open_issues: Option<i64>,
    default_branch: Option<String>,
    #[serde(default)]
    sources: Vec<Value>,
    #[serde(default)]
    readme_excerpt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    observation: Option<schedule::ObservationWindow>,
}

#[derive(Default, Serialize, Deserialize)]
struct State {
    #[serde(default = "schema_one")]
    schema: u32,
    #[serde(default)]
    editions: editions::Ledger,
    #[serde(default)]
    snapshots: Vec<Snapshot>,
    #[serde(default)]
    covered: BTreeMap<String, String>,
    #[serde(default)]
    sent_digests: BTreeMap<String, i64>,
    /// One issue per weekly publication period. Legacy slot keys are preserved.
    #[serde(default)]
    sent_slots: BTreeMap<String, i64>,
    #[serde(default)]
    pending_digests: BTreeMap<String, Value>,
    #[serde(default)]
    llm_daily_requests: BTreeMap<String, usize>,
    #[serde(default)]
    delivery_progress: BTreeMap<String, DeliveryProgress>,
    /// Legacy summary receipts, retained for compatibility with saved state.
    #[serde(default)]
    overview_ids: BTreeMap<String, i64>,
    #[serde(default)]
    uncertain_delivery: Option<String>,
}

#[derive(Default, Serialize, Deserialize)]
struct DeliveryProgress {
    #[serde(default)]
    project_message_ids: BTreeMap<String, i64>,
    #[serde(default)]
    overview_message_id: Option<i64>,
    #[serde(default)]
    message_texts: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct DebugDelivery {
    day: String,
    chat_id: String,
    posts: BTreeMap<String, i64>,
    overview_message_id: i64,
}

fn schema_one() -> u32 {
    1
}

#[derive(Clone)]
struct Ranked {
    snapshot: Snapshot,
    baseline: Option<Snapshot>,
    delta: i64,
    period: String,
    trending_source: bool,
}

fn root() -> PathBuf {
    PathBuf::from(".")
}
fn data_dir() -> PathBuf {
    root().join("data")
}
fn state_path() -> PathBuf {
    data_dir().join("state.json")
}
fn runs_dir() -> PathBuf {
    data_dir().join("runs")
}
fn stamp(now: DateTime<Utc>) -> String {
    now.format("%Y%m%dT%H%M%SZ").to_string()
}

fn record_run_directory(directory: &Path) -> Result<()> {
    if let Some(output) = env::var_os("GITHUB_OUTPUT") {
        use std::io::Write;
        let mut file = fs::OpenOptions::new().append(true).open(output)?;
        writeln!(file, "run_directory={}", directory.display())?;
    }
    Ok(())
}

fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    state_store::StateStore::new(path).save(value)?;
    if path == state_path() {
        checkpoint::save(path)?;
    }
    Ok(())
}

/// Strict read: a state file that exists but cannot be parsed must abort
/// the run instead of silently starting from empty memory and reposting
/// already published projects.
fn read_state_strict() -> Result<State> {
    let mut state: State = state_store::StateStore::new(&state_path()).load()?;
    editions::migrate(&mut state)?;
    // Collection and previews may continue during reconciliation. The sender
    // independently refuses every side effect while delivery is uncertain.
    Ok(state)
}

fn client() -> Result<Client> {
    Ok(Client::builder()
        .timeout(StdDuration::from_secs(30))
        .user_agent("github-weekly-digest-bot/0.2")
        .build()?)
}

fn update_state(new: &[Snapshot]) -> Result<State> {
    let mut state = read_state_strict()?;
    schedule::record(&mut state, new);
    write_json(&state_path(), &state)?;
    Ok(state)
}

fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

fn rank(state: &State, now: DateTime<Utc>, metric: &str) -> Vec<Ranked> {
    let mut groups: BTreeMap<String, Vec<Snapshot>> = BTreeMap::new();
    for s in &state.snapshots {
        if parse_time(&s.captured_at).is_some_and(|t| t <= now) {
            groups
                .entry(s.full_name.clone())
                .or_default()
                .push(s.clone());
        }
    }
    let cutoff = now - Duration::days(7);
    let mut out = Vec::new();
    for (_, mut rows) in groups {
        rows.sort_by(|a, b| a.captured_at.cmp(&b.captured_at));
        let latest = rows.last().unwrap().clone();
        let baseline = rows
            .iter()
            .find(|s| parse_time(&s.captured_at).is_some_and(|t| t <= cutoff))
            .cloned();
        let start = baseline.clone().unwrap_or_else(|| rows[0].clone());
        if rows.len() < 2 || latest.captured_at == start.captured_at {
            continue;
        }
        let field = if metric == "forks" { "forks" } else { "stars" };
        let (Some(a), Some(b)) = (latest_metric(&latest, field), latest_metric(&start, field))
        else {
            continue;
        };
        if metric == "percent" && b < 100 {
            continue;
        }
        let days =
            (now - parse_time(&start.captured_at).unwrap_or(now)).num_seconds() as f64 / 86400.0;
        let period = if baseline.is_some() {
            format!("{days:.1} дн.")
        } else {
            format!("доступно {days:.1} дн.")
        };
        out.push(Ranked {
            snapshot: latest,
            baseline,
            delta: a - b,
            period,
            trending_source: false,
        });
    }
    out.sort_by(|a, b| {
        b.delta
            .cmp(&a.delta)
            .then(a.snapshot.full_name.cmp(&b.snapshot.full_name))
    });
    out
}

fn latest_metric(s: &Snapshot, field: &str) -> Option<i64> {
    if field == "forks" { s.forks } else { s.stars }
}

fn source_priority(snapshot: &Snapshot) -> (u8, i64, &'static str) {
    if let Some(delta) = snapshot
        .sources
        .iter()
        .find(|source| source["type"] == "trending_weekly")
        .and_then(|source| source["trending_stars_period"].as_i64())
    {
        return (0, delta, "trending_weekly");
    }
    if let Some(delta) = snapshot
        .sources
        .iter()
        .find(|source| source["type"] == "trending_daily")
        .and_then(|source| source["trending_stars_period"].as_i64())
    {
        return (1, delta, "trending_daily");
    }
    (2, snapshot.stars.unwrap_or(0), "new_repo")
}

fn source_period(kind: &str) -> String {
    match kind {
        "trending_weekly" => "GitHub Trending: максимум наблюдавшихся звёзд за неделю".into(),
        "trending_daily" => "GitHub Trending: максимум наблюдавшихся звёзд за день".into(),
        _ => "новый репозиторий: звёзды на момент среза".into(),
    }
}

/// Picks the issue candidates from the current run's snapshots.
/// Repositories listed in `covered` were reviewed in an earlier issue and
/// never reappear. Total stars do not determine eligibility.
/// Ordering is stable: weekly Trending +N desc, then daily Trending +N
/// desc, then new repositories by total stars.
fn select_candidates(
    current: &[Snapshot],
    covered: &BTreeMap<String, String>,
) -> (Vec<Ranked>, Vec<Value>) {
    let mut pool: Vec<&Snapshot> = Vec::new();
    let mut excluded = Vec::<Value>::new();
    for snapshot in current {
        let (_, _, kind) = source_priority(snapshot);
        if let Some(day) = covered.get(&snapshot.full_name) {
            excluded.push(
                json!({"full_name":snapshot.full_name,"source_kind":kind,"reason":format!("already reviewed in the digest on {day}")}),
            );
            continue;
        }
        pool.push(snapshot);
    }
    pool.sort_by(|a, b| {
        let ka = source_priority(a);
        let kb = source_priority(b);
        ka.0.cmp(&kb.0)
            .then(kb.1.cmp(&ka.1))
            .then(a.full_name.cmp(&b.full_name))
    });
    let overflow: Vec<Value> = pool
        .iter()
        .skip(MAX_DIGEST_ENTRIES)
        .map(|snapshot| {
            json!({"full_name":snapshot.full_name,"reason":"digest cap after stable source priority"})
        })
        .collect();
    let entries = pool
        .iter()
        .take(MAX_DIGEST_ENTRIES)
        .map(|snapshot| {
            let (_, delta, kind) = source_priority(snapshot);
            Ranked {
                snapshot: (*snapshot).clone(),
                baseline: None,
                delta,
                period: source_period(kind),
                trending_source: kind != "new_repo",
            }
        })
        .collect();
    (entries, excluded.into_iter().chain(overflow).collect())
}

/// Re-checks a stored digest against the selection pool that its run
/// recorded from the raw captured pages, so a delivered artifact can never
/// contain candidates that the deterministic selection did not choose.
fn validate_digest_selection(run_dir: &Path, digest: &Value) -> Result<()> {
    let selection: Value = serde_json::from_slice(&fs::read(run_dir.join("selection.json"))?)
        .context("run directory has no recorded selection pool")?;
    let pool = selection
        .get("pool")
        .unwrap_or(&selection)
        .as_array()
        .ok_or_else(|| anyhow!("invalid recorded selection pool"))?;
    let entries = digest["entries"]
        .as_array()
        .ok_or_else(|| anyhow!("digest has no ranked entries"))?;
    if entries.len() > pool.len() {
        return Err(anyhow!(
            "digest has more entries than the recorded selection pool"
        ));
    }
    for (index, entry) in entries.iter().enumerate() {
        let name = entry["full_name"]
            .as_str()
            .ok_or_else(|| anyhow!("ranked entry has no repository name"))?;
        let expected = pool[index]["full_name"]
            .as_str()
            .ok_or_else(|| anyhow!("selection pool row has no repository name"))?;
        if name != expected {
            return Err(anyhow!(
                "digest ranking differs from the recorded selection pool at position {}",
                index + 1
            ));
        }
    }
    editions::validate_catalogue(digest, &selection)?;
    Ok(())
}

fn exact_evidence_quote(description: Option<&str>, readme_excerpt: Option<&str>) -> Option<String> {
    if let Some(description) = description
        .map(str::trim)
        .filter(|text| text.chars().count() >= 12)
    {
        return Some(description.chars().take(180).collect());
    }
    let readme = readme_excerpt?.trim();
    let paragraph = readme.split("\n\n").map(str::trim).find(|text| {
        text.chars().count() >= 12
            && !text.starts_with('#')
            && !text.starts_with("![")
            && !text.starts_with('<')
            && !text.starts_with("---")
    })?;
    Some(paragraph.chars().take(180).collect())
}

fn make_digest(
    state: &mut State,
    current: &[Snapshot],
    manifest: &Value,
    now: DateTime<Utc>,
    do_llm: bool,
) -> Result<(Value, String)> {
    make_digest_for(
        state,
        current,
        manifest,
        now,
        do_llm,
        editions::Format::Weekly,
    )
}

fn make_digest_for(
    state: &mut State,
    current: &[Snapshot],
    manifest: &Value,
    now: DateTime<Utc>,
    do_llm: bool,
    format: editions::Format,
) -> Result<(Value, String)> {
    let (mut entries, mut selection_excluded) = editions::select(current, state, format);
    let mut selected = entries
        .iter()
        .map(|entry| entry.snapshot.clone())
        .collect::<Vec<_>>();
    let source_clock = clock::SystemClock;
    let source_http = sources::GitHubHttp::from_env(&source_clock)?;
    let evidence_sources = sources::enrich_readmes(
        &mut selected,
        &runs_dir().join(stamp(now)),
        &source_http,
        &source_clock,
    )?;
    for (entry, snapshot) in entries.iter_mut().zip(selected) {
        entry.snapshot = snapshot;
    }
    let mut manifest = manifest.clone();
    manifest["sources"]
        .as_array_mut()
        .context("source manifest has no source list")?
        .extend(evidence_sources);
    manifest["weekly_observed_repositories"] = json!(current.len());
    manifest["weekly_selected_repositories"] = json!(entries.len());
    write_json(
        &runs_dir().join(stamp(now)).join("manifest.json"),
        &manifest,
    )?;
    let pool_rows: Vec<Value> = entries
        .iter()
        .map(|entry| {
            let (_, metric, kind) = editions::priority(&entry.snapshot, format);
            json!({"full_name":entry.snapshot.full_name,"source_kind":kind,"ordering_metric":metric})
        })
        .collect();
    write_json(
        &runs_dir().join(stamp(now)).join("selection.json"),
        &json!({"pool":pool_rows,"excluded":selection_excluded,"covered_repositories_tracked":state.covered.len()}),
    )?;
    let state_file = state_path();
    let provider = providers::Groq::from_env(&state_file, &clock::SystemClock)?;
    let providers::Enrichment {
        descriptions,
        jokes,
        evidence_quotes,
        complete,
        budget: llm_budget,
    } = if format == editions::Format::Daily && do_llm {
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
        let ready_pool = pool_rows
            .into_iter()
            .filter(|row| {
                row["full_name"]
                    .as_str()
                    .is_some_and(|name| descriptions.contains_key(name))
            })
            .collect::<Vec<_>>();
        write_json(
            &runs_dir().join(stamp(now)).join("selection.json"),
            &json!({"pool":ready_pool,"excluded":selection_excluded,"covered_repositories_tracked":state.covered.len()}),
        )?;
        anyhow::ensure!(
            attempted == 0 || !entries.is_empty(),
            "no source-backed Russian explanations ready; findings remain queued; ready={}/{attempted}, requests={}: {}",
            entries.len(),
            llm_budget["actual_requests"],
            llm_budget["reason"]
                .as_str()
                .unwrap_or("no enrichment diagnostic")
        );
    }
    let entries_json: Vec<Value> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let id = &entry.snapshot.full_name;
            let description = descriptions.get(id).cloned().unwrap_or_else(|| {
                safe_public_text(
                    entry
                        .snapshot
                        .description
                        .as_deref()
                        .filter(|text| !text.trim().is_empty())
                        .unwrap_or("Описание не указано в GitHub."),
                )
            });
            let evidence_quote = evidence_quotes
                .get(id)
                .cloned()
                .or_else(|| {
                    exact_evidence_quote(
                        entry.snapshot.description.as_deref(),
                        entry.snapshot.readme_excerpt.as_deref(),
                    )
                })
                .unwrap_or_default();
            let evidence_kind = if evidence_quotes.contains_key(id) {
                "provider_selected_exact_readme_quote"
            } else if evidence_quote.is_empty() {
                "unavailable"
            } else if entry.snapshot.description.as_deref().is_some_and(|text| text.trim().chars().count() >= 12) {
                "github_description_excerpt"
            } else {
                "readme_excerpt"
            };
            json!({
                "rank": index + 1,
                "total": entries.len(),
                "full_name": id,
                "latest": entry.snapshot,
                "baseline": entry.baseline,
                "delta": entry.delta,
                "period": entry.period,
                "source_kind": editions::priority(&entry.snapshot, format).2,
                "metric_captured_at": entry.snapshot.sources.iter().find(|source| source["type"].as_str() == Some(editions::priority(&entry.snapshot, format).2)).and_then(|source| source["captured_at"].as_str()).unwrap_or(&entry.snapshot.captured_at),
                "trending_source": entry.trending_source,
                "description": description,
                "joke": jokes.get(id).cloned().unwrap_or_default(),
                "evidence_quote": evidence_quote,
                "evidence_kind": evidence_kind,
                "evidence_scope":"Exact source quotation confirms provenance only; factual accuracy and claim entailment are not automatically verified",
                "llm_used": descriptions.contains_key(id)
            })
        })
        .collect();
    let mut official = BTreeMap::<String, Value>::new();
    let mut newrepos = Vec::<Value>::new();
    let cutoff = now - Duration::days(7);
    for snapshot in current {
        for source in &snapshot.sources {
            if source["type"] == "trending_weekly" && source["trending_stars_period"].is_number() {
                official.insert(
                    snapshot.full_name.clone(),
                    json!({"full_name":snapshot.full_name,"stars":snapshot.stars,"weekly_trending_stars":source["trending_stars_period"],"url":snapshot.html_url}),
                );
            }
        }
        if snapshot
            .created_at
            .as_deref()
            .and_then(parse_time)
            .is_some_and(|date| date >= cutoff)
        {
            newrepos.push(json!({"full_name":snapshot.full_name,"stars":snapshot.stars,"url":snapshot.html_url,"created_at":snapshot.created_at}));
        }
    }
    let mut official: Vec<Value> = official.into_values().collect();
    official.sort_by_key(|entry| -entry["weekly_trending_stars"].as_i64().unwrap_or(0));
    official.truncate(10);
    newrepos.sort_by_key(|entry| -entry["stars"].as_i64().unwrap_or(0));
    newrepos.truncate(10);
    let coverage_report = coverage::report(&state.snapshots, now, &runs_dir())?;
    let trend_summary = "Недельная подборка из сохранённых наблюдений GitHub Trending и поиска. Плановый интервал сбора — три часа; фактические интервалы и пробелы указаны отдельно. Сначала проекты из недельного Trending, затем дневного, затем найденные новые репозитории. Для Trending показан максимальный наблюдавшийся +N своего периода, а не сумма срезов и не измеренный прирост за календарную неделю. Это ограниченная выборка, не рейтинг качества всего GitHub и не список новых релизов.";
    let digest = json!({
        "format_version":3,
        "generated_at":now.to_rfc3339_opts(chrono::SecondsFormat::Secs,true),
        "delivery_slot":format.slot(now),
        "edition":format.name(),
        "period_start":(now - Duration::days(7)).to_rfc3339_opts(chrono::SecondsFormat::Secs,true),
        "period_end":now.to_rfc3339_opts(chrono::SecondsFormat::Secs,true),
        "ranking":"Union of all retained observations within seven days; latest metadata and maximum observed rolling Trending metric with its original timestamp; first/last appearance retained. Only weekly selection excludes covered repositories and caps at 30. Weekly Trending peak +N first, daily peak +N next, then Search by latest total stars. Never sum overlapping rolling metrics; not a global census or a release feed.",
        "dedup_excluded":selection_excluded,
        "entries":entries_json,
        "llm_used":complete || (format == editions::Format::Daily && do_llm && !entries.is_empty()),
        "llm_requested":do_llm,
        "trend_summary":trend_summary,
        "coverage":coverage_report,
        "joke":"",
        "llm_budget":llm_budget,
        "official_github_trending_weekly":official,
        "new_repositories_by_total_stars":newrepos,
        "trends":{
            "forks":rank(state,now,"forks").iter().take(10).map(|entry|json!({"full_name":entry.snapshot.full_name,"delta":entry.delta})).collect::<Vec<_>>(),
            "percent":rank(state,now,"percent").iter().take(10).map(|entry|json!({"full_name":entry.snapshot.full_name,"delta":entry.delta})).collect::<Vec<_>>()
        },
        "manifest":manifest
    });
    let output = render(&digest);
    Ok((digest, output))
}

fn safe_public_text(text: &str) -> String {
    text.split_inclusive(char::is_whitespace)
        .map(|piece| {
            let part = piece.trim_end_matches(char::is_whitespace);
            let whitespace = &piece[part.len()..];
            let trimmed =
                part.trim_matches(|c: char| !c.is_ascii_alphanumeric() && !"_-:".contains(c));
            let lower = trimmed.to_ascii_lowercase();
            let raw_key = lower.starts_with("gsk_")
                || lower.starts_with("sk-")
                || lower.starts_with("aiza")
                || lower.starts_with("ghp_")
                || lower.starts_with("gho_")
                || lower.starts_with("github_pat_")
                || (trimmed.contains(':')
                    && lower.len() > 35
                    && lower.chars().take_while(|c| c.is_ascii_digit()).count() >= 8);
            let assigned_secret = part.split_once('=').is_some_and(|(key, _)| {
                let key = key.to_ascii_lowercase();
                [
                    "api_key",
                    "apikey",
                    "token",
                    "secret",
                    "password",
                    "credential",
                ]
                .iter()
                .any(|marker| key.contains(marker))
            });
            let safe_part = if raw_key || assigned_secret {
                "[скрыто]"
            } else {
                part
            };
            format!("{safe_part}{whitespace}")
        })
        .collect()
}

/// Telegram caps one message at 4096 chars; stay safely under it.
const ISSUE_CHUNK_LIMIT: usize = 3_800;

fn ru_count(n: usize, one: &str, few: &str, many: &str) -> String {
    let n10 = n % 10;
    let n100 = n % 100;
    let word = if n10 == 1 && n100 != 11 {
        one
    } else if (2..=4).contains(&n10) && !(12..=14).contains(&n100) {
        few
    } else {
        many
    };
    format!("{n} {word}")
}

fn language_hashtag(language: &str) -> Option<String> {
    let language = match language.trim() {
        "C++" => "CPP",
        "C#" => "CSharp",
        "F#" => "FSharp",
        "Objective-C++" => "ObjectiveCPP",
        value => value,
    };
    let tag: String = language
        .chars()
        .filter(|ch| ch.is_alphanumeric() || *ch == '_')
        .collect();
    tag.chars()
        .any(char::is_alphabetic)
        .then(|| format!("#{tag}"))
}

fn grouped_count(value: i64) -> String {
    let digits = value.to_string();
    let mut text = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push(' ');
        }
        text.push(digit);
    }
    text
}

/// Render the complete source-backed card; never remove its description to
/// make other cards fit. Chunking happens only between these rendered cards.
fn issue_item(entry: &Value) -> String {
    let id = entry["full_name"].as_str().unwrap_or("unknown/repository");
    let snapshot = &entry["latest"];
    let title = project_github_url(id)
        .map(|url| {
            format!(
                "[{}]({})",
                markdown_v2_escape(id),
                markdown_v2_url_escape(&url)
            )
        })
        .unwrap_or_else(|| markdown_v2_escape(id));
    let mut stats = Vec::new();
    if let Some(tag) = snapshot["language"].as_str().and_then(language_hashtag) {
        stats.push(tag);
    }
    if let Some(stars) = snapshot["stars"].as_i64().filter(|value| *value >= 0) {
        stats.push(format!("⭐ {}", grouped_count(stars)));
    }
    if let Some(delta) = entry["delta"].as_i64().filter(|value| *value > 0) {
        let period = match entry["source_kind"].as_str() {
            Some("trending_daily") => Some("день"),
            Some("trending_weekly") => Some("неделю"),
            _ => None,
        };
        if let Some(period) = period {
            stats.push(format!(
                "Trending: макс. +{} за {period}",
                grouped_count(delta)
            ));
        }
    }
    let description = entry["description"].as_str().unwrap_or("").trim();
    let joke_line = match entry["joke"].as_str().map(str::trim) {
        Some(joke) if !joke.is_empty() => {
            format!("\n{}", render_markdown_v2_inline(joke))
        }
        _ => String::new(),
    };
    let mut card = format!(
        "{title}\n{}{joke_line}",
        render_markdown_v2_inline(description)
    );
    if !stats.is_empty() {
        card.push('\n');
        card.push_str(&markdown_v2_escape(&stats.join(" · ")));
    }
    card
}

/// Preserve complete cards and split only between projects. A card that cannot
/// fit by itself is an explicit error, never a silently shortened description.
fn render_issue_messages(digest: &Value) -> Result<Vec<String>> {
    let entries: Vec<&Value> = digest["entries"]
        .as_array()
        .map(|entries| entries.iter().collect())
        .unwrap_or_default();
    if entries.is_empty() {
        return Ok(Vec::new());
    }
    let generated = digest["generated_at"].as_str().unwrap_or("");
    let when = DateTime::parse_from_rfc3339(generated)
        .ok()
        .map(|time| {
            markdown_v2_escape(
                &(time.with_timezone(&Utc) + Duration::hours(3))
                    .format("%d.%m.%Y")
                    .to_string(),
            )
        })
        .unwrap_or_else(|| markdown_v2_escape(generated));
    let counted = ru_count(entries.len(), "проект", "проекта", "проектов");
    let edition = if digest["edition"] == "daily" {
        "находки дня"
    } else {
        "подборка недели"
    };
    let date_and_count = if when.is_empty() {
        markdown_v2_escape(&counted)
    } else {
        format!("{when} · {}", markdown_v2_escape(&counted))
    };
    let header = format!("*GitHub — {edition}*\n{date_and_count}");
    let header = if env::var("DIGEST_TEST_ONLY").as_deref() == Ok("true") {
        format!("*ТЕСТОВЫЙ ВЫПУСК — наблюдения с 31\\.10\\.2026*\n{header}")
    } else {
        header
    };
    let continuation = "*GitHub — продолжение*";
    let mut chunks = Vec::new();
    let mut current = header.trim_end().to_string();
    for entry in entries {
        let item = issue_item(entry);
        anyhow::ensure!(
            continuation.encode_utf16().count() + 2 + item.encode_utf16().count()
                <= ISSUE_CHUNK_LIMIT,
            "a complete project card exceeds the Telegram-safe limit; review its source text"
        );
        if current.encode_utf16().count() + 2 + item.encode_utf16().count() > ISSUE_CHUNK_LIMIT {
            chunks.push(current);
            current = continuation.to_string();
        }
        current.push_str("\n\n");
        current.push_str(&item);
    }
    chunks.push(current);
    editions::append_catalogue(&mut chunks, digest)?;
    Ok(chunks)
}

/// Joined representation for the preview artifact; the sender uses separate chunks.
fn render_issue_message(digest: &Value) -> Result<String> {
    Ok(render_issue_messages(digest)?.join("\n\n"))
}

fn markdown_v2_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        if "_*[]()~`>#+-=|{}.!\\".contains(ch) {
            escaped.push('\\');
        }
        escaped.push(ch);
    }
    escaped
}

fn markdown_v2_url_escape(url: &str) -> String {
    url.replace('\\', "\\\\").replace(')', "\\)")
}

fn render_markdown_v2_inline(text: &str) -> String {
    let mut output = String::new();
    let mut remaining = text;
    while let Some(start) = remaining.find('`') {
        let after_start = &remaining[start + 1..];
        let Some(end) = after_start.find('`') else {
            output.push_str(&render_plain_text_with_scoped_packages(remaining));
            return output;
        };
        output.push_str(&render_plain_text_with_scoped_packages(&remaining[..start]));
        if end == 0 {
            // Empty code entities are not part of the emitted subset. Keep the
            // literal backticks from a source instead of generating one.
            output.push_str(&markdown_v2_escape("``"));
        } else {
            output.push('`');
            output.push_str(&after_start[..end].replace('\\', "\\\\").replace('`', "\\`"));
            output.push('`');
        }
        remaining = &after_start[end + 1..];
    }
    output.push_str(&render_plain_text_with_scoped_packages(remaining));
    output
}

fn render_plain_text_with_scoped_packages(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for token in text.split_inclusive(char::is_whitespace) {
        let token_end = token.trim_end_matches(char::is_whitespace).len();
        let (word, whitespace) = token.split_at(token_end);
        let package = word.trim_end_matches(|ch: char| ",.;:!?)]}".contains(ch));
        let punctuation = &word[package.len()..];
        if package.starts_with('@')
            && package[1..]
                .split_once('/')
                .is_some_and(|(scope, name)| !scope.is_empty() && !name.is_empty())
        {
            output.push('`');
            output.push_str(&package.replace('\\', "\\\\").replace('`', "\\`"));
            output.push('`');
            output.push_str(&markdown_v2_escape(punctuation));
        } else {
            output.push_str(&markdown_v2_escape(word));
        }
        output.push_str(whitespace);
    }
    output
}

fn project_github_url(repository: &str) -> Option<String> {
    let (owner, name) = repository.split_once('/')?;
    let valid_owner = !owner.is_empty()
        && owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    let valid_name = !name.is_empty()
        && !name.contains('/')
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte));
    (valid_owner && valid_name).then(|| format!("https://github.com/{owner}/{name}"))
}

fn telegram_message_link(configured_chat: &str, message_id: i64) -> Result<String> {
    if let Some(username) = configured_chat.strip_prefix('@') {
        return Ok(format!("https://t.me/{username}/{message_id}"));
    }
    let channel_id = configured_chat.strip_prefix("-100").ok_or_else(|| anyhow!("TELEGRAM_CHAT_ID must be a channel id beginning with -100 or a public @username to create post links"))?;
    Ok(format!("https://t.me/c/{channel_id}/{message_id}"))
}

fn render(d: &Value) -> String {
    let total = d["entries"]
        .as_array()
        .map(|entries| entries.len())
        .unwrap_or(0);
    let mut lines = vec![
        format!(
            "Генерация: {}",
            d["generated_at"].as_str().unwrap_or("неизвестно")
        ),
        format!("Новых проектов в выпуске: {total}"),
    ];
    if let Some(reason) = d["llm_budget"]["reason"].as_str() {
        lines.push(format!("Статус LLM: {reason}"));
    }
    lines.push(
        "Формат выпуска: полные карточки, разбитые на сообщения (см. telegram-messages.json)."
            .into(),
    );
    lines.join("\n")
}

/// A legacy artifact cannot acquire a complete observation history by re-rendering.
fn coverage_notice(digest: &Value) -> String {
    let mut notice = match digest.get("coverage").filter(|report| report["format_version"] == 1) {
        Some(report) => coverage::render_notice(report),
        None => "Полнота наблюдений для этого сохранённого выпуска неизвестна; пропущенные данные не восстановлены.".to_string(),
    };
    notice.push_str(
        " Возможности проектов приведены по материалам авторов; работа проектов не проверялась.",
    );
    notice
}

fn record_coverage_summary(report: &Value) -> Result<()> {
    if let Some(path) = env::var_os("GITHUB_STEP_SUMMARY") {
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?;
        writeln!(
            file,
            "## Observation coverage\n\n{}\n",
            coverage::render_notice(report)
        )?;
    }
    Ok(())
}

/// Human-readable source audit; the outgoing messages live in issue.md.
fn render_summary(digest: &Value) -> String {
    let mut text = format!(
        "# GitHub — {}\n\nПериод обзора: {} — {}\n",
        if digest["edition"] == "daily" {
            "находки дня"
        } else {
            "итоги недели"
        },
        digest["period_start"].as_str().unwrap_or("?"),
        digest["period_end"].as_str().unwrap_or("?")
    );
    text.push_str(&format!("\n{}\n", coverage_notice(digest)));
    if let Some(entries) = digest["entries"].as_array() {
        for entry in entries {
            let name = entry["full_name"].as_str().unwrap_or("?");
            if let Some(url) = project_github_url(name) {
                text.push_str(&format!(
                    "\n## [{name}]({url})\n\n{}\n\nМетрика: {} — {}; всего звёзд: {}; срез метаданных: {}; срез метрики: {}.\n",
                    entry["description"].as_str().unwrap_or(""),
                    entry["period"].as_str().unwrap_or(""),
                    entry["delta"],
                    entry["latest"]["stars"],
                    entry["latest"]["captured_at"].as_str().unwrap_or("?"),
                    entry["metric_captured_at"].as_str().or_else(|| entry["latest"]["captured_at"].as_str()).unwrap_or("?")
                ));
                if let Some(observation) = entry["latest"].get("observation") {
                    text.push_str(&format!(
                        "\nВпервые замечен: {}; последнее наблюдение: {}; срезов: {}.\n",
                        observation["first_seen"].as_str().unwrap_or("?"),
                        observation["last_seen"].as_str().unwrap_or("?"),
                        observation["observations"]
                    ));
                }
            }
        }
    }
    text.push_str(&editions::catalogue_markdown(digest));
    text
}

fn telegram_send_to(chat: &str, text: &str) -> Result<i64> {
    telegram_send_reply_to(chat, None, text)
}

/// Same as `telegram_send_to`, but the message is threaded as a Telegram
/// reply to `reply_to` when given.
fn telegram_send_reply_to(chat: &str, reply_to: Option<i64>, text: &str) -> Result<i64> {
    test_delivery::guard(chat)?;
    if env::var("DIGEST_TEST_ONLY").as_deref() == Ok("true") {
        std::thread::sleep(StdDuration::from_millis(3100));
    }
    telegram_format::validate_message(text, ISSUE_CHUNK_LIMIT)
        .context("refusing to send invalid emitted Telegram MarkdownV2")?;
    let token = env::var("TELEGRAM_BOT_TOKEN").context("TELEGRAM_BOT_TOKEN missing")?;
    let http = client()?;
    let mut body = json!({"chat_id":chat,"text":text,"parse_mode":"MarkdownV2","disable_web_page_preview":true});
    if let Some(reply_to) = reply_to {
        body["reply_parameters"] =
            json!({"message_id":reply_to,"allow_sending_without_reply":true});
    }
    let response = http
        .post(format!("https://api.telegram.org/bot{token}/sendMessage"))
        .json(&body)
        .send()
        .map_err(|_| anyhow!("Telegram sendMessage transport failed"))?;
    let status = response.status();
    let resp: Value = response.json()?;
    if !status.is_success() || resp["ok"] != true {
        let description = resp["description"]
            .as_str()
            .unwrap_or("Telegram rejected the message without a description");
        return Err(anyhow!(
            "Telegram sendMessage rejected the message (HTTP {}): {}",
            status.as_u16(),
            description.chars().take(200).collect::<String>()
        ));
    }
    test_delivery::receipt(&resp["result"], text)?;
    resp["result"]["message_id"]
        .as_i64()
        .ok_or_else(|| anyhow!("Telegram sendMessage response omitted message_id"))
}

fn telegram_pin_in(chat: &str, message_id: i64) -> Result<()> {
    test_delivery::guard(chat)?;
    let token = env::var("TELEGRAM_BOT_TOKEN").context("TELEGRAM_BOT_TOKEN missing")?;
    let http = client()?;
    let pinned: Value = http
        .post(format!("https://api.telegram.org/bot{token}/getChat"))
        .json(&json!({"chat_id":chat}))
        .send()
        .map_err(|_| anyhow!("Telegram getChat transport failed"))?
        .json()?;
    if pinned["ok"] == true && pinned["result"]["pinned_message"]["message_id"] == message_id {
        return Ok(());
    }
    let response = http
        .post(format!(
            "https://api.telegram.org/bot{token}/pinChatMessage"
        ))
        .json(&json!({"chat_id":chat,"message_id":message_id,"disable_notification":true}))
        .send()
        .map_err(|_| anyhow!("Telegram pinChatMessage transport failed"))?;
    let status = response.status();
    let resp: Value = response.json()?;
    if resp["ok"] != true || resp["result"] != true {
        return Err(anyhow!(
            "Telegram pinChatMessage failed (HTTP {})",
            status.as_u16()
        ));
    }
    Ok(())
}

fn telegram_pin(message_id: i64) -> Result<()> {
    let chat = env::var("TELEGRAM_CHAT_ID").context("TELEGRAM_CHAT_ID missing")?;
    telegram_pin_in(&chat, message_id)
}

fn credential_markers(text: &str) -> Vec<&'static str> {
    let mut matches = Vec::new();
    let lower = text.to_ascii_lowercase();
    for word in text.split_whitespace() {
        let token = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && !"_-.:".contains(c));
        let lower_token = token.to_ascii_lowercase();
        for (prefix, class, min_len) in [
            ("ghp_", "github-token", 30),
            ("gho_", "github-token", 30),
            ("ghu_", "github-token", 30),
            ("ghs_", "github-token", 30),
            ("ghr_", "github-token", 30),
            ("github_pat_", "github-token", 30),
            ("gsk_", "groq-key", 30),
            ("sk-", "api-key", 32),
            ("aiza", "google-api-key", 30),
            ("xoxb-", "slack-token", 30),
            ("xoxp-", "slack-token", 30),
            ("ya29.", "google-oauth-token", 30),
            ("akia", "aws-access-key", 20),
            ("asia", "aws-access-key", 20),
        ] {
            if lower_token.starts_with(prefix) && token.len() >= min_len {
                matches.push(class);
            }
        }
    }
    if lower.contains("-----begin private key-----")
        || lower.contains("-----begin rsa private key-----")
        || lower.contains("-----begin openssh private key-----")
    {
        matches.push("private-key");
    }
    for word in text.split_whitespace() {
        let token = word.trim_matches(|c: char| !c.is_ascii_alphanumeric() && !"_-:".contains(c));
        if let Some((id, secret)) = token.split_once(':')
            && (8..=12).contains(&id.len())
            && id.chars().all(|c| c.is_ascii_digit())
            && secret.len() >= 25
            && secret
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
        {
            matches.push("telegram-bot-token-pattern");
            break;
        }
    }
    matches
}

/// Validate every actual payload before the
/// caller accepts a preview or starts sending the first project chunk.
fn digest_publication_text(digest: &Value) -> Result<Vec<String>> {
    if digest["format_version"] != 3 {
        return Err(anyhow!(
            "publication requires a deterministic format version 3 artifact"
        ));
    }
    let entries = digest["entries"]
        .as_array()
        .ok_or_else(|| anyhow!("digest has no project entries"))?;
    if entries.is_empty() {
        return Err(anyhow!("publication requires at least one project"));
    }
    for entry in entries {
        if entry["description"]
            .as_str()
            .is_none_or(|description| description.trim().is_empty())
        {
            return Err(anyhow!("publication found a project without a description"));
        }
    }
    let chunks = render_issue_messages(digest)?;
    for (index, chunk) in chunks.iter().enumerate() {
        telegram_format::validate_message(chunk, ISSUE_CHUNK_LIMIT)
            .with_context(|| format!("invalid rendered publication chunk {}", index + 1))?;
    }
    Ok(chunks)
}

/// Sends the issue chunks sequentially, resuming per chunk after an
/// interruption instead of duplicating messages. Returns the message ids.
fn send_issue_chunks(
    chat: &str,
    chunks: &[String],
    progress_key: &str,
    state: &mut State,
) -> Result<Vec<i64>> {
    anyhow::ensure!(
        !state.delivery_progress.contains_key(progress_key),
        "legacy unscoped delivery progress requires operator reconciliation"
    );
    delivery::send_chunks(
        chunks,
        &format!("{chat}:{progress_key}"),
        state,
        |text| telegram_send_to(chat, text),
        |state| write_json(&state_path(), state),
    )
}

/// Delivers complete project chunks to the technical chat.
fn telegram_deliver_debug_digest(
    digest: &Value,
    digest_id: &str,
    run_dir: &Path,
    state: &mut State,
) -> Result<DebugDelivery> {
    let chunks = digest_publication_text(digest)?;
    let chat = env::var("TELEGRAM_TECH_CHAT_ID").context("TELEGRAM_TECH_CHAT_ID missing")?;
    let delivery_path = run_dir.join("debug-delivery.json");
    if delivery_path.exists() {
        let delivery: DebugDelivery = serde_json::from_slice(&fs::read(&delivery_path)?)?;
        let progress_key = format!("{chat}:{digest_id}");
        delivery::validate_chunks(&chunks, &progress_key, state)?;
        let progress = state.delivery_progress.get(&progress_key);
        if delivery.day != digest_id
            || delivery.chat_id != chat
            || delivery.posts.len() != chunks.len()
            || !chunks.iter().enumerate().all(|(index, text)| {
                let key = format!("chunk-{index:02}");
                progress.is_some_and(|saved| {
                    saved.message_texts.get(&key) == Some(text)
                        && saved
                            .project_message_ids
                            .get(&key)
                            .is_some_and(|id| delivery.posts.get(&key) == Some(id))
                })
            })
        {
            return Err(anyhow!(
                "existing debug delivery does not match this digest"
            ));
        }
        return Ok(delivery);
    }
    let ids = send_issue_chunks(&chat, &chunks, digest_id, state)?;
    let first_id = *ids.first().ok_or_else(|| anyhow!("issue had no chunks"))?;
    fs::write(run_dir.join("issue.md"), chunks.join("\n\n"))?;
    let mut posts = BTreeMap::new();
    for (index, id) in ids.iter().enumerate() {
        posts.insert(format!("chunk-{index:02}"), *id);
    }
    let delivery = DebugDelivery {
        day: digest_id.to_string(),
        chat_id: chat,
        posts,
        overview_message_id: first_id,
    };
    write_json(&delivery_path, &delivery)?;
    Ok(delivery)
}

/// Best-effort notification to the technical chat about conditions the
/// operator should know about; never fails the run by itself.
fn tech_notice(text: &str) {
    let Ok(chat) = env::var("TELEGRAM_TECH_CHAT_ID") else {
        return;
    };
    let Ok(token) = env::var("TELEGRAM_BOT_TOKEN") else {
        return;
    };
    if token.trim().is_empty() || chat.trim().is_empty() {
        return;
    }
    let _ = telegram_send_to(&chat, &markdown_v2_escape(text));
}

/// Drops entries that are already covered (published by an earlier digest,
/// manually released, or seeded from prior channel content) so no delivery
/// path can ever repost them, then reindexes rank/total.
fn trim_covered_entries(digest: &mut Value, state: &State) -> usize {
    let format = if digest["edition"] == "daily" {
        editions::Format::Daily
    } else {
        editions::Format::Weekly
    };
    let Some(entries) = digest["entries"].as_array_mut() else {
        return 0;
    };
    let before = entries.len();
    entries.retain(|entry| {
        entry["full_name"]
            .as_str()
            .is_none_or(|name| !editions::was_published(state, format, name))
    });
    let total = entries.len();
    for (index, entry) in entries.iter_mut().enumerate() {
        entry["rank"] = json!(index + 1);
        entry["total"] = json!(total);
    }
    before - total
}

/// Sends the issue to the main chat and pins its first message. Progress is
/// keyed by the digest id, so a workflow retry resumes instead of
/// duplicating messages. On completion every reviewed repository is added
/// to `covered`, which keeps it out of future issues, and the slot is
/// marked as having delivered its one issue.
fn telegram_deliver_digest(
    digest: &Value,
    digest_id: &str,
    day: &str,
    slot_key: &str,
    state: &mut State,
) -> Result<()> {
    let chunks = digest_publication_text(digest)?;
    if chunks.is_empty() {
        return Err(anyhow!("refusing delivery without any project"));
    }
    let chat = env::var("TELEGRAM_CHAT_ID").context("TELEGRAM_CHAT_ID missing")?;
    telegram_message_link(&chat, 1)?;
    let ids = send_issue_chunks(&chat, &chunks, digest_id, state)?;
    let first_id = *ids
        .first()
        .ok_or_else(|| anyhow!("issue delivery produced no messages"))?;
    fs::create_dir_all(runs_dir().join(digest_id))?;
    fs::write(
        runs_dir().join(digest_id).join("issue.md"),
        chunks.join("\n\n"),
    )?;
    editions::acknowledge(state, digest, day)?;
    state.sent_digests.insert(digest_id.to_string(), first_id);
    state.sent_slots.insert(slot_key.to_string(), first_id);
    state
        .delivery_progress
        .remove(&format!("{chat}:{digest_id}"));
    state.pending_digests.remove(digest_id);
    write_json(&state_path(), state)?;
    // Pinning is cosmetic and must follow complete, durable delivery.
    if let Err(error) = telegram_pin(first_id) {
        println!("Pin skipped: {error:#}");
    }
    Ok(())
}

/// Retries and delayed schedule events share one weekly publication identity.
fn slot_key_of(moment: DateTime<Utc>) -> String {
    schedule::slot(moment)
}

/// Resume the frozen weekly payload even when its original slot is overdue.
fn deliver_pending_digests(state: &mut State, now: DateTime<Utc>) -> Result<()> {
    editions::resume(state, now, editions::Format::Weekly)
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("preview");
    if matches!(command, "--version" | "-V") {
        let version = option_env!("WEEKLY_DIGEST_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"));
        let commit = option_env!("WEEKLY_DIGEST_COMMIT").unwrap_or("unknown");
        println!("weekly-gh-digest {version} ({commit})");
        return Ok(());
    }
    if command == "preview-retained" {
        return editions::preview_retained(&args);
    }
    checkpoint::require_hosted_configuration()?;
    let format = if args.iter().any(|arg| arg == "--daily") {
        editions::Format::Daily
    } else {
        editions::Format::Weekly
    };
    let dry = args.iter().any(|x| x == "--dry-run");
    if command == "debug-delivery" || (matches!(command, "digest" | "debug") && !dry) {
        anyhow::ensure!(
            schedule::delivery_enabled(env::var("DIGEST_DELIVERY_ENABLED").ok().as_deref()),
            "publication is disabled pending preview review"
        );
        anyhow::ensure!(
            format != editions::Format::Daily
                || schedule::delivery_enabled(
                    env::var("DIGEST_DAILY_DELIVERY_ENABLED").ok().as_deref()
                ),
            "daily publication is disabled pending preview review"
        );
    }
    if command == "test-summary" {
        return test_delivery_review::run(&args);
    }
    if command == "preview-stored" {
        let run_dir = args
            .get(1)
            .map(PathBuf::from)
            .context("usage: weekly-gh-digest preview-stored <run-directory>")?;
        record_run_directory(&run_dir)?;
        let mut digest: Value =
            serde_json::from_slice(&fs::read(run_dir.join("ranking-input.json"))?)?;
        validate_digest_selection(&run_dir, &digest)?;
        digest_publication_text(&digest)?;
        let refreshed_at = Utc::now().to_rfc3339();
        digest["format_refreshed_at"] = json!(refreshed_at);
        save_preview_files(&run_dir, &digest, false)?;
        write_json(
            &run_dir.join("format-refresh.json"),
            &json!({
                "source_generated_at":digest["generated_at"],
                "refreshed_at":refreshed_at,
                "provider_requests":0,
                "message_count":digest_publication_text(&digest)?.len(),
                "editorial_policy_version":editorial::POLICY_VERSION
            }),
        )?;
        println!(
            "Stored cards reformatted offline; no collection, provider requests or Telegram delivery."
        );
        return Ok(());
    }
    if command == "debug-delivery" {
        let run_dir = args
            .get(1)
            .map(PathBuf::from)
            .ok_or_else(|| anyhow!("usage: weekly-gh-digest debug-delivery <run-directory>"))?;
        record_run_directory(&run_dir)?;
        let mut digest: Value =
            serde_json::from_slice(&fs::read(run_dir.join("ranking-input.json"))?)
                .context("debug artifact has invalid ranking-input.json")?;
        validate_digest_selection(&run_dir, &digest)?;
        anyhow::ensure!(
            editions::Format::from_digest(&digest)? != editions::Format::Daily
                || schedule::delivery_enabled(
                    env::var("DIGEST_DAILY_DELIVERY_ENABLED").ok().as_deref()
                ),
            "daily publication is disabled pending preview review"
        );
        if digest["entries"]
            .as_array()
            .is_none_or(|entries| entries.is_empty())
        {
            return Err(anyhow!("refusing debug delivery without any project"));
        }
        let mut state = read_state_strict()?;
        let generated_at = digest["generated_at"]
            .as_str()
            .ok_or_else(|| anyhow!("debug artifact has no generation timestamp"))?;
        let generated = DateTime::parse_from_rfc3339(generated_at)?.with_timezone(&Utc);
        let digest_id = generated.format("%Y%m%dT%H%M%SZ").to_string();
        let audit = advisory_audit(&mut digest, &mut state, &run_dir)?;
        let delivery = telegram_deliver_debug_digest(&digest, &digest_id, &run_dir, &mut state)?;
        write_json(&state_path(), &state)?;
        if let Err(error) = telegram_pin_in(&delivery.chat_id, delivery.overview_message_id) {
            println!("Pin skipped: {error:#}");
        }
        println!(
            "Technical preview delivered as {} message(s); review status: {}",
            delivery.posts.len(),
            audit["status"]
        );
        return Ok(());
    }
    if !["collect", "preview", "digest", "debug"].contains(&command) {
        return Err(anyhow!(
            "usage: weekly-gh-digest [collect|preview|digest|debug] [--dry-run] [--llm]"
        ));
    }
    let llm = args.iter().any(|x| x == "--llm");
    let now = Utc::now();
    let source_clock = clock::SystemClock;
    let source_http = sources::GitHubHttp::from_env(&source_clock)?;
    let gathered = sources::collect(now, &runs_dir(), &source_http, &source_clock)?;
    record_run_directory(&gathered.dir)?;
    let mut state = update_state(&gathered.snapshots)?;
    if command == "collect" {
        let coverage_report = coverage::report(&state.snapshots, now, &runs_dir())?;
        write_json(&gathered.dir.join("coverage.json"), &coverage_report)?;
        record_coverage_summary(&coverage_report)?;
        println!(
            "Observation saved without provider calls or Telegram delivery. {}",
            coverage::render_notice(&coverage_report)
        );
        return Ok(());
    }
    let day = now.format("%Y-%m-%d").to_string();
    let digest_id = format!("{}-{}", format.name(), stamp(now));
    let slot_key = format.slot(now);
    if command == "digest" && !dry {
        if format == editions::Format::Weekly {
            deliver_pending_digests(&mut state, now)?;
        } else {
            editions::resume(&mut state, now, format)?;
        }
    }
    // Keep collecting on retries, but do not spend provider requests for a
    // publication slot that already has a durable acknowledgement.
    if command == "digest" && !dry && state.sent_slots.contains_key(&slot_key) {
        if let Some(output) = env::var_os("GITHUB_OUTPUT") {
            use std::io::Write;
            let mut file = fs::OpenOptions::new().append(true).open(output)?;
            writeln!(file, "delivery_skipped=true")?;
        }
        write_json(
            &gathered.dir.join("delivery-skipped.json"),
            &json!({"slot":slot_key,"already_delivered":true,"collection_retained":true}),
        )?;
        println!(
            "Slot {slot_key} already delivered; collection retained, no provider calls or sends"
        );
        return Ok(());
    }
    let weekly = schedule::candidates(&state, now);
    write_json(&gathered.dir.join("weekly-observations.json"), &weekly)?;
    let captured_instants = state
        .snapshots
        .iter()
        .filter_map(|snapshot| parse_time(&snapshot.captured_at))
        .filter(|time| *time >= now - Duration::days(7) && *time <= now)
        .collect::<std::collections::BTreeSet<_>>();
    let provenance = captured_instants.iter().map(|time| {
        let run = stamp(*time);
        let path = runs_dir().join(&run).join("manifest.json");
        match fs::read(&path).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()) {
            Some(manifest) => json!({"captured_at":time.to_rfc3339(),"run":run,"status":"included","manifest":manifest}),
            None => json!({"captured_at":time.to_rfc3339(),"run":run,"status":"gap","reason":"Retained observations exist but the source manifest is unavailable; complete source coverage cannot be asserted."})
        }
    }).collect::<Vec<_>>();
    write_json(
        &gathered.dir.join("weekly-source-manifest.json"),
        &json!({"captures":provenance,"note":"Every retained-state capture is included or explicitly marked as a gap. coverage.json additionally lists archive captures absent from retained state. Legacy per-run caps and source limits remain visible in original manifests; no history is backfilled."}),
    )?;
    let observed_dates = state
        .snapshots
        .iter()
        .filter_map(|snapshot| parse_time(&snapshot.captured_at))
        .filter(|time| *time >= now - Duration::days(7) && *time <= now)
        .map(|time| time.date_naive().to_string())
        .collect::<std::collections::BTreeSet<_>>();
    write_json(
        &gathered.dir.join("weekly-window.json"),
        &json!({
            "window_start":(now - Duration::days(7)).to_rfc3339(),
            "window_end":now.to_rfc3339(),
            "observed_repository_dates":observed_dates,
            "observed_capture_count":captured_instants.len(),
            "weekly_observed_repositories":weekly.len(),
            "note":"Historical observations are not backfilled; source failures/caps remain in each run manifest."
        }),
    )?;
    let candidates = editions::pending(&state, now, format);
    let (mut digest, output) = if format == editions::Format::Weekly {
        make_digest(&mut state, &candidates, &gathered.manifest, now, llm)?
    } else {
        make_digest_for(
            &mut state,
            &candidates,
            &gathered.manifest,
            now,
            llm,
            format,
        )?
    };
    editions::decorate(&mut digest, &candidates, now, format, &gathered.dir)?;
    record_coverage_summary(&digest["coverage"])?;
    save_preview_files(&gathered.dir, &digest, command == "preview" && llm)?;
    editions::remember_analyses(&mut state, &digest);
    write_json(&state_path(), &state)?;
    if matches!(command, "digest" | "debug") && !dry {
        println!("Prepared digest preview: {}", gathered.dir.display());
    } else {
        println!("{output}\n\nRaw run: {}", gathered.dir.display());
    }
    if command == "digest" && !dry {
        deliver_new_digest(
            &mut state,
            &mut digest,
            &gathered.dir,
            &digest_id,
            &day,
            &slot_key,
        )?;
    }
    if command == "debug" && !dry {
        let run_dir = gathered.dir.as_path();
        let audit = advisory_audit(&mut digest, &mut state, run_dir)?;
        let delivery = telegram_deliver_debug_digest(&digest, &digest_id, run_dir, &mut state)?;
        write_json(&state_path(), &state)?;
        if let Err(error) = telegram_pin_in(&delivery.chat_id, delivery.overview_message_id) {
            println!("Pin skipped: {error:#}");
        }
        println!(
            "Debug digest delivered to the technical chat; review status: {}",
            audit["status"]
        );
    }
    Ok(())
}

fn save_preview_files(run_dir: &Path, digest: &Value, require_editorial: bool) -> Result<()> {
    if require_editorial {
        editorial::require_synthesis(digest)?;
    }
    if let Some(report) = digest.get("coverage") {
        write_json(&run_dir.join("coverage.json"), report)?;
    }
    write_json(&run_dir.join("ranking-input.json"), digest)?;
    fs::write(run_dir.join("digest.md"), format!("{}\n", render(digest)))?;
    fs::write(run_dir.join("summary.md"), render_summary(digest))?;
    if digest["entries"]
        .as_array()
        .is_some_and(|entries| !entries.is_empty())
    {
        let messages = digest_publication_text(digest)?;
        fs::write(run_dir.join("issue.md"), render_issue_message(digest)?)?;
        let audit = publication_audit(digest)?;
        write_json(&run_dir.join("release-audit.json"), &audit)?;
        anyhow::ensure!(
            audit["approved"] == true,
            "preview blocked by credential scan"
        );
        write_json(&run_dir.join("telegram-messages.json"), &messages)?;
    }
    Ok(())
}

/// Local release checks cover payload formatting and credential patterns.
/// They do not review factual accuracy, project quality, or source coverage.
fn advisory_audit(digest: &mut Value, state: &mut State, run_dir: &Path) -> Result<Value> {
    let audit = publication_audit(digest)?;
    write_json(&run_dir.join("release-audit.json"), &audit)?;
    if audit["approved"] != true {
        tech_notice(
            "⛔ GitHub Digest: выпуск остановлен — детектор нашёл похожий на секрет текст. Кандидаты не помечены покрытыми.",
        );
        return Err(anyhow!("delivery blocked by deterministic credential scan"));
    }
    digest["audit"] = audit.clone();
    write_json(&run_dir.join("release-audit.json"), &audit)?;
    write_json(&state_path(), state)?;
    Ok(audit)
}

/// The preview and publication inspect the same complete public text.
/// This function never contacts Telegram, including when the scan fails.
fn publication_audit(digest: &Value) -> Result<Value> {
    let mut chunks = digest_publication_text(digest)?;
    let message_lengths = chunks
        .iter()
        .map(|text| text.encode_utf16().count())
        .collect::<Vec<_>>();
    if let Some(entries) = digest["entries"].as_array() {
        for entry in entries {
            for field in ["description", "joke"] {
                chunks.push(entry[field].as_str().unwrap_or("").to_string());
            }
        }
    }
    let matches = chunks
        .iter()
        .flat_map(|chunk| credential_markers(chunk))
        .collect::<Vec<_>>();
    Ok(json!({
        "approved":matches.is_empty(),
        "status":if matches.is_empty() { "deterministic-only" } else { "blocked" },
        "approval_scope":"Only local payload-format and credential-pattern checks; not factual, editorial, quality, safety or coverage approval",
        "telegram_format":{
            "status":"passed",
            "scope":telegram_format::VALIDATION_SCOPE,
            "utf16_limit":ISSUE_CHUNK_LIMIT,
            "message_utf16_units":message_lengths,
            "api_acceptance_tested":false
        },
        "factual_review":"not_performed",
        "deterministic_scan":if matches.is_empty() { "passed" } else { "failed" },
        "matched_pattern_classes":matches,
        "review":null
    }))
}

/// Delivery of one fresh issue into the main chat. The slot guard runs
/// first: at most one issue per week no matter how many runs fire.
/// Only local payload-format and credential checks run automatically.
/// On success every reviewed repository is marked covered and stays out of
/// all future issues. "Nothing new" stays silent in both chats.
fn deliver_new_digest(
    state: &mut State,
    digest: &mut Value,
    run_dir: &Path,
    digest_id: &str,
    day: &str,
    slot_key: &str,
) -> Result<()> {
    editions::Format::from_digest(digest)?;
    if state.sent_digests.contains_key(digest_id) {
        println!("Digest {digest_id} was already sent to the main chat; skipping");
        return Ok(());
    }
    if state.sent_slots.contains_key(slot_key) {
        println!("Slot {slot_key} already delivered an issue; staying quiet");
        return Ok(());
    }
    let dropped = trim_covered_entries(digest, state);
    if dropped > 0 {
        println!("{dropped} entries are already covered; trimmed before delivery");
        write_json(&run_dir.join("ranking-input.json"), digest)?;
    }
    if digest["entries"]
        .as_array()
        .is_none_or(|entries| entries.is_empty())
    {
        let sources = digest["manifest"]["sources"].as_array();
        let all_failed = sources.is_some_and(|sources| {
            !sources.is_empty() && sources.iter().all(|source| source["status"] != "included")
        });
        if all_failed {
            return Err(anyhow!(
                "every candidate source failed; refusing to report a quiet day"
            ));
        }
        // Тихий чек: отсутствие поста в основном чате и есть сигнал, тех-чат
        // не дёргаем.
        println!("No fresh candidates after deduplication; main chat stays quiet");
        return Ok(());
    }
    editorial::require_synthesis(digest)?;
    // The deterministic credential scan blocks unsafe publication text.
    let _ = advisory_audit(digest, state, run_dir)?;
    state
        .pending_digests
        .insert(digest_id.to_string(), digest.clone());
    write_json(&state_path(), state)?;
    telegram_deliver_digest(digest, digest_id, day, slot_key, state)?;
    let sent = digest["entries"]
        .as_array()
        .map(|entries| entries.len())
        .unwrap_or(0);
    println!("Digest {digest_id} sent to the main chat; {sent} repositories marked covered.");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::parse_project_description;

    fn snapshot(name: &str, captured_at: &str, source: Value) -> Snapshot {
        Snapshot {
            full_name: name.into(),
            captured_at: captured_at.into(),
            stars: Some(100),
            forks: Some(10),
            language: Some("Rust".into()),
            description: None,
            html_url: Some(format!("https://github.com/{name}")),
            topics: Vec::new(),
            created_at: None,
            updated_at: None,
            pushed_at: None,
            license: None,
            open_issues: None,
            default_branch: None,
            sources: vec![source],
            readme_excerpt: None,
            observation: None,
        }
    }

    #[test]
    fn evidence_quotes_are_exact_and_fall_back_to_readme_text() {
        let description = "  A real, source-backed project description.  ";
        assert_eq!(
            exact_evidence_quote(Some(description), None).as_deref(),
            Some("A real, source-backed project description.")
        );
        let readme = "# Project\n\n![logo](logo.png)\n\nFirst documented paragraph with useful implementation context.\n\nSecond paragraph.";
        let quote = exact_evidence_quote(None, Some(readme)).unwrap();
        assert_eq!(
            quote,
            "First documented paragraph with useful implementation context."
        );
        assert!(readme.contains(&quote));
    }

    #[test]
    fn public_delivery_redacts_credentials_and_builds_private_channel_links() {
        assert_eq!(
            safe_public_text("example sk-abcdefghijklmnopqrstuvwxyz0123456789"),
            "example [скрыто]"
        );
        assert_eq!(
            telegram_message_link("-1003944093514", 42).unwrap(),
            "https://t.me/c/3944093514/42"
        );
    }

    #[test]
    fn deterministic_release_scan_catches_credentials_without_logging_values() {
        let text = "debug response gsk_abcdefghijklmnopqrstuvwxyz0123456789";
        let matches = credential_markers(text);
        assert!(matches.contains(&"groq-key"));
        assert!(!matches.iter().any(|marker| text.contains(marker)));

        let telegram_token = "9988776655:abcdefghijklmnopqrstuvwxyzABCDEFGH123456";
        assert!(credential_markers(telegram_token).contains(&"telegram-bot-token-pattern"));
        assert!(credential_markers("Normal Rust project overview").is_empty());
    }

    #[test]
    fn project_description_parser_accepts_description_and_joke_only() {
        let complete = "Инструмент собирает данные репозитория и показывает изменения за неделю. Он читает GitHub API и формирует отдельный отчёт для каждого проекта. Это полезно для оценки новых инструментов перед внедрением в команду.";
        let joke = "Дайджест читает репозитории, пока вы читаете дайджест.";
        assert_eq!(
            parse_project_description(&json!({"description":complete,"joke":joke}).to_string())
                .unwrap(),
            (complete.to_string(), joke.to_string())
        );
        assert!(parse_project_description("{\"summary\":\"не описание\"}").is_err());
        assert!(
            parse_project_description(
                "{\"description\":\"Описание\",\"joke\":\"Шутка\",\"joke2\":\"лишнее\"}"
            )
            .is_err()
        );
        assert!(
            parse_project_description(
                "{\"description\":\"Слишком коротко\",\"joke\":\"Шутка без разбора\"}"
            )
            .is_err()
        );
        assert_eq!(
            parse_project_description(&json!({"description":complete,"joke":""}).to_string())
                .unwrap()
                .1,
            ""
        );
        assert!(
            parse_project_description(
                &json!({"description":"я".repeat(1601),"joke":""}).to_string()
            )
            .is_err()
        );
    }

    #[test]
    fn issue_items_keep_full_text_with_compact_tags_and_metrics() {
        let description = [
            "Инструмент для проверки репозитория. ".repeat(30).trim(),
            "Показывает найденные зависимости и помогает оценить объём изменений.",
            "Результат проверки можно использовать при подготовке обновления.",
        ]
        .join("\n\n");
        let project = json!({
            "rank": 1,
            "total": 1,
            "source_kind": "trending_weekly",
            "full_name": "org/repo",
            "delta": 120,
            "metric_captured_at":"2026-10-20T12:17:08Z",
            "description": description,
            "joke": "Проверку прошёл даже этот пример.",
            "latest": {"html_url":"https://github.com/org/repo","language":"Rust","stars":500,
                "captured_at":"2026-10-24T20:53:42Z"}
        });
        let item = issue_item(&project);
        assert!(item.contains("[org/repo](https://github.com/org/repo)"));
        assert!(item.contains("Trending: макс\\. \\+120 за неделю"));
        assert!(item.contains("\\#Rust · ⭐ 500"));
        assert!(!item.contains("метаданные"));
        assert!(!item.contains("срез метрики"));
        assert!(!item.contains("2026"));
        assert!(item.contains(&markdown_v2_escape(
            project["description"].as_str().unwrap().trim()
        )));
        assert!(item.contains("Проверку прошёл даже этот пример\\."));
        assert!(telegram_format::validate_message(&item, ISSUE_CHUNK_LIMIT).is_ok());
        assert!(!item.contains("ПРОЕКТ"));
        assert_eq!(markdown_v2_escape("a_b.c"), "a\\_b\\.c");
        let scoped_package = render_markdown_v2_inline(
            "Установите @alibaba-group/open-code-review, затем выполните `npm install @alibaba-group/open-code-review`.",
        );
        assert!(scoped_package.contains("`@alibaba-group/open-code-review`,"));
        assert!(scoped_package.contains("`npm install @alibaba-group/open-code-review`"));
        assert!(!scoped_package.contains("@alibaba-group/open-code-review,"));
        assert!(telegram_format::validate_message(&scoped_package, ISSUE_CHUNK_LIMIT).is_ok());
        assert!(
            telegram_format::validate_message(
                &render_markdown_v2_inline("Source with empty `` or an unmatched ` marker."),
                ISSUE_CHUNK_LIMIT
            )
            .is_ok()
        );
        assert!(project_github_url("org/repo/extra").is_none());
    }

    #[test]
    fn issues_split_between_complete_cards_without_compressing_descriptions() {
        let entries = (1..=10)
            .map(|rank| {
                json!({
                    "rank":rank,
                    "total":10,
                    "source_kind":"trending_weekly",
                    "full_name":format!("org/repo-{rank}"),
                    "delta":100,
                    "description":"Конкретный разбор проекта с задачей, механизмом и примером применения.",
                    "llm_used":true,
                    "latest":{"stars":500,"language":"Rust"}
                })
            })
            .collect::<Vec<_>>();
        let digest = json!({
            "format_version":3,
            "llm_used":true,
            "generated_at":"2026-09-28T00:00:00Z",
            "trend_summary":"Срез недельного внимания",
            "entries":entries
        });
        let chunks = digest_publication_text(&digest).unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].starts_with("*GitHub — подборка недели*"));
        assert!(chunks[0].contains("10 проектов"));
        assert!(chunks[0].contains("[org/repo\\-1](https://github.com/org/repo-1)"));
        assert!(chunks[0].contains("28\\.09\\.2026"));
        assert!(!chunks.join("\n").contains("Порядок:"));
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.encode_utf16().count() <= ISSUE_CHUNK_LIMIT)
        );
        // A large issue keeps every complete description and joke, even when
        // several messages are required. No 280/140/60/0 compression remains.
        let heavy = json!({
            "format_version":3,
            "llm_used":false,
            "generated_at":"2026-09-28T00:00:00Z",
            "entries":(1..=20).map(|rank| {
                json!({
                    "rank":rank,
                    "source_kind":"trending_weekly",
                    "full_name":format!("org/big-{rank:02}"),
                    "delta":rank*10,
                    "description":format!("Описание {rank}: {} конец описания.", "х".repeat(500)),
                    "joke":format!("Шутка {rank} без потери текста."),
                    "latest":{"stars":500,"language":"Rust"}
                })
            }).collect::<Vec<_>>()
        });
        let heavy_chunks = digest_publication_text(&heavy).unwrap();
        assert!(heavy_chunks.len() > 1);
        assert!(
            heavy_chunks
                .iter()
                .all(|chunk| chunk.encode_utf16().count() <= ISSUE_CHUNK_LIMIT)
        );
        for entry in heavy["entries"].as_array().unwrap() {
            let complete_card = issue_item(entry);
            assert_eq!(
                heavy_chunks
                    .iter()
                    .filter(|chunk| chunk.contains(&complete_card))
                    .count(),
                1
            );
        }
        assert!(!heavy_chunks.join("\n").contains('…'));
        let preview = render(&digest);
        assert!(!preview.contains("ПРОЕКТ"));
    }

    #[test]
    fn an_oversized_individual_card_fails_instead_of_removing_its_description() {
        let heavy = json!({
            "format_version":3,
            "generated_at":"2026-09-28T00:00:00Z",
            "entries":[{
                    "rank":1,
                    "source_kind":"new_repo",
                    "full_name":"org/wide",
                    "delta":10,
                    "description":"😀".repeat(1900),
                    "latest":{"stars":500,"language":"Rust"}
            }]
        });
        let error = digest_publication_text(&heavy).unwrap_err().to_string();
        assert!(error.contains("complete project card exceeds"));
        assert!(publication_audit(&heavy).is_err());
    }

    #[test]
    fn delayed_and_manual_runs_share_the_same_weekly_slot() {
        let monday = parse_time("2026-10-19T07:00:00Z").unwrap();
        let friday = parse_time("2026-10-23T22:00:00Z").unwrap();
        assert_eq!(slot_key_of(monday), slot_key_of(friday));
        assert_ne!(slot_key_of(monday), slot_key_of(monday + Duration::days(7)));
    }
    #[test]
    fn project_tags_are_valid_and_search_provenance_is_not_public() {
        for (language, expected) in [
            ("Python", "#Python"),
            ("C++", "#CPP"),
            ("C#", "#CSharp"),
            ("F#", "#FSharp"),
            ("Objective-C", "#ObjectiveC"),
            ("Objective-C++", "#ObjectiveCPP"),
            ("Jupyter Notebook", "#JupyterNotebook"),
        ] {
            let entry = json!({
                "full_name":"org/project", "source_kind":"new_repo", "delta":1234,
                "description":"Инструмент проверяет зависимости проекта.", "joke":"",
                "llm_used":false, "latest":{"language":language,"stars":1234}
            });
            let card = issue_item(&entry);
            assert!(card.ends_with(&format!("{} · ⭐ 1 234", markdown_v2_escape(expected))));
            for clutter in [
                "из поиска",
                "метаданные",
                "Trending",
                "Исходное описание",
                "😄",
            ] {
                assert!(!card.contains(clutter), "{card}");
            }
            telegram_format::validate_message(&card, ISSUE_CHUNK_LIMIT).unwrap();
        }
        for language in ["", " ", "++", "123"] {
            assert!(language_hashtag(language).is_none());
        }
        let mut unknown = json!({
            "full_name":"org/project", "description":"Описание проекта.",
            "latest":{"language":null,"stars":null}
        });
        assert!(issue_item(&unknown).ends_with(r"Описание проекта\."));
        unknown["latest"]["stars"] = json!(0);
        assert!(issue_item(&unknown).ends_with("⭐ 0"));
    }

    #[test]
    fn previews_and_delivery_have_only_project_messages_even_with_a_legacy_overview() {
        let directory = tempfile::tempdir().unwrap();
        let digest = json!({
            "format_version":3, "edition":"daily", "generated_at":"2026-11-04T15:00:00Z",
            "overview":"Legacy summary that must not be rendered.",
            "entries":[{
                "full_name":"org/project", "source_kind":"new_repo",
                "description":"Инструмент проверяет зависимости проекта.",
                "latest":{"language":"Python","stars":1234}
            }]
        });
        let chunks = digest_publication_text(&digest).unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].starts_with("*GitHub — находки дня*"));
        for clutter in [
            "Итог выпуска",
            "Legacy summary",
            "очеред",
            "Порядок:",
            "из поиска",
        ] {
            assert!(!chunks[0].contains(clutter));
        }
        save_preview_files(directory.path(), &digest, false).unwrap();
        let preview: Vec<String> = serde_json::from_slice(
            &fs::read(directory.path().join("telegram-messages.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(preview, chunks);
        let audit = publication_audit(&digest).unwrap();
        assert_eq!(
            audit["telegram_format"]["message_utf16_units"]
                .as_array()
                .unwrap()
                .len(),
            chunks.len()
        );
        let mut state = State::default();
        let mut sent = Vec::new();
        let ids = delivery::send_chunks(
            &chunks,
            "chat:format-test",
            &mut state,
            |text| {
                sent.push(text.to_string());
                Ok(42)
            },
            |_| Ok(()),
        )
        .unwrap();
        assert_eq!(sent, preview);
        assert_eq!(ids, [42]);
        assert!(state.overview_ids.is_empty());
        delivery::send_chunks(
            &chunks,
            "chat:format-test",
            &mut state,
            |_| panic!("acknowledged cards must not be sent again"),
            |_| Ok(()),
        )
        .unwrap();
    }

    #[test]
    fn zero_and_unknown_star_counts_do_not_exclude_projects() {
        let mut young = snapshot(
            "org/young",
            "2026-10-19T07:00:00Z",
            json!({"type":"trending_weekly","trending_stars_period":12}),
        );
        young.stars = Some(0);
        let mut unknown = young.clone();
        unknown.full_name = "org/unknown".into();
        unknown.stars = None;
        let (selected, excluded) = select_candidates(&[young, unknown], &BTreeMap::new());
        assert_eq!(selected.len(), 2);
        assert!(excluded.is_empty());
    }

    #[test]
    fn source_audit_retains_provenance_and_checks_unescaped_card_credentials() {
        let mut digest = json!({
            "format_version":3,
            "overview":"На этой неделе появились инструменты для разбора данных.",
            "period_start":"2026-10-12T07:00:00Z",
            "period_end":"2026-10-19T07:00:00Z",
            "entries":[{
                "full_name":"org/young","description":"Documented data parser.",
                "period":"GitHub Trending: звёзды за неделю","delta":12,
                "latest":{"stars":12,"captured_at":"2026-10-18T07:00:00Z"}
            }]
        });
        let preview = render_summary(&digest);
        assert!(!preview.contains(digest["overview"].as_str().unwrap()));
        assert!(preview.contains("[org/young](https://github.com/org/young)"));
        assert!(preview.contains("2026-10-18T07:00:00Z"));
        let approved = publication_audit(&digest).unwrap();
        assert_eq!(approved["approved"], true);
        assert_eq!(approved["factual_review"], "not_performed");
        assert_eq!(approved["telegram_format"]["status"], "passed");
        assert_eq!(approved["telegram_format"]["api_acceptance_tested"], false);
        digest["entries"][0]["description"] = json!(format!("Example ghp_{}", "a".repeat(40)));
        let audit = publication_audit(&digest).unwrap();
        assert_eq!(audit["approved"], false);
        assert!(!audit.to_string().contains(&"a".repeat(40)));
    }

    #[test]
    fn obsolete_overview_cannot_block_valid_project_previews() {
        let directory = tempfile::tempdir().unwrap();
        let digest = json!({
            "format_version":3,
            "overview":"😀".repeat(1900),
            "entries":[{
                "full_name":"org/project", "description":"Source-backed project description.",
                "source_kind":"new_repo", "latest":{"stars":5}
            }]
        });
        let messages = digest_publication_text(&digest).unwrap();
        assert_eq!(messages.len(), 1);
        assert!(!messages[0].contains('😀'));
        save_preview_files(directory.path(), &digest, false).unwrap();
        assert!(directory.path().join("telegram-messages.json").exists());
    }

    #[test]
    fn retry_selection_accepts_recorded_pool_and_rejects_changed_ranking() {
        let directory = tempfile::tempdir().unwrap();
        let rows = json!([{"full_name":"org/young"}]);
        let digest = json!({"entries":[{"full_name":"org/young"}]});
        for selection in [rows.clone(), json!({"pool":rows,"excluded":[]})] {
            write_json(&directory.path().join("selection.json"), &selection).unwrap();
            assert!(validate_digest_selection(directory.path(), &digest).is_ok());
            let changed = json!({"entries":[{"full_name":"org/other"}]});
            assert!(validate_digest_selection(directory.path(), &changed).is_err());
        }
    }

    #[test]
    fn a_full_week_keeps_every_repository_in_bounded_telegram_messages() {
        let entries = (0..30)
            .map(|index| {
                json!({
                    "full_name":format!("org/project-{index}-{}", "w".repeat(60)),
                    "description":"Подробное описание проекта 😀 ".repeat(60),
                    "source_kind":"trending_weekly","delta":123,
                    "latest":{"stars":42,"language":"Rust","captured_at":"2026-10-19T07:00:00Z"}
                })
            })
            .collect::<Vec<_>>();
        let digest = json!({"format_version":3,"entries":entries,
            "overview":"На этой неделе появились новые инструменты разработки."});
        let messages = digest_publication_text(&digest).unwrap();
        assert!(messages.len() > 1);
        assert!(
            messages
                .iter()
                .all(|text| text.encode_utf16().count() <= ISSUE_CHUNK_LIMIT)
        );
        let text = messages.join("\n");
        for entry in digest["entries"].as_array().unwrap() {
            assert!(
                text.contains(&project_github_url(entry["full_name"].as_str().unwrap()).unwrap())
            );
        }
        assert_eq!(publication_audit(&digest).unwrap()["approved"], true);
    }

    #[test]
    fn selection_prefers_weekly_then_daily_then_new_and_skips_covered() {
        let now = Utc::now();
        let captured = now.to_rfc3339();
        let mut snapshots = (0..11)
            .map(|n| {
                snapshot(
                    &format!("org/week-{n:02}"),
                    &captured,
                    json!({"type":"trending_weekly","trending_stars_period":n*100}),
                )
            })
            .map(|mut s| {
                s.stars = Some(20_000);
                s
            })
            .collect::<Vec<_>>();
        snapshots.push(snapshot(
            "org/daily-hot",
            &captured,
            json!({"type":"trending_daily","trending_stars_period":100_000}),
        ));
        snapshots.last_mut().unwrap().stars = Some(30_000);
        snapshots.push(snapshot(
            "org/new-but-big",
            &captured,
            json!({"type":"new_repo_search","stars":9_000}),
        ));
        snapshots.last_mut().unwrap().stars = Some(9_000);
        // A young project remains eligible regardless of its total stars.
        snapshots.push(snapshot(
            "org/small-hype",
            &captured,
            json!({"type":"trending_weekly","trending_stars_period":50_000}),
        ));
        snapshots.last_mut().unwrap().stars = Some(400);
        let mut covered = BTreeMap::new();
        covered.insert("org/week-10".to_string(), "2026-09-20".to_string());
        covered.insert("org/week-09".to_string(), "2026-09-20".to_string());

        let (selected, excluded) = select_candidates(&snapshots, &covered);

        // The issue is the whole fresh difference: ten uncovered weekly
        // candidates by +N desc, then the daily one, then the new one.
        // Covered repositories do not consume slots; young projects remain eligible.
        assert_eq!(selected.len(), 12);
        assert_eq!(selected[0].snapshot.full_name, "org/small-hype");
        assert_eq!(selected[1].snapshot.full_name, "org/week-08");
        assert_eq!(selected[9].snapshot.full_name, "org/week-00");
        assert_eq!(selected[10].snapshot.full_name, "org/daily-hot");
        assert_eq!(selected[11].snapshot.full_name, "org/new-but-big");
        assert_eq!(
            selected[0].period,
            "GitHub Trending: максимум наблюдавшихся звёзд за неделю".to_string()
        );
        assert_eq!(
            selected[10].period,
            "GitHub Trending: максимум наблюдавшихся звёзд за день".to_string()
        );
        assert!(excluded.iter().any(
            |row| row["full_name"] == "org/week-10" && row["source_kind"] == "trending_weekly"
        ));
        assert!(excluded.iter().any(
            |row| row["full_name"] == "org/week-09" && row["source_kind"] == "trending_weekly"
        ));
        assert!(
            !excluded
                .iter()
                .any(|row| row["full_name"] == "org/new-but-big")
        );
    }
}
