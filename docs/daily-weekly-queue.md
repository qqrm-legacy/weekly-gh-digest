# Daily discoveries and weekly summaries

## Durable state, not a text hand-off

Collection still runs every three hours. All observations enter `State.snapshots`
before editorial selection and remain in the existing durable checkpoint and
source archive. No Redis deployment is needed.

Each edition has a persistent logical queue: all retained valid observations,
minus repositories successfully acknowledged by that edition. This queue is not
restricted to the latest day or week. A candidate seen once, then removed from
Trending, stays pending through outages and capacity limits. Raw history is not
deleted when a queue item is acknowledged.

`covered` remains the weekly receipt map for compatibility. The new
`editions.daily_covered` map is independent. `editions.analyses` retains analyses
between runs and editions; reuse requires exactly matching primary description
and README text. Metrics and timestamps come from observations, not model prose.
Sources are untrusted data, never executable instructions.

Legacy weekly receipts seed daily receipts once on migration, avoiding replay
of already published pre-daily issues. After migration, neither format's
acknowledgements consume the other's queue. Identity is case-insensitive
`owner/name`; transfers/renames are not yet reconciled by GitHub repository ID.

## Editorial policies

Daily: at most 12 detailed discoveries per issue. Oldest discovery date first,
then daily Trending peak, weekly Trending peak and Search within that date.
This prevents new popular projects from starving the backlog. Overflow remains
pending even after leaving Trending. New means new to the feed, not necessarily
newly created or newly released.

Weekly: up to 30 detailed cards using the existing source priority. Every other
pending discovery is published in a linked index, split into Telegram-safe
messages. The index is part of the delivered payload, not just a hidden file.
All daily discoveries therefore remain represented in the weekly summary even
when there are more than 30. Older undelivered discoveries are included and counted
as carried backlog. A weekly index appearance is a weekly publication, but does
not mark that project's daily card delivered.

Seven-day observation and coverage artifacts remain separate from the all-history
pending queue. `queue.json` records queued candidates, detailed cards, carried
backlog and expected remainder after successful delivery. Overlapping Trending
rolling counts are never summed; peak metrics retain source timestamps.

## Delivery, then acknowledgement

A preview, uploaded artifact, collection or generated analysis never acknowledges
a repository. Only successful delivery of every message, including the weekly
index and final overview, acknowledges the precise names in that edition.
New discoveries and capacity overflow are never cleared wholesale.

Daily slots are `daily:YYYY-MM-DD`, starting at 07:00 UTC (10:00 Moscow).
Weekly slots remain `weekly:YYYY-MM-DD`, Monday 07:00 UTC. Message progress is
scoped by chat and edition-prefixed digest ID. A pending payload is frozen before
sending; recovery reuses it without regenerating or trimming it. Pending issues
no longer expire after two weeks and can resume across date/week boundaries.

Existing remote intent/acknowledgement checkpoints and shared workflow concurrency
are retained. Confirmed chunks are not resent after a restart. Ambiguous Telegram
results require operator reconciliation rather than blind retries. This is not an
unconditional exactly-once claim: Telegram and the state store do not share an
atomic transaction. Collection and previews continue while delivery is reconciled.

## Schedule and permissions

- `17 */3 * * *`: collect; recover unfinished weekly work first, then daily work.
- `0 7 * * 1`: existing weekly publication/recovery slot, unchanged.
- `37 7 * * *`: daily issue, using the same daily identity on delayed recovery.

`DIGEST_DELIVERY_ENABLED=true` remains mandatory for every send. Daily sends also
require `DIGEST_DAILY_DELIVERY_ENABLED=true`. Neither variable is enabled by this
change. Missing/false daily permission produces only a preview even when the
weekly gate is enabled. Manual dispatch defaults to weekly dry-run; select
`digest_format=daily` for a daily preview. Technical delivery retries also check
the stored edition's gate.

Daily artifact completion uses `data/daily-runs.json`; weekly completion stays in
`data/weekly-runs.json`. Preview receipts never substitute for `sent_slots`.
Explicit `recover_weekly` retains its weekly-only and never-send semantics.

## Read-only replay

```sh
cargo build --locked
./target/debug/weekly-gh-digest preview-retained /tmp/state.json /tmp/daily-preview --daily --at=2026-10-31T12:00:00Z
./target/debug/weekly-gh-digest preview-retained /tmp/state.json /tmp/weekly-preview --at=2026-10-31T12:00:00Z
```

Output directories must be new. The command reads a saved state copy, uses source
descriptions or matching cached analyses, and writes only preview artifacts. It
makes no GitHub, LLM or Telegram requests and does not modify input state. Offline
previews are labelled, not presented as fresh LLM analyses.

Run the repository's Rust checks and:

```sh
python3 -m unittest discover -s .github/scripts -p 'test_*.py'
```

Tests cover disappearing projects, prolonged outages, independent acknowledgements,
capacity carry-over, migration, source-sensitive analysis reuse, date boundaries,
frozen overdue payloads, full weekly indexes, chunk limits and safe daily routing.

## Coverage limits

Retention protects actually observed findings. It cannot reconstruct projects
that entered and left all configured sources between collection ticks, or data
missed during source failures. Bounded searches are not a GitHub census. Existing
source manifests and coverage notices remain explicit about gaps. History currently
grows without pruning; future compaction must preserve both queues and evidence.
