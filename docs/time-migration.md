# Timestamp compatibility

The digest uses `time` instead of `chrono` following
[RUSTSEC-2026-0543](https://rustsec.org/advisories/RUSTSEC-2026-0543.html).
The dependency policy in `deny.toml` still rejects unmaintained packages.

`src/timestamp.rs` keeps UTC arithmetic and the existing external encodings
in one place. These encodings differ intentionally:

| Use | Example | Contract |
| --- | --- | --- |
| Observation keys, manifests and reporting windows | `2026-11-21T15:00:00Z` | Whole seconds, UTC `Z`; never rewrite existing snapshot strings. |
| Checkpoint `daily_retry_at` and serialized retry diagnostics | `2026-11-21T15:00:00.123400Z` | Zero, three, six or nine fractional digits, UTC `Z`. |
| Explicit diagnostic RFC3339 strings | `2026-11-21T15:00:00.123400+00:00` | The same fractional grouping, explicit UTC offset. |
| Archive and digest identifiers | `20261121T150000Z` | Existing compact UTC timestamp. |

All parsed instants normalize to UTC. External timestamps keep strict RFC3339
parsing; the checkpoint reader also accepts canonical extended years and the
legacy relaxed spellings covered by its tests. Invalid checkpoint values abort
loading instead of starting an empty queue. Leap-second strings retain their
identity and the previous elapsed-time semantics rather than being clamped to
the preceding second by the new backend.

Scheduling remains fixed UTC+3: the daily issue changes at 18:00 Moscow,
the weekly issue at Monday 10:00 Moscow. Retry delays remain exactly 24 elapsed
hours; daily and weekly observation windows retain their inclusive lower bound
and exclude future observations. Provider budgets continue to use UTC dates.

## State and rollout

No checkpoint rewrite, schema increment or queue reset is required. Existing
receipts, pending payloads, issue IDs and snapshot strings remain intact.
Canonical timestamps written by the new serializer can also be read by the
previous executable. No new time-zone database or host-local timezone setting
is needed.

`src/test_time_compatibility.rs` checks literal legacy checkpoints, retry
eligibility after reload, unchanged pending payloads, deduplication, archive
reconciliation, nanosecond publication boundaries and Moscow date rendering.
The timestamp module contains golden wire-format and parsing checks. These run
in the existing CI nextest job, alongside the unchanged advisory gate.

For an additional read-only comparison against an older executable, run both
versions' `preview-retained` command with the same state file and explicit
`--at=RFC3339`, each into a different new directory. Compare every generated
file. Repeat with `--daily` and at a saved retry deadline. This command makes
no provider or delivery requests and does not modify the input state.
