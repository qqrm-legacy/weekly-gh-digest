# Daily discoveries and manual weekly editions

Automatic delivery is daily-only; see [the current schedule](daily-only.md).
`Run Bot` uses the verified binary and matching runtime scripts from the latest
stable [GitHub Release](releases.md).

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
between runs and editions; reuse requires exactly matching primary description,
README text and editorial policy version. Fresh and cached cards are checked
after redaction and joke selection against the final editorial, rendered-payload
and credential gates before acceptance. Daily editorial checks use the same
full repository snapshot as the final issue check. Invalid cached analyses are
evicted; their daily retry is checkpointed without spending a model request or
acknowledging the finding. Metrics and timestamps come from observations, not
model prose. Sources are untrusted data, never instructions.

Legacy weekly receipts seed daily receipts once on migration, avoiding replay
of already published pre-daily issues. After migration, neither format's
acknowledgements consume the other's queue. Identity is case-insensitive
`owner/name`; transfers/renames are not yet reconciled by GitHub repository ID.

## Editorial policies

Daily: at most 12 concise discoveries per issue. Selection reserves eight places
for findings first observed in the last 24 hours and four for older pending
findings. Fresh findings rank by daily Trending, weekly Trending, then Search;
within each source, the higher metric comes first, followed by queue time and
repository name for ties. Older findings keep earliest queue time first, then
the same source and metric priority. Fresh cards appear before backlog cards.
Either group uses the other's unfilled places, so an all-fresh or all-backlog
pool can still select 12. These are candidate quotas; rejected or unavailable
explanations can reduce the final issue.

Freshness uses the earliest retained observation in the inclusive UTC interval
`[now - 24 hours, now]`. A later sighting or retry time does not make an old
finding fresh. Overflow stays pending and joins the backlog after leaving that
window; no observation is dropped or acknowledged by selection.

Backlog queue time starts at first discovery. A confirmed local explanation failure
(missing or unusable README, rejected quotation, invalid description or payload
after repair, or a rejected cached card) sets `editions.daily_retry_at` to 24 hours
after that attempt. The finding stays pending, then rejoins at that fixed retry
time instead of retaining its old position. Within the backlog, later discoveries
cannot continually overtake a due retry. Matching cached or newly accepted
analyses clear the retry.
Only fetched README content or HTTP 404/410 establishes a local README failure;
transport errors, authentication/rate-limit failures, exhausted request budgets
and unvisited candidates do not move queue positions. Weekly and offline
enrichment never changes daily retries. Retry checkpoints are saved even when
all selected cards fail, so the next run can reach later candidates. Daily ACKs
still require complete delivery; the 12-card and 40/run, 100/UTC-day request caps
are unchanged.
Overflow remains pending even after leaving Trending. New means new to the feed,
not necessarily newly created or newly released. With AI enabled, only completed
Russian explanations are included; raw metadata is not a published daily card.

Manual weekly: up to 30 cards using the existing source priority. Every other
pending discovery is published in a linked index, split into Telegram-safe
messages. The index is part of the delivered payload, not just a hidden file.
All daily discoveries therefore remain represented in a manual weekly edition even
when there are more than 30. Older undelivered discoveries are included and counted
as carried backlog. A weekly index appearance is a weekly publication, but does
not mark that project's daily card delivered.

Seven-day observation and coverage artifacts remain separate from the all-history
pending queue. `queue.json` records queued candidates, detailed cards, carried
backlog and expected remainder after successful delivery. Overlapping Trending
rolling counts are never summed; peak metrics retain source timestamps. This is
not a strict calendar-week top-20 ranking or a release/change log for every repo.

## Delivery, then acknowledgement

A preview, uploaded artifact, collection or generated analysis never acknowledges
a repository. Only successful delivery of every message, including a manual weekly
index, acknowledges the precise names in that edition.
New discoveries and capacity overflow are never cleared wholesale.

Daily slots are `daily:YYYY-MM-DD`, starting at 15:00 UTC (18:00 Moscow).
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

- `17 */3 * * *`: collect; after 15:00 UTC recover only the due daily issue
  or daily preview. Before that deadline and after an acknowledged issue, collect only.
- `0 15 * * *`: daily issue, using the same daily identity on delayed recovery.
- Weekly work requires a manual request; the retired Monday cron is collection-only.

`DIGEST_DELIVERY_ENABLED=true` remains mandatory for every send. Daily sends also
require `DIGEST_DAILY_DELIVERY_ENABLED=true`. Neither variable is enabled by a
code merge. Missing/false daily permission produces only a preview even when the
weekly gate is enabled. Manual dispatch defaults to weekly dry-run; select
`digest_format=daily` for a daily preview. Technical delivery retries also check
the stored edition's gate.

Daily artifact completion uses `data/daily-runs.json`; weekly completion stays in
`data/weekly-runs.json`. Preview receipts never substitute for `sent_slots`.
Explicit `recover_weekly` retains its weekly-only and never-send semantics.

After reviewing the merged code, target and provider configuration, an authorized
operator can activate the scheduled production pipeline with:

```sh
gh variable set DIGEST_DELIVERY_ENABLED --repo qqrm/weekly-gh-digest --body true
gh variable set DIGEST_DAILY_DELIVERY_ENABLED --repo qqrm/weekly-gh-digest --body true
```

This permits scheduled daily and explicitly requested weekly delivery using the configured
`TELEGRAM_CHAT_ID`. It does not point production at the technical chat or migrate
test acknowledgements. To stop all publication, set `DIGEST_DELIVERY_ENABLED=false`.
No secrets belong in these variables or in source control.

## Read-only replay

These commands build the current checkout for local development and produce only
offline artifacts. Production and ordinary Actions previews use `Run Bot` and its
released binary.

```sh
cargo build --locked
./target/debug/weekly-gh-digest preview-retained /tmp/state.json /tmp/daily-preview --daily --at=2026-11-03T15:00:00Z
./target/debug/weekly-gh-digest preview-retained /tmp/state.json /tmp/weekly-preview --at=2026-11-03T15:00:00Z
```

Output directories must be new. The command reads a saved state copy, uses source
descriptions or matching cached analyses, and writes only preview artifacts. It
makes no GitHub, LLM or Telegram requests and does not modify input state. Offline
previews are labelled, not presented as fresh LLM analyses.

`Evening digest validation` is an explicit developer validation of the selected
source checkout, including its Rust checks and a real free-provider preview on a
local copy of state. It has read-only repository permission, no Telegram
credentials, and no remote checkpoint writer. Its artifact is a preview, not a
delivery receipt or a production deployment. Use `Run Bot` for a preview of the
released production version.

Run the repository's Rust checks and:

```sh
python3 -m unittest discover -s .github/scripts -p 'test_*.py'
```

Tests cover disappearing projects, prolonged outages, independent acknowledgements,
capacity carry-over, migration, source-sensitive analysis reuse, date boundaries,
frozen overdue payloads, full weekly indexes, chunk limits, daily recovery priority
and safe publication routing. Daily selection tests cover the 24-hour boundary,
fresh popularity, reserved backlog progress during continuing arrivals, capacity
transfer in both directions, stable ordering and retained retry positions.

## Evening daily edition

The daily deadline is 18:00 Moscow (15:00 UTC), without a test expiry in the normal
Run Bot workflow. Three-hour collection remains enabled so transient observations
are retained. Before the daily deadline, automatic recovery only collects observations. Unsuccessful evening work can retry at later
collection ticks. A successfully acknowledged daily slot is never resent.

Daily LLM editions publish only source-backed Russian explanations that completed
validation. Raw README/metadata fallbacks stay in the daily queue. There is no
issue synopsis or separate summary request. Completed analyses are checkpointed
individually and reused when
sources and editorial policy match. Weekly eligibility remains independent.
README paragraph selection accepts LF and CRLF line endings while preserving
the original quoted text for exact matching.

Mini-reviews normally use two or three short paragraphs and 800–1200 characters
(requested maximum 1500; responses up to 2000 are accepted without truncation)
to explain the task, user-facing result, mechanism, two or three
useful capabilities or distinctions, practical use, and material documented
requirements or limits. Paragraph breaks and useful details are preserved; each
sentence adds information, and sparse sources need no padding. Humor is optional
and project-specific; language is rendered as a hashtag. Selection provenance remains in artifacts. Language, Latin product-name provenance and known
terminology regression checks are conservative heuristics, not a complete factual
audit. They can defer valid text and cannot detect all unsupported claims.

Free Groq is used only with explicit free-tier confirmation; existing request caps,
retry-after handling and no paid fallback remain in place. Exact on-the-minute
delivery or an AI issue every day cannot be guaranteed by Actions or a free
external provider. If no valid cards are available, the run fails
without publishing raw fallback text and retains the queue.

## Coverage limits

Retention protects actually observed findings. It cannot reconstruct projects
that entered and left all configured sources between collection ticks, or data
missed during source failures. Bounded searches are not a GitHub census. Existing
source manifests and coverage artifacts remain explicit about gaps. History currently
grows without pruning; future compaction must preserve both queues and evidence.
