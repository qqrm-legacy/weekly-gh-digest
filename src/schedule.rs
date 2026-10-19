//! Weekly publication identity and retained collection window.
use crate::{Snapshot, State, parse_time};
use chrono::{DateTime, Datelike, Duration, Utc};
use std::collections::BTreeMap;

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

/// Keep the latest captured observation per repository from the past seven days.
/// Metric timestamps remain attached; daily/weekly counts are never summed.
pub(super) fn candidates(state: &State, now: DateTime<Utc>) -> Vec<Snapshot> {
    let cutoff = now - Duration::days(7);
    let mut latest = BTreeMap::<String, (DateTime<Utc>, Snapshot)>::new();
    for snapshot in &state.snapshots {
        let Some(captured) = parse_time(&snapshot.captured_at) else {
            continue;
        };
        if captured < cutoff || captured > now {
            continue;
        }
        let entry = latest.entry(snapshot.full_name.clone());
        entry
            .and_modify(|(previous, value)| {
                if captured > *previous {
                    *previous = captured;
                    *value = snapshot.clone();
                }
            })
            .or_insert_with(|| (captured, snapshot.clone()));
    }
    latest.into_values().map(|(_, value)| value).collect()
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

    #[test]
    fn publication_is_closed_unless_explicitly_enabled() {
        for setting in [None, Some(""), Some("false"), Some("yes"), Some("TRUE")] {
            assert!(!delivery_enabled(setting));
        }
        assert!(delivery_enabled(Some("true")));
    }
}
