//! Factual coverage of retained observations; never reconstruct missed captures.
use crate::timestamp::Timestamp;
use crate::{Snapshot, parse_time};
use anyhow::{Context, Result};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use time::{SignedDuration as Duration, macros::format_description};

const TARGET_INTERVAL_SECONDS: i64 = 3 * 60 * 60;

fn timestamp(time: Timestamp) -> String {
    time.rfc3339_seconds()
}

/// Count nominal trigger instants in (start, end], not successful scheduled runs.
fn nominal_slots(start: Timestamp, end: Timestamp) -> usize {
    let mut slot = Timestamp::from_utc(start.date().with_hms(0, 17, 0).unwrap());
    let mut count = 0;
    while slot <= end {
        if slot > start {
            count += 1;
        }
        slot += Duration::hours(3);
    }
    count
}

/// Pass the retained state snapshots, not the aggregated weekly candidate pool.
/// `runs_dir` is read-only. Call after the current capture manifest is written.
pub(super) fn report(snapshots: &[Snapshot], now: Timestamp, runs_dir: &Path) -> Result<Value> {
    let start = now - Duration::days(7);
    let mut captures = BTreeSet::new();
    let mut repositories = BTreeSet::new();
    let mut observation_keys = BTreeSet::new();
    let mut raw_rows = 0;
    let mut invalid_timestamps = 0;
    let mut future_rows = 0;
    for snapshot in snapshots {
        let Some(captured) = parse_time(&snapshot.captured_at) else {
            invalid_timestamps += 1;
            continue;
        };
        if captured > now {
            future_rows += 1;
            continue;
        }
        if captured < start {
            continue;
        }
        raw_rows += 1;
        captures.insert(captured);
        repositories.insert(snapshot.full_name.as_str());
        observation_keys.insert((captured, snapshot.full_name.as_str()));
    }
    let times: Vec<_> = captures.iter().copied().collect();
    let mut intervals: Vec<_> = times
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).whole_seconds())
        .collect();
    let long_intervals: Vec<_> = times
        .windows(2)
        .filter_map(|pair| {
            let seconds = (pair[1] - pair[0]).whole_seconds();
            (seconds > TARGET_INTERVAL_SECONDS).then(
                || json!({"from":timestamp(pair[0]),"to":timestamp(pair[1]),"seconds":seconds}),
            )
        })
        .collect();
    intervals.sort_unstable();
    let median = if intervals.is_empty() {
        None
    } else {
        let middle = intervals.len() / 2;
        Some(if intervals.len().is_multiple_of(2) {
            (intervals[middle - 1] as f64 + intervals[middle] as f64) / 2.0
        } else {
            intervals[middle] as f64
        })
    };

    let archive_available = runs_dir
        .try_exists()
        .context("cannot inspect observation archive")?;
    let mut manifests = BTreeMap::new();
    let mut filtered_captures = Vec::new();
    let mut invalid_manifests = Vec::new();
    let mut directories_without_manifest = Vec::new();
    let mut metadata_failures = 0;
    let mut source_failures = 0;
    if archive_available {
        let mut entries = fs::read_dir(runs_dir)
            .context("cannot read observation archive")?
            .collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            // Stored-preview and retry copies are not additional observations.
            let Some(captured) = Timestamp::parse_stamp(name) else {
                continue;
            };
            if captured < start || captured > now {
                continue;
            }
            let at = timestamp(captured);
            let bytes = match fs::read(entry.path().join("manifest.json")) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    directories_without_manifest.push(at);
                    continue;
                }
                Err(error) => return Err(error).context("cannot read capture manifest"),
            };
            let Ok(manifest) = serde_json::from_slice::<Value>(&bytes) else {
                invalid_manifests.push(json!({"captured_at":at,"reason":"invalid JSON"}));
                continue;
            };
            if manifest["captured_at"].as_str().and_then(parse_time) != Some(captured) {
                invalid_manifests.push(
                    json!({"captured_at":at,"reason":"missing or mismatched capture timestamp"}),
                );
                continue;
            }
            let excluded = manifest["excluded_candidates"]
                .as_array()
                .map_or(0, Vec::len);
            let policy_has_cap = manifest["candidate_policy"]
                .as_str()
                .is_some_and(|policy| policy.to_lowercase().contains("max 30"));
            if excluded > 0 || policy_has_cap {
                filtered_captures.push(json!({
                    "captured_at":at,
                    "candidate_count":manifest["candidate_count"].as_u64(),
                    "excluded_count":excluded,
                    "legacy_collection_cap":policy_has_cap
                }));
            }
            if let Some(records) = manifest["api_repositories"].as_array() {
                metadata_failures += records
                    .iter()
                    .filter(|record| record["status"] != "included")
                    .count();
            }
            if let Some(records) = manifest["sources"].as_array() {
                source_failures += records
                    .iter()
                    .filter(|record| record["status"] != "included")
                    .count();
            }
            manifests.insert(
                captured,
                json!({"captured_at":at,"candidate_count":manifest["candidate_count"].as_u64()}),
            );
        }
    }
    let captures_without_manifest: Vec<_> = captures
        .iter()
        .filter(|at| !manifests.contains_key(*at))
        .map(|at| timestamp(*at))
        .collect();
    let manifests_without_snapshots: Vec<_> = manifests
        .iter()
        .filter(|(at, _)| !captures.contains(*at))
        .map(|(_, manifest)| manifest.clone())
        .collect();

    Ok(json!({
        "format_version":1,
        "window":{"start":timestamp(start),"end":timestamp(now),"kind":"rolling_seven_days"},
        "observed":{
            "snapshot_rows":observation_keys.len(),
            "duplicate_snapshot_rows":raw_rows-observation_keys.len(),
            "unique_repositories":repositories.len(),
            "capture_count":captures.len(),
            "first_capture_at":times.first().copied().map(timestamp),
            "last_capture_at":times.last().copied().map(timestamp),
            "invalid_timestamp_rows":invalid_timestamps,
            "ignored_future_rows":future_rows
        },
        "cadence":{
            "nominal_cron":"17 */3 * * *",
            "target_interval_seconds":TARGET_INTERVAL_SECONDS,
            "nominal_slots_in_window":nominal_slots(start,now),
            "nominal_slot_boundary":"start_exclusive_end_inclusive",
            "scheduled_capture_count":Value::Null,
            "trigger_note":"Historical snapshots do not identify manual versus scheduled runs; capture counts are not schedule completion counts.",
            "interval_count":intervals.len(),
            "min_interval_seconds":intervals.first(),
            "median_interval_seconds":median,
            "max_interval_seconds":intervals.last(),
            "intervals_over_three_hours":long_intervals,
            "leading_gap_seconds":times.first().map(|first|(*first-start).whole_seconds()),
            "trailing_gap_seconds":times.last().map(|last|(now-*last).whole_seconds()),
            "empty_window":captures.is_empty()
        },
        "archive":{
            "available":archive_available,
            "manifest_capture_count":manifests.len(),
            "captures_without_manifest":captures_without_manifest,
            "manifests_without_snapshots":manifests_without_snapshots,
            "directories_without_manifest":directories_without_manifest,
            "filtered_captures":filtered_captures,
            "invalid_manifests":invalid_manifests,
            "metadata_failure_records":metadata_failures,
            "source_failure_records":source_failures
        },
        "scope":{
            "trending":"first returned weekly and daily pages",
            "search_first_page_limit":30,
            "global_coverage_known":false,
            "missing_intervals_reconstructed":false,
            "note":"No observations are invented, interpolated or restored from aggregated weekly candidates. Leading and trailing gaps describe the observed window, not proven failed triggers."
        }
    }))
}

fn short_time(value: &Value) -> String {
    value
        .as_str()
        .and_then(parse_time)
        .map(|time| time.format(format_description!("[day].[month] [hour]:[minute]")))
        .unwrap_or_else(|| "неизвестно".into())
}

fn elapsed(seconds: f64) -> String {
    let minutes = (seconds / 60.0).round() as u64;
    format!("{} ч {} мин", minutes / 60, minutes % 60)
}

/// Plain text; the caller must escape it for its output format.
pub(super) fn render_notice(report: &Value) -> String {
    let observed = &report["observed"];
    let captures = observed["capture_count"].as_u64().unwrap_or(0);
    let mut text = if captures == 0 {
        "За выбранные семь дней нет сохранённых наблюдений.".to_string()
    } else {
        format!(
            "Данные {} — {} UTC: {} срезов, {} репозиториев.",
            short_time(&observed["first_capture_at"]),
            short_time(&observed["last_capture_at"]),
            captures,
            observed["unique_repositories"].as_u64().unwrap_or(0)
        )
    };
    let cadence = &report["cadence"];
    if let (Some(median), Some(maximum)) = (
        cadence["median_interval_seconds"].as_f64(),
        cadence["max_interval_seconds"].as_f64(),
    ) {
        text.push_str(&format!(
            " Интервалы: медиана {}, максимум {}.",
            elapsed(median),
            elapsed(maximum)
        ));
    }
    let count = |key: &str| report["archive"][key].as_array().map_or(0, Vec::len);
    let missing = count("captures_without_manifest");
    let orphaned = count("manifests_without_snapshots");
    let filtered = count("filtered_captures");
    let invalid = count("invalid_manifests");
    if missing + orphaned + filtered + invalid > 0 {
        text.push_str(&format!(
            " Пробелы архива: {missing} срезов без manifest, {orphaned} manifest без наблюдений, {filtered} срезов с ранними исключениями, {invalid} повреждённых manifest."
        ));
    }
    text.push_str(" Первые страницы Trending и 30 результатов Search; пропущенные наблюдения не восстановлены, полнота GitHub не гарантируется.");
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(value: &str) -> Timestamp {
        parse_time(value).unwrap()
    }

    fn snapshot(name: &str, captured: &str) -> Snapshot {
        serde_json::from_value(json!({"full_name":name,"captured_at":captured})).unwrap()
    }

    fn manifest(root: &Path, captured: &str, value: Value) {
        let dir = root.join(crate::stamp(time(captured)));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("manifest.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn incomplete_week_reports_real_gaps_without_claiming_schedule_completion() {
        let temp = tempfile::tempdir().unwrap();
        let rows = vec![
            snapshot("a/one", "2026-10-20T00:17:00Z"),
            snapshot("a/one", "2026-10-20T00:17:00Z"),
            snapshot("b/two", "2026-10-20T00:17:00Z"),
            snapshot("a/one", "2026-10-20T03:17:00Z"),
            snapshot("c/three", "2026-10-20T12:17:00Z"),
            snapshot("future/ignored", "2026-10-27T00:00:00Z"),
            snapshot("old/ignored", "2026-10-18T00:00:00Z"),
            snapshot("invalid/ignored", "invalid"),
        ];
        let value = report(&rows, time("2026-10-26T07:00:00Z"), temp.path()).unwrap();
        assert_eq!(value["observed"]["snapshot_rows"], 4);
        assert_eq!(value["observed"]["duplicate_snapshot_rows"], 1);
        assert_eq!(value["observed"]["capture_count"], 3);
        assert_eq!(value["observed"]["unique_repositories"], 3);
        assert_eq!(value["observed"]["ignored_future_rows"], 1);
        assert_eq!(value["observed"]["invalid_timestamp_rows"], 1);
        assert_eq!(value["cadence"]["nominal_slots_in_window"], 56);
        assert!(value["cadence"]["scheduled_capture_count"].is_null());
        assert_eq!(value["cadence"]["median_interval_seconds"], 21600.0);
        assert_eq!(value["cadence"]["max_interval_seconds"], 32400);
        assert_eq!(
            value["cadence"]["intervals_over_three_hours"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(value["cadence"]["leading_gap_seconds"], 62220);
        assert_eq!(value["cadence"]["trailing_gap_seconds"], 499380);
        assert_eq!(
            value["archive"]["captures_without_manifest"]
                .as_array()
                .unwrap()
                .len(),
            3
        );
        assert!(render_notice(&value).contains("3 срезов, 3 репозиториев"));
    }

    #[test]
    fn manifest_reconciliation_exposes_lost_capped_capture_and_ignores_refresh_copies() {
        let temp = tempfile::tempdir().unwrap();
        let retained = "2026-10-20T00:17:00Z";
        let lost = "2026-10-20T03:17:00Z";
        manifest(
            temp.path(),
            lost,
            json!({
                "captured_at":lost,"candidate_count":30,
                "candidate_policy":"stable order; max 30",
                "excluded_candidates":[{"full_name":"not/copied","reason":"cap"}]
            }),
        );
        manifest(
            temp.path(),
            "2026-10-20T06:17:00Z",
            json!({"captured_at":lost}),
        );
        let copied = temp.path().join("refreshed-123");
        fs::create_dir(&copied).unwrap();
        fs::write(
            copied.join("manifest.json"),
            json!({"captured_at":retained}).to_string(),
        )
        .unwrap();
        let rows = [snapshot("a/one", retained)];
        let value = report(&rows, time("2026-10-26T07:00:00Z"), temp.path()).unwrap();
        assert_eq!(value["observed"]["capture_count"], 1);
        assert_eq!(value["archive"]["manifest_capture_count"], 1);
        assert_eq!(
            value["archive"]["captures_without_manifest"],
            json!([retained])
        );
        assert_eq!(
            value["archive"]["manifests_without_snapshots"][0]["captured_at"],
            lost
        );
        assert_eq!(
            value["archive"]["filtered_captures"][0]["excluded_count"],
            1
        );
        assert_eq!(
            value["archive"]["filtered_captures"][0]["legacy_collection_cap"],
            true
        );
        assert_eq!(
            value["archive"]["invalid_manifests"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert!(!value.to_string().contains("not/copied"));
        assert!(!value.to_string().contains("refreshed-123"));
        assert!(render_notice(&value).contains("1 manifest без наблюдений"));
    }

    #[test]
    fn empty_window_and_exact_cron_boundaries_do_not_invent_observations() {
        let temp = tempfile::tempdir().unwrap();
        let value = report(
            &[],
            time("2026-10-26T06:17:00Z"),
            &temp.path().join("absent"),
        )
        .unwrap();
        assert_eq!(value["cadence"]["nominal_slots_in_window"], 56);
        assert_eq!(value["cadence"]["empty_window"], true);
        assert!(value["cadence"]["leading_gap_seconds"].is_null());
        assert!(value["cadence"]["trailing_gap_seconds"].is_null());
        assert!(value["cadence"]["median_interval_seconds"].is_null());
        assert_eq!(value["archive"]["available"], false);
        assert!(
            render_notice(&value).starts_with("За выбранные семь дней нет сохранённых наблюдений.")
        );
    }
}
