//! Golden contracts for state and publication identities written before the time migration.

use crate::clock::Clock;
use crate::editions::{self, Format};
use crate::test_support::FakeClock;
use crate::timestamp::Timestamp;
use crate::{Snapshot, State, coverage, parse_time, schedule, stamp};
use serde_json::{Value, json};
use std::time::Duration as StdDuration;
use time::SignedDuration as Duration;

fn at(value: &str) -> Timestamp {
    parse_time(value).unwrap()
}

fn observed(name: &str, captured: &str) -> Snapshot {
    serde_json::from_value(json!({
        "full_name":name,"captured_at":captured,"stars":1,
        "sources":[{"type":"trending_daily","trending_stars_period":1}]
    }))
    .unwrap()
}

#[test]
fn legacy_retry_checkpoint_preserves_fractional_precision_and_receipts() {
    // Literal persisted values are the oracle, independent of the replacement formatter.
    let legacy = json!({
        "schema":1,
        "editions":{"version":1,"daily_retry_at":{
            "org/seconds":"2026-11-21T15:00:00Z",
            "org/millis":"2026-11-21T15:00:00.120Z",
            "org/micros":"2026-11-21T15:00:00.123400Z",
            "org/nanos":"2026-11-21T15:00:00.123456700Z"
        }},
        "sent_slots":{"daily:2026-11-20":301,"weekly:2026-11-16":302},
        "sent_digests":{"unchanged-delivery-id":301},
        "delivery_progress":{"unchanged-delivery-id":{
            "project_message_ids":{"chunk-00":301},
            "overview_message_id":null,
            "message_texts":{"chunk-00":"Frozen text"}
        }}
    });
    let state: State = serde_json::from_value(legacy.clone()).unwrap();
    for (name, timestamp) in legacy["editions"]["daily_retry_at"].as_object().unwrap() {
        assert_eq!(
            editions::daily_retry_at(&state, &name.to_ascii_uppercase()),
            Some(at(timestamp.as_str().unwrap()))
        );
    }
    let saved = serde_json::to_value(&state).unwrap();
    assert_eq!(
        saved["editions"]["daily_retry_at"],
        legacy["editions"]["daily_retry_at"]
    );
    for field in ["sent_slots", "sent_digests", "delivery_progress"] {
        assert_eq!(saved[field], legacy[field], "{field}");
    }
    let empty: State = serde_json::from_str("{}").unwrap();
    assert_eq!(editions::daily_retry_at(&empty, "org/missing"), None);
}

#[test]
fn retry_checkpoint_normalizes_offsets_but_rejects_corruption() {
    let state: State = serde_json::from_value(json!({"editions":{"daily_retry_at":{
        "org/offset":"2026-11-21T18:00:00.120+03:00"
    }}}))
    .unwrap();
    assert_eq!(
        serde_json::to_value(state).unwrap()["editions"]["daily_retry_at"]["org/offset"],
        "2026-11-21T15:00:00.120Z"
    );
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("checkpoint.json");
    for invalid in [
        json!("not a timestamp"),
        json!("2026-02-30T15:00:00Z"),
        json!(42),
        Value::Null,
    ] {
        let bytes = serde_json::to_vec(&json!({"editions":{"daily_retry_at":{
            "org/retry":invalid
        }},"sent_slots":{"daily:2026-11-20":301}}))
        .unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert!(
            crate::state_store::StateStore::new(&path)
                .load::<State>()
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }
}

#[test]
fn deferred_retry_survives_reload_and_becomes_due_at_the_exact_instant() {
    let mut state = State::default();
    state
        .snapshots
        .push(observed("org/retry", "2024-02-27T00:00:00Z"));
    let deferred = editions::defer_daily(
        &mut state,
        "ORG/RETRY",
        at("2024-02-28T23:59:59.123456789Z"),
    );
    assert_eq!(deferred, at("2024-02-29T23:59:59.123456789Z"));
    let bytes = serde_json::to_vec(&state).unwrap();
    let mut restored: State = serde_json::from_slice(&bytes).unwrap();
    let saved: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        saved["editions"]["daily_retry_at"]["org/retry"],
        "2024-02-29T23:59:59.123456789Z"
    );
    for (instant, eligible) in [
        ("2024-02-29T23:59:59.123456788Z", false),
        ("2024-02-29T23:59:59.123456789Z", true),
    ] {
        let now = at(instant);
        let pending = editions::pending(&restored, now, Format::Daily);
        let (selected, excluded) = editions::select(&pending, &restored, Format::Daily, now);
        assert_eq!(selected.len(), usize::from(eligible), "{instant}");
        assert_eq!(excluded.len(), usize::from(!eligible), "{instant}");
        assert_eq!(
            editions::daily_retry_at(&restored, "org/retry"),
            Some(deferred)
        );
    }
    editions::acknowledge(
        &mut restored,
        &json!({"edition":"daily","entries":[{"full_name":"ORG/RETRY"}]}),
        "2024-02-29",
    )
    .unwrap();
    assert_eq!(editions::daily_retry_at(&restored, "org/retry"), None);
}

#[test]
fn frozen_pending_issue_keeps_its_original_slot_and_payload_after_reload() {
    let legacy = json!({"pending_digests":{
        "daily-frozen":{
            "edition":"daily","generated_at":"2027-01-01T14:59:59.999999999+00:00",
            "delivery_slot":"daily:2026-12-31","entries":[{"full_name":"org/daily","description":"Frozen daily card"}]
        },
        "weekly-frozen":{
            "edition":"weekly","generated_at":"2027-01-04T09:59:59.999999999+03:00",
            "delivery_slot":"weekly:2026-12-28","entries":[{"full_name":"org/weekly","description":"Frozen weekly card"}]
        }
    }});
    let state: State = serde_json::from_value(legacy.clone()).unwrap();
    for (format, id, slot) in [
        (Format::Daily, "daily-frozen", "daily:2026-12-31"),
        (Format::Weekly, "weekly-frozen", "weekly:2026-12-28"),
    ] {
        let pending = editions::pending_issues(&state, at("2027-03-01T07:00:00Z"), format).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].0, id);
        assert_eq!(pending[0].1, legacy["pending_digests"][id]);
        assert_eq!(pending[0].2, slot);
    }
    assert_eq!(
        serde_json::to_value(&state).unwrap()["pending_digests"],
        legacy["pending_digests"]
    );
    let mut inconsistent = legacy;
    inconsistent["pending_digests"]["weekly-frozen"]["delivery_slot"] = json!("weekly:2027-01-04");
    let inconsistent: State = serde_json::from_value(inconsistent).unwrap();
    assert!(
        editions::pending_issues(&inconsistent, at("2027-03-01T07:00:00Z"), Format::Weekly)
            .is_err()
    );
}

#[test]
fn daily_and_weekly_identity_boundaries_preserve_nanoseconds_and_calendar_rollovers() {
    for (instant, expected) in [
        ("2027-01-01T14:59:59.999999999Z", "daily:2026-12-31"),
        ("2027-01-01T15:00:00Z", "daily:2027-01-01"),
        ("2024-03-01T14:59:59.999999999Z", "daily:2024-02-29"),
        ("2024-03-01T18:00:00+03:00", "daily:2024-03-01"),
        ("2025-03-01T14:59:59.999999999Z", "daily:2025-02-28"),
    ] {
        assert_eq!(Format::Daily.slot(at(instant)), expected, "{instant}");
    }
    for (instant, expected) in [
        ("2027-01-04T06:59:59.999999999Z", "weekly:2026-12-28"),
        ("2027-01-04T10:00:00+03:00", "weekly:2027-01-04"),
        ("2024-03-04T06:59:59.999999999Z", "weekly:2024-02-26"),
        ("2024-03-04T07:00:00Z", "weekly:2024-03-04"),
    ] {
        assert_eq!(Format::Weekly.slot(at(instant)), expected, "{instant}");
    }
}

#[test]
fn weekly_window_is_inclusive_and_never_rounds_future_or_expired_rows_into_it() {
    let state = State {
        snapshots: [
            ("org/boundary", "2024-02-29T00:00:00.123456789Z"),
            ("org/expired", "2024-02-29T00:00:00.123456788Z"),
            ("org/now", "2024-03-07T00:00:00.123456789Z"),
            ("org/future", "2024-03-07T00:00:00.123456790Z"),
        ]
        .map(|(name, captured)| observed(name, captured))
        .into(),
        ..State::default()
    };
    let now = at("2024-03-07T00:00:00.123456789Z");
    let selected = schedule::candidates(&state, now);
    assert_eq!(
        selected
            .iter()
            .map(|row| row.full_name.as_str())
            .collect::<Vec<_>>(),
        ["org/boundary", "org/now"]
    );
    let directory = tempfile::tempdir().unwrap();
    let report = coverage::report(&state.snapshots, now, directory.path()).unwrap();
    assert_eq!(report["observed"]["snapshot_rows"], 2);
    assert_eq!(report["observed"]["ignored_future_rows"], 1);
    assert_eq!(report["cadence"]["max_interval_seconds"], 604800);
}

#[test]
fn daily_freshness_includes_the_exact_day_boundary_before_backlog() {
    let current = [
        observed("org/backlog", "2024-02-29T15:00:00.123456788Z"),
        observed("org/boundary", "2024-02-29T15:00:00.123456789Z"),
        observed("org/new", "2024-03-01T15:00:00.123456789Z"),
    ];
    let (selected, excluded) = editions::select(
        &current,
        &State::default(),
        Format::Daily,
        at("2024-03-01T15:00:00.123456789Z"),
    );
    assert_eq!(
        selected
            .iter()
            .map(|entry| entry.snapshot.full_name.as_str())
            .collect::<Vec<_>>(),
        ["org/boundary", "org/new", "org/backlog"]
    );
    assert!(excluded.is_empty());
}

#[test]
fn replayed_capture_retains_legacy_dedup_key_and_whole_second_observation_strings() {
    let mut state: State = serde_json::from_value(json!({"snapshots":[{
        "full_name":"org/capture","captured_at":"2026-11-21T00:17:00Z","stars":1,
        "sources":[{"type":"trending_daily","trending_stars_period":1}]
    }]}))
    .unwrap();
    let captured = at("2026-11-21T03:17:00.987654321+03:00").rfc3339_seconds();
    assert_eq!(captured, "2026-11-21T00:17:00Z");
    let mut replay = observed("org/capture", &captured);
    replay.stars = Some(2);
    schedule::record(&mut state, &[replay]);
    assert_eq!(state.snapshots.len(), 1);
    assert_eq!(state.snapshots[0].captured_at, "2026-11-21T00:17:00Z");
    assert_eq!(state.snapshots[0].stars, Some(2));
    let candidates = schedule::candidates(&state, at("2026-11-21T01:00:00Z"));
    let observation = candidates[0].observation.as_ref().unwrap();
    assert_eq!(observation.first_seen, "2026-11-21T00:17:00Z");
    assert_eq!(observation.last_seen, "2026-11-21T00:17:00Z");
    assert_eq!(observation.observations, 1);
    assert_eq!(
        candidates[0].sources[0]["captured_at"],
        "2026-11-21T00:17:00Z"
    );
}

#[test]
fn archive_reconciliation_accepts_legacy_names_and_ignores_retry_copies() {
    let directory = tempfile::tempdir().unwrap();
    for name in [
        "20240229T001700Z",
        "20240229T001700Z-retry",
        "refreshed-20240229T001700Z",
        "20240230T001700Z",
    ] {
        let path = directory.path().join(name);
        std::fs::create_dir(&path).unwrap();
        std::fs::write(
            path.join("manifest.json"),
            br#"{"captured_at":"2024-02-29T00:17:00Z","candidate_count":1}"#,
        )
        .unwrap();
    }
    assert_eq!(
        stamp(at("2024-02-29T03:17:00.999999999+03:00")),
        "20240229T001700Z"
    );
    let report = coverage::report(&[], at("2024-03-04T07:00:00Z"), directory.path()).unwrap();
    assert_eq!(report["archive"]["manifest_capture_count"], 1);
    assert_eq!(
        report["archive"]["manifests_without_snapshots"],
        json!([{
            "captured_at":"2024-02-29T00:17:00Z","candidate_count":1
        }])
    );
    assert_eq!(report["archive"]["invalid_manifests"], json!([]));
}

#[test]
fn fake_clock_preserves_subseconds_across_utc_midnight_and_epoch_fallback() {
    let clock = FakeClock::at("2026-12-31T23:59:59.999999999Z");
    assert_eq!(clock.now().date().to_string(), "2026-12-31");
    clock.sleep(StdDuration::from_nanos(1));
    assert_eq!(clock.now().rfc3339(), "2027-01-01T00:00:00+00:00");
    assert_eq!(clock.now().date().to_string(), "2027-01-01");
    clock.sleep(StdDuration::from_millis(123));
    clock.sleep(StdDuration::from_nanos(456_789));
    assert_eq!(clock.now().rfc3339(), "2027-01-01T00:00:00.123456789+00:00");
    assert_eq!(
        *clock.sleeps.borrow(),
        [
            StdDuration::from_nanos(1),
            StdDuration::from_millis(123),
            StdDuration::from_nanos(456_789)
        ]
    );
    assert_eq!(Timestamp::default().rfc3339(), "1970-01-01T00:00:00+00:00");
    assert_eq!(
        at("2027-01-01T00:00:00.123456789Z") - at("2026-12-31T23:59:59.999999999Z"),
        Duration::nanoseconds(123_456_790)
    );
}

#[test]
fn forwarded_card_date_still_uses_fixed_moscow_offset() {
    for (instant, expected) in [
        ("2024-02-28T20:59:59.999999999Z", "28\\.02\\.2024"),
        ("2024-02-28T21:00:00Z", "29\\.02\\.2024"),
        ("2026-12-31T21:00:00Z", "01\\.01\\.2027"),
        ("2027-01-01T00:00:00+03:00", "01\\.01\\.2027"),
    ] {
        let messages = crate::render_issue_messages(&json!({
            "edition":"daily","generated_at":instant,
            "entries":[{"full_name":"org/card","description":"Описание проекта","latest":{}}]
        }))
        .unwrap();
        assert_eq!(messages.len(), 1);
        assert!(
            messages[0].contains(&format!("*GitHub — находки дня*\n{expected}\n\n")),
            "{instant}"
        );
    }
}
