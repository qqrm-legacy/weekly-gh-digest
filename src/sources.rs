//! GitHub collection and provenance; selection inputs and transport are explicit.
#[cfg(test)]
mod tests;
use crate::clock::Clock;
use crate::timestamp::Timestamp;
use crate::{Snapshot, stamp, write_json};
use anyhow::{Result, anyhow};
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
use time::SignedDuration as Duration;

const API: &str = "https://api.github.com";
const README_LIMIT: usize = 5_000;

#[derive(Debug)]
struct HttpStatusError {
    status: reqwest::StatusCode,
    body: String,
}

impl std::fmt::Display for HttpStatusError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "HTTP {}: {}", self.status, self.body)
    }
}

impl std::error::Error for HttpStatusError {}

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
                    let error = anyhow!(HttpStatusError {
                        status,
                        body: body.chars().take(300).collect(),
                    });
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
    now: Timestamp,
    runs: &Path,
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
                entry["fetched_at"] = json!(clock.now().rfc3339_seconds());
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
                entry["fetched_at"] = json!(clock.now().rfc3339_seconds());
                entry["reason"] = json!(format!("fetch failed: {e:#}"));
            }
        }
        sources.push(entry);
    }
    let from = (now - Duration::days(7)).date();
    let query = format!("created:>={from}");
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
            search_entry["fetched_at"] = json!(clock.now().rfc3339_seconds());
            search_entry["complete"] = json!(
                items.len() < 30
                    && val["incomplete_results"] != true
                    && val["total_count"].as_u64() == Some(items.len() as u64)
            );
            search_entry["incomplete_results"] = val
                .get("incomplete_results")
                .cloned()
                .unwrap_or(Value::Null);
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
                        "description":item["description"],
                        "html_url":item["html_url"],
                        "created_at":item["created_at"],
                    }));
                }
            }
        }
        Err(e) => {
            let m = format!("{e:#}");
            api_blocked = m.contains("HTTP 403") || m.contains("HTTP 429");
            search_entry["fetched_at"] = json!(clock.now().rfc3339_seconds());
            search_entry["reason"] = json!(format!("fetch failed: {m}"));
        }
    }
    sources.push(search_entry);
    let all: Vec<Candidate> = candidates.into_values().collect();
    // Preserve every observed identity before optional metadata requests can fail.
    let mut snapshots = all
        .iter()
        .map(|candidate| observed(candidate, now))
        .collect::<Result<Vec<_>>>()?;
    write_json(&dir.join("candidate-observations.json"), &snapshots)?;
    let mut api_records = Vec::new();
    for (candidate, observation) in all.iter().zip(&mut snapshots) {
        let url = format!("{API}/repos/{}", candidate.name);
        let mut info = source_entry(
            "GitHub REST repository metadata",
            &url,
            "single repository response",
        );
        if api_blocked {
            info["fetched_at"] = json!(clock.now().rfc3339_seconds());
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
                let v: Value = match serde_json::from_slice(&bytes) {
                    Ok(value) => value,
                    Err(_) => {
                        info["reason"] =
                            json!("invalid metadata JSON; source observation retained");
                        api_records.push(info);
                        continue;
                    }
                };
                let snap = Snapshot {
                    full_name: candidate.name.clone(),
                    captured_at: now.rfc3339_seconds(),
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
                    observation: None,
                };
                info["fetched_at"] = json!(clock.now().rfc3339_seconds());
                info["complete"] = json!(true);
                info["status"] = json!("included");
                info["reason"] = json!("metadata captured");
                info["stars"] = json!(snap.stars);
                info["forks"] = json!(snap.forks);
                *observation = snap;
            }
            Err(e) => {
                let m = format!("{e:#}");
                if m.contains("HTTP 403") || m.contains("HTTP 429") {
                    api_blocked = true;
                }
                info["fetched_at"] = json!(clock.now().rfc3339_seconds());
                info["reason"] = json!(format!("fetch failed: {m}"));
            }
        }
        api_records.push(info);
        clock.sleep(StdDuration::from_millis(100));
    }
    let manifest = json!({"captured_at":now.rfc3339_seconds(),"sources":sources,"candidate_policy":"All identities returned by configured Trending pages and bounded Search page are retained, including covered repositories and metadata failures; weekly selection happens later; not a global GitHub census.","candidate_count":snapshots.len(),"excluded_candidates":[],"api_repositories":api_records,"validation_notes":[]});
    write_json(&dir.join("candidate-observations.json"), &snapshots)?;
    write_json(&dir.join("manifest.json"), &manifest)?;
    Ok(RunData {
        dir,
        manifest,
        snapshots,
    })
}

fn observed(candidate: &Candidate, now: Timestamp) -> Result<Snapshot> {
    let field = |name: &str| {
        candidate
            .sources
            .iter()
            .find_map(|source| source.get(name).filter(|value| !value.is_null()).cloned())
    };
    Ok(serde_json::from_value(json!({
        "full_name":candidate.name,
        "captured_at":now.rfc3339_seconds(),
        "stars":field("stars"),"description":field("description"),
        "html_url":field("html_url").unwrap_or_else(|| json!(format!("https://github.com/{}", candidate.name))),
        "created_at":field("created_at"),"language":field("language"),
        "sources":candidate.sources
    }))?)
}

/// Fetch evidence only for the final weekly selection, never for each observation.
pub(super) fn enrich_readmes(
    snapshots: &mut [Snapshot],
    dir: &Path,
    http: &dyn SourceHttp,
    clock: &dyn Clock,
) -> Result<Vec<Value>> {
    let mut sources = Vec::new();
    let readme_names = snapshots
        .iter()
        .map(|snapshot| snapshot.full_name.clone())
        .collect::<Vec<_>>();
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
                    info["fetched_at"] = json!(clock.now().rfc3339_seconds());
                    info["complete"] = json!(true);
                    info["status"] = json!("included");
                    info["reason"] = json!("raw README captured; bounded excerpt used for model");
                    info["chars"] = json!(excerpt.chars().count());
                    snap.readme_excerpt = Some(excerpt);
                }
                Err(e) => {
                    info["fetched_at"] = json!(clock.now().rfc3339_seconds());
                    if let Some(error) = e.downcast_ref::<HttpStatusError>() {
                        info["http_status"] = json!(error.status.as_u16());
                    }
                    info["reason"] = json!(format!("fetch failed: {e:#}"));
                }
            }
            sources.push(info);
        }
    }
    Ok(sources)
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
