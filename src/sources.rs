//! GitHub collection and provenance; selection inputs and transport are explicit.
#[cfg(test)]
mod tests;
use crate::clock::Clock;
use crate::{MAX_CANDIDATES, Snapshot, source_priority, stamp, write_json};
use anyhow::{Result, anyhow};
use chrono::{DateTime, Duration, Utc};
use reqwest::blocking::Client;
use reqwest::header::{ACCEPT, USER_AGENT};
use scraper::{Html, Selector};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    time::Duration as StdDuration,
};

const API: &str = "https://api.github.com";
const README_LIMIT: usize = 5_000;

pub(super) trait SourceHttp {
    fn get(&self, url: &str, raw: bool, max_bytes: Option<usize>) -> Result<Vec<u8>>;
}

pub(super) struct GitHubHttp<'a> {
    client: Client,
    token: Option<String>,
    clock: &'a dyn Clock,
}

impl<'a> GitHubHttp<'a> {
    pub(super) fn from_env(clock: &'a dyn Clock) -> Result<Self> {
        Ok(Self {
            client: crate::client()?,
            token: env::var("GITHUB_TOKEN")
                .or_else(|_| env::var("GH_TOKEN"))
                .ok(),
            clock,
        })
    }
}

#[derive(Clone)]
struct Candidate {
    name: String,
    sources: Vec<Value>,
}

pub(super) struct RunData {
    pub(super) dir: PathBuf,
    pub(super) manifest: Value,
    pub(super) snapshots: Vec<Snapshot>,
}

impl SourceHttp for GitHubHttp<'_> {
    fn get(&self, url: &str, raw: bool, max_bytes: Option<usize>) -> Result<Vec<u8>> {
        let client = &self.client;
        let mut last = None;
        for attempt in 0..=2 {
            let mut req = client
                .get(url)
                .header(USER_AGENT, "github-weekly-digest-bot/0.2");
            req = if raw && url.contains("github.com/trending") {
                req.header(ACCEPT, "text/html,application/xhtml+xml")
            } else if raw {
                req.header(ACCEPT, "text/plain")
            } else {
                req.header(ACCEPT, "application/vnd.github+json")
            };
            if let Some(token) = &self.token
                && !raw
            {
                req = req.bearer_auth(token);
            }
            match req.send() {
                Ok(resp) if resp.status().is_success() => {
                    let bytes = resp.bytes()?;
                    return Ok(match max_bytes {
                        Some(n) => bytes.iter().take(n).copied().collect(),
                        None => bytes.to_vec(),
                    });
                }
                Ok(resp) => {
                    let status = resp.status();
                    let body = resp.text().unwrap_or_default();
                    let error = anyhow!(
                        "HTTP {status}: {}",
                        body.chars().take(300).collect::<String>()
                    );
                    if status == 403 || status == 429 || !status.is_server_error() {
                        return Err(error);
                    }
                    last = Some(error);
                }
                Err(e) => last = Some(anyhow!(e)),
            }
            if attempt < 2 {
                self.clock.sleep(StdDuration::from_secs(2_u64.pow(attempt)));
            }
        }
        Err(last.unwrap_or_else(|| anyhow!("request failed")))
    }
}

fn parse_count(text: &str) -> Option<i64> {
    let text = text.trim().replace(',', "");
    let mut number = String::new();
    let mut suffix = None;
    for c in text.chars() {
        if c.is_ascii_digit() || (c == '.' && !number.contains('.')) {
            number.push(c);
        } else if !number.is_empty() && matches!(c.to_ascii_lowercase(), 'k' | 'm') {
            suffix = Some(c.to_ascii_lowercase());
            break;
        } else if !number.is_empty() {
            break;
        }
    }
    let mut n = number.parse::<f64>().ok()?;
    n *= match suffix {
        Some('k') => 1000.0,
        Some('m') => 1_000_000.0,
        _ => 1.0,
    };
    Some(n as i64)
}

fn text(el: Option<scraper::ElementRef<'_>>) -> String {
    el.map(|x| {
        x.text()
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    })
    .unwrap_or_default()
}

fn parse_trending(raw: &str) -> Vec<Value> {
    let document = Html::parse_document(raw);
    let article = Selector::parse("article.Box-row").unwrap();
    let title = Selector::parse("h2 a[href]").unwrap();
    let desc = Selector::parse("p").unwrap();
    let language = Selector::parse("span[itemprop='programmingLanguage']").unwrap();
    let stars = Selector::parse("a[href$='/stargazers']").unwrap();
    let forks = Selector::parse("a[href$='/network/members']").unwrap();
    let weekly = Selector::parse("span.d-inline-block.float-sm-right").unwrap();
    document.select(&article).filter_map(|row| {
        let href = row.select(&title).next()?.value().attr("href")?;
        let name = href.trim_matches('/');
        if name.split('/').count() != 2 { return None; }
        let d = row.select(&desc).next();
        Some(json!({
            "full_name": name,
            "description": text(d),
            "language": text(row.select(&language).next()),
            "trending_total_stars": row.select(&stars).next().and_then(|x| parse_count(&text(Some(x)))),
            "trending_total_forks": row.select(&forks).next().and_then(|x| parse_count(&text(Some(x)))),
            "trending_stars_period": row.select(&weekly).next().and_then(|x| parse_count(&text(Some(x))))
        }))
    }).collect()
}

fn source_entry(source: &str, url: &str, pagination: &str) -> Value {
    json!({"source":source,"url":url,"fetched_at":Value::Null,"complete":false,"pagination":pagination,"status":"error","reason":"not fetched"})
}

pub(super) fn collect(
    now: DateTime<Utc>,
    runs: &Path,
    covered: &BTreeMap<String, String>,
    http: &dyn SourceHttp,
    clock: &dyn Clock,
) -> Result<RunData> {
    let dir = runs.join(stamp(now));
    fs::create_dir_all(&dir)?;
    let mut sources = Vec::<Value>::new();
    let mut candidates = BTreeMap::<String, Candidate>::new();
    for period in ["weekly", "daily"] {
        let url = format!("https://github.com/trending?since={period}");
        let mut entry = source_entry(
            &format!("GitHub Trending {period}"),
            &url,
            "HTML first page as returned; no pagination",
        );
        match http.get(&url, true, None) {
            Ok(bytes) => {
                fs::write(dir.join(format!("trending-{period}.html")), &bytes)?;
                let html = String::from_utf8_lossy(&bytes);
                let rows = parse_trending(&html);
                entry["fetched_at"] = json!(
                    clock
                        .now()
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                );
                entry["parsed_candidates"] = json!(rows.len());
                entry["complete"] = json!(!rows.is_empty());
                entry["status"] = json!(if rows.is_empty() { "error" } else { "included" });
                entry["reason"] = json!(if rows.is_empty() {
                    "page fetched but no repository rows parsed"
                } else {
                    "repository rows parsed from first page"
                });
                for row in rows {
                    let name = row["full_name"].as_str().unwrap_or_default().to_string();
                    let c = candidates.entry(name.clone()).or_insert_with(|| Candidate {
                        name,
                        sources: vec![],
                    });
                    let mut source = row.clone();
                    source["type"] = json!(format!("trending_{period}"));
                    c.sources.push(source);
                }
            }
            Err(e) => {
                entry["fetched_at"] = json!(
                    clock
                        .now()
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                );
                entry["reason"] = json!(format!("fetch failed: {e:#}"));
            }
        }
        sources.push(entry);
    }
    let from = (now - Duration::days(7)).date_naive();
    let query = format!("created:>={}", from.format("%Y-%m-%d"));
    let mut url = reqwest::Url::parse(&format!("{API}/search/repositories"))?;
    url.query_pairs_mut()
        .append_pair("q", &query)
        .append_pair("sort", "stars")
        .append_pair("order", "desc")
        .append_pair("per_page", "30");
    let mut search_entry = source_entry(
        "GitHub REST Search repositories created in last 7 days",
        url.as_str(),
        "per_page=30; first page only; Search API result cap applies",
    );
    search_entry["query"] = json!(query);
    let mut api_blocked = false;
    match http.get(url.as_str(), false, None) {
        Ok(bytes) => {
            fs::write(dir.join("search-new-repos.json"), &bytes)?;
            let val: Value = serde_json::from_slice(&bytes)?;
            let items = val["items"].as_array().cloned().unwrap_or_default();
            search_entry["fetched_at"] = json!(
                clock
                    .now()
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            );
            search_entry["complete"] = json!(items.len() < 30);
            search_entry["status"] = json!("included");
            search_entry["reason"] = json!("first page ingested as candidate source only");
            search_entry["total_count"] = val.get("total_count").cloned().unwrap_or(Value::Null);
            search_entry["returned"] = json!(items.len());
            for item in items {
                if let Some(name) = item["full_name"].as_str() {
                    let c = candidates
                        .entry(name.to_string())
                        .or_insert_with(|| Candidate {
                            name: name.to_string(),
                            sources: vec![],
                        });
                    c.sources.push(json!({
                        "type":"new_repo_search",
                        "stars": item["stargazers_count"].as_i64(),
                    }));
                }
            }
        }
        Err(e) => {
            let m = format!("{e:#}");
            api_blocked = m.contains("HTTP 403") || m.contains("HTTP 429");
            search_entry["fetched_at"] = json!(
                clock
                    .now()
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            );
            search_entry["reason"] = json!(format!("fetch failed: {m}"));
        }
    }
    sources.push(search_entry);
    let mut all: Vec<Candidate> = candidates.into_values().collect();
    all.sort_by(|a, b| {
        let candidate_key = |candidate: &Candidate| {
            let weekly = candidate
                .sources
                .iter()
                .find(|source| source["type"] == "trending_weekly");
            let daily = candidate
                .sources
                .iter()
                .find(|source| source["type"] == "trending_daily");
            let (priority, metric) = if let Some(source) = weekly {
                (0, source["trending_stars_period"].as_i64().unwrap_or(0))
            } else if let Some(source) = daily {
                (1, source["trending_stars_period"].as_i64().unwrap_or(0))
            } else {
                (2, 0)
            };
            (priority, -metric, candidate.name.clone())
        };
        candidate_key(a).cmp(&candidate_key(b))
    });
    // Coverage history controls deduplication; total stars never exclude a project.
    let (fresh, rejected): (Vec<&Candidate>, Vec<&Candidate>) = all
        .iter()
        .partition(|candidate| !covered.contains_key(&candidate.name));
    let mut excluded: Vec<Value> = rejected
        .iter()
        .map(|candidate| {
            json!({"full_name":candidate.name,"reason":format!(
            "already reviewed in the digest on {}", covered[&candidate.name])})
        })
        .collect();
    let selected: Vec<Candidate> = fresh
        .iter()
        .take(MAX_CANDIDATES)
        .map(|candidate| (*candidate).clone())
        .collect();
    excluded.extend(fresh.iter().skip(MAX_CANDIDATES).map(
        |x| json!({"full_name":x.name,"reason":"candidate cap after stable source priority"}),
    ));
    let mut api_records = Vec::new();
    let mut snapshots = Vec::new();
    for candidate in &selected {
        let url = format!("{API}/repos/{}", candidate.name);
        let mut info = source_entry(
            "GitHub REST repository metadata",
            &url,
            "single repository response",
        );
        if api_blocked {
            info["fetched_at"] = json!(
                clock
                    .now()
                    .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
            );
            info["status"] = json!("excluded");
            info["reason"] = json!("skipped after GitHub API rate-limit response");
            api_records.push(info);
            continue;
        }
        match http.get(&url, false, None) {
            Ok(bytes) => {
                fs::write(
                    dir.join(format!("repo-{}.json", safe_name(&candidate.name))),
                    &bytes,
                )?;
                let v: Value = serde_json::from_slice(&bytes)?;
                let snap = Snapshot {
                    full_name: candidate.name.clone(),
                    captured_at: now.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                    stars: v["stargazers_count"].as_i64(),
                    forks: v["forks_count"].as_i64(),
                    language: v["language"].as_str().map(str::to_string),
                    description: v["description"].as_str().map(str::to_string),
                    html_url: v["html_url"].as_str().map(str::to_string),
                    topics: v["topics"]
                        .as_array()
                        .map(|x| {
                            x.iter()
                                .filter_map(|i| i.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default(),
                    created_at: v["created_at"].as_str().map(str::to_string),
                    updated_at: v["updated_at"].as_str().map(str::to_string),
                    pushed_at: v["pushed_at"].as_str().map(str::to_string),
                    license: v["license"]["spdx_id"]
                        .as_str()
                        .filter(|id| *id != "NOASSERTION")
                        .or_else(|| v["license"]["name"].as_str())
                        .map(str::to_string),
                    open_issues: v["open_issues_count"].as_i64(),
                    default_branch: v["default_branch"].as_str().map(str::to_string),
                    sources: candidate.sources.clone(),
                    readme_excerpt: None,
                };
                info["fetched_at"] = json!(
                    clock
                        .now()
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                );
                info["complete"] = json!(true);
                info["status"] = json!("included");
                info["reason"] = json!("metadata captured");
                info["stars"] = json!(snap.stars);
                info["forks"] = json!(snap.forks);
                snapshots.push(snap);
            }
            Err(e) => {
                let m = format!("{e:#}");
                if m.contains("HTTP 403") || m.contains("HTTP 429") {
                    api_blocked = true;
                }
                info["fetched_at"] = json!(
                    clock
                        .now()
                        .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                );
                info["reason"] = json!(format!("fetch failed: {m}"));
            }
        }
        api_records.push(info);
        clock.sleep(StdDuration::from_millis(100));
    }
    let mut readme_names = Vec::<String>::new();
    let mut readme_candidates: Vec<&Snapshot> = snapshots
        .iter()
        .filter(|snapshot| !covered.contains_key(&snapshot.full_name))
        .collect();
    readme_candidates.sort_by(|a, b| {
        let ka = source_priority(a);
        let kb = source_priority(b);
        ka.0.cmp(&kb.0)
            .then(kb.1.cmp(&ka.1))
            .then(a.full_name.cmp(&b.full_name))
    });
    readme_names.extend(
        readme_candidates
            .iter()
            .take(15)
            .map(|snapshot| snapshot.full_name.clone()),
    );
    for name in readme_names {
        if let Some(snap) = snapshots.iter_mut().find(|s| s.full_name == name) {
            let branch = snap
                .default_branch
                .clone()
                .unwrap_or_else(|| "main".to_string());
            let mut parts = name.splitn(2, '/');
            let owner = parts.next().unwrap_or("");
            let repo = parts.next().unwrap_or("");
            let url = format!(
                "https://raw.githubusercontent.com/{}/{}/{}/README.md",
                url::form_urlencoded::byte_serialize(owner.as_bytes()).collect::<String>(),
                url::form_urlencoded::byte_serialize(repo.as_bytes()).collect::<String>(),
                url::form_urlencoded::byte_serialize(branch.as_bytes())
                    .collect::<String>()
                    .replace("%2F", "/")
            );
            let mut info = source_entry(
                "GitHub raw README.md",
                &url,
                &format!(
                    "root README.md on default branch; excerpt capped at {README_LIMIT}; transfer capped at 1MB"
                ),
            );
            info["candidate"] = json!(name);
            match http.get(&url, true, Some(1_000_000)) {
                Ok(bytes) => {
                    fs::write(
                        dir.join(format!("readme-{}.md", safe_name(&snap.full_name))),
                        &bytes,
                    )?;
                    let full = String::from_utf8_lossy(&bytes).to_string();
                    let excerpt: String = full.chars().take(README_LIMIT).collect();
                    info["fetched_at"] = json!(
                        clock
                            .now()
                            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                    );
                    info["complete"] = json!(true);
                    info["status"] = json!("included");
                    info["reason"] = json!("raw README captured; bounded excerpt used for model");
                    info["chars"] = json!(excerpt.chars().count());
                    snap.readme_excerpt = Some(excerpt);
                }
                Err(e) => {
                    info["fetched_at"] = json!(
                        clock
                            .now()
                            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
                    );
                    info["reason"] = json!(format!("fetch failed: {e:#}"));
                }
            }
            sources.push(info);
        }
    }
    let manifest = json!({"captured_at":now.to_rfc3339_opts(chrono::SecondsFormat::Secs,true),"sources":sources,"candidate_policy":format!("Weekly Trending, daily Trending, then Search; stable order; max {MAX_CANDIDATES}; not a global GitHub census."),"candidate_count":selected.len(),"excluded_candidates":excluded,"api_repositories":api_records,"validation_notes":[]});
    write_json(&dir.join("manifest.json"), &manifest)?;
    Ok(RunData {
        dir,
        manifest,
        snapshots,
    })
}

fn safe_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect()
}
