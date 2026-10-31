//! Weekly publication identity and retained collection window.
use crate::{Snapshot, State, parse_time};
use chrono::{DateTime, Datelike, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct ObservationWindow {
    pub(super) first_seen: String,
    pub(super) last_seen: String,
    pub(super) observations: usize,
}

/// Multiple captures on the same day are distinct; exact retries are idempotent.
pub(super) fn record(state: &mut State, new: &[Snapshot]) {
    let mut rows = BTreeMap::new();
    for snapshot in state.snapshots.drain(..).chain(new.iter().cloned()) {
        rows.insert(
            (snapshot.captured_at.clone(), snapshot.full_name.clone()),
            snapshot,
        );
    }
    state.snapshots = rows.into_values().collect();
}

/// Moscow is UTC+3. A new issue period opens on Monday at 10:00 Moscow.
pub(super) fn slot(moment: DateTime<Utc>) -> String {
    let local = moment + Duration::hours(3);
    let mut monday =
        local.date_naive() - Duration::days(local.weekday().num_days_from_monday().into());
    if local.naive_utc() < monday.and_hms_opt(10, 0, 0).unwrap() {
        monday -= Duration::days(7);
    }
    format!("weekly:{monday}")
}

/// Union every observed repository, retaining peak metrics separately from latest metadata.
/// Daily/weekly rolling counts are never summed, and disappearing rows stay eligible.
pub(super) fn candidates(state: &State, now: DateTime<Utc>) -> Vec<Snapshot> {
    candidates_since(state, now, Some(now - Duration::days(7)))
}

/// Pending findings do not expire with the reporting window.
pub(super) fn all_candidates(state: &State, now: DateTime<Utc>) -> Vec<Snapshot> {
    candidates_since(state, now, None)
}

fn candidates_since(
    state: &State,
    now: DateTime<Utc>,
    cutoff: Option<DateTime<Utc>>,
) -> Vec<Snapshot> {
    let mut groups = BTreeMap::<String, BTreeMap<DateTime<Utc>, &Snapshot>>::new();
    for snapshot in &state.snapshots {
        let Some(captured) = parse_time(&snapshot.captured_at) else {
            continue;
        };
        if cutoff.is_some_and(|cutoff| captured < cutoff) || captured > now {
            continue;
        }
        groups
            .entry(snapshot.full_name.to_ascii_lowercase())
            .or_default()
            .insert(captured, snapshot);
    }
    groups
        .into_values()
        .map(|observations| {
            let (first, _) = observations.first_key_value().unwrap();
            let (last, latest) = observations.last_key_value().unwrap();
            let mut snapshot = (**latest).clone();
            let mut peaks = BTreeMap::<String, (i64, Value)>::new();
            for (captured, row) in &observations {
                for source in &row.sources {
                    let Some(kind) = source["type"].as_str() else {
                        continue;
                    };
                    let metric = source["trending_stars_period"]
                        .as_i64()
                        .or_else(|| source["stars"].as_i64())
                        .unwrap_or(0);
                    let entry = peaks
                        .entry(kind.to_string())
                        .or_insert_with(|| (i64::MIN, Value::Null));
                    if metric >= entry.0 {
                        let mut source = source.clone();
                        source["captured_at"] = Value::String(
                            captured.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                        );
                        *entry = (metric, source);
                    }
                }
            }
            snapshot.sources = peaks.into_values().map(|(_, source)| source).collect();
            snapshot.observation = Some(ObservationWindow {
                first_seen: first.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                last_seen: last.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
                observations: observations.len(),
            });
            snapshot
        })
        .collect()
}

pub(super) fn delivery_enabled(value: Option<&str>) -> bool {
    value == Some("true")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn time(value: &str) -> DateTime<Utc> {
        parse_time(value).unwrap()
    }

    #[test]
    fn week_changes_only_at_monday_ten_moscow() {
        for instant in [
            "2026-10-19T07:00:00Z",
            "2026-10-19T19:00:00Z",
            "2026-10-25T23:59:59Z",
            "2026-10-26T06:59:59Z",
        ] {
            assert_eq!(slot(time(instant)), "weekly:2026-10-19");
        }
        assert_eq!(slot(time("2026-10-19T06:59:59Z")), "weekly:2026-10-12");
        assert_eq!(slot(time("2026-10-26T07:00:00Z")), "weekly:2026-10-26");
        assert_eq!(slot(time("2027-01-04T07:00:00Z")), "weekly:2027-01-04");
    }

    #[test]
    fn retained_candidates_use_a_bounded_window_and_latest_real_observation() {
        let mut state = State::default();
        for (name, captured, stars) in [
            ("org/young", "2026-10-13T07:00:00Z", 0),
            ("org/young", "2026-10-18T07:00:00Z", 12),
            ("org/boundary", "2026-10-12T07:00:00Z", 3),
            ("org/stale", "2026-10-12T06:59:59Z", 9000),
            ("org/future", "2026-10-19T07:00:01Z", 9000),
            ("org/invalid", "invalid", 9000),
        ] {
            state.snapshots.push(
                serde_json::from_value(json!({
                    "full_name": name, "captured_at": captured, "stars": stars
                }))
                .unwrap(),
            );
        }
        let rows = candidates(&state, time("2026-10-19T07:00:00Z"));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].full_name, "org/young");
        assert_eq!(rows[1].stars, Some(12));
        assert_eq!(rows[1].captured_at, "2026-10-18T07:00:00Z");
    }

    fn observed(name: &str, at: &str, kind: &str, metric: i64) -> Snapshot {
        serde_json::from_value(json!({"full_name":name,"captured_at":at,"stars":metric,
            "sources":[{"type":kind,"trending_stars_period":metric}]}))
        .unwrap()
    }

    #[test]
    fn same_day_entries_exits_and_peak_metrics_survive_later_captures() {
        let mut state = State::default();
        let first = vec![
            observed(
                "org/transient",
                "2026-10-13T00:17:00Z",
                "trending_weekly",
                900,
            ),
            observed(
                "org/persistent",
                "2026-10-13T00:17:00Z",
                "trending_weekly",
                500,
            ),
        ];
        record(&mut state, &first);
        record(&mut state, &first); // exact retry must not inflate appearances
        record(
            &mut state,
            &[observed(
                "org/persistent",
                "2026-10-13T03:17:00Z",
                "trending_weekly",
                100,
            )],
        );
        record(
            &mut state,
            &[observed(
                "org/persistent",
                "2026-10-18T06:17:00Z",
                "trending_daily",
                20,
            )],
        );
        assert_eq!(state.snapshots.len(), 4);
        let weekly = candidates(&state, time("2026-10-19T07:00:00Z"));
        assert_eq!(weekly.len(), 2);
        let persistent = weekly
            .iter()
            .find(|row| row.full_name == "org/persistent")
            .unwrap();
        assert_eq!(persistent.stars, Some(20)); // latest metadata
        assert_eq!(
            crate::source_priority(persistent),
            (0, 500, "trending_weekly")
        ); // not 620, not 20
        let peak = persistent
            .sources
            .iter()
            .find(|source| source["type"] == "trending_weekly")
            .unwrap();
        assert_eq!(peak["captured_at"], "2026-10-13T00:17:00Z");
        let appearances = persistent.observation.as_ref().unwrap();
        assert_eq!(appearances.first_seen, "2026-10-13T00:17:00Z");
        assert_eq!(appearances.last_seen, "2026-10-18T06:17:00Z");
        assert_eq!(appearances.observations, 3);
        assert!(weekly.iter().any(|row| row.full_name == "org/transient"));
    }

    #[test]
    fn weekly_cap_and_covered_filter_do_not_discard_observations() {
        let mut state = State::default();
        for index in 0..45 {
            record(
                &mut state,
                &[observed(
                    &format!("org/p{index:02}"),
                    "2026-10-13T00:17:00Z",
                    "trending_weekly",
                    index,
                )],
            );
        }
        state.covered.insert("org/p44".into(), "2026-10-01".into());
        let weekly = candidates(&state, time("2026-10-19T07:00:00Z"));
        assert_eq!(weekly.len(), 45);
        let (selected, excluded) = crate::select_candidates(&weekly, &state.covered);
        assert_eq!(selected.len(), 30);
        assert_eq!(excluded.len(), 15);
        assert_eq!(selected[0].snapshot.full_name, "org/p43");
        assert_eq!(state.snapshots.len(), 45);
    }

    #[test]
    fn publication_is_closed_unless_explicitly_enabled() {
        for setting in [None, Some(""), Some("false"), Some("yes"), Some("TRUE")] {
            assert!(!delivery_enabled(setting));
        }
        assert!(delivery_enabled(Some("true")));
    }
}
