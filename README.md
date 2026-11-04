# GitHub Digest

Russian GitHub discoveries delivered to Telegram each day at 18:00 Moscow, with observations collected every three hours. Weekly editions remain available for explicit manual requests. See the [daily-only operating policy](docs/daily-only.md) and [queue and delivery semantics](docs/daily-weekly-queue.md).

## Cadence and candidate selection

- Collect candidate metadata at minute 17 every three hours (00:17, 03:17, …, 21:17 UTC). Collection itself uses no LLM calls or Telegram credentials. After 15:00 UTC, collection ticks may recover an unfinished daily issue. Actual captures and gaps are recorded in `coverage.json`.
- Prepare one daily issue at 15:00 UTC (18:00 Moscow). Delayed runs share the same daily slot. There is no automatic Monday publication or weekly recovery timer; manual weekly previews and delivery keep their existing gates.
- Sources are the first returned pages of weekly Trending, daily Trending, and GitHub Search for repositories created in the past seven days. This is a bounded sample, not a global ranking or code audit.
- Retain every real observation and keep undelivered findings in the edition's queue even after they leave Trending. Daily issues select at most 12 cards, oldest discovery date first, then daily Trending, weekly Trending, and Search within that date. Manual weekly issues select up to 30 detailed cards plus an index of other pending findings.
- Keep metadata and maximum observed rolling Trending values with their original timestamps in artifacts. Daily and weekly growth are never added together. Previously acknowledged repositories are excluded only from the edition that published them; the two ledgers remain independent.

## Message format

A message starts with the edition and date, followed by linked repository names and concise Russian descriptions. A card explains the concrete task and one useful mechanism, use case, or material limitation in one or two sentences, normally 160–320 characters and at most 420. Language is a hashtag such as `#Python`; total stars and an available positive Trending peak form a compact statistics line. `C++` and `C#` use valid tags `#CPP` and `#CSharp`.

Humor is optional: a short observation tied to that project can be kept, while filler, repeated templates, and duplicate jokes are omitted. There is no final issue summary, duplicate link list, `Метаданные:` label, capture timestamp, queue explanation, or “new repository from Search” line in the published cards. Source provenance, coverage, queue decisions, and evidence quotes remain in the review artifacts.

## Preview before publication

Publication is **off by default**. Leave repository variable `DIGEST_DELIVERY_ENABLED=false` until the generated preview has been reviewed. Both workflow routing and every sending CLI entry point enforce the gate. Preview/collection receive no Telegram token or destination.

Run `Run Bot` manually with `dry_run=true`, `preview_with_llm=true`, and `deliver_to_main_chat=false` for a real provider-enriched artifact without sending any message. The existing `LLM_FREE_TIER_CONFIRMED=yes` assertion and persisted request budgets remain mandatory; there is no paid fallback. Default dry runs do not call the provider.

Review `summary.md` (readable cards and provenance), `issue.md` (Telegram-formatted body), `ranking-input.json`, `selection.json`, `candidate-observations.json`, `weekly-observations.json`, `weekly-window.json`, `weekly-source-manifest.json`, `coverage.json`, `telegram-messages.json`, `manifest.json`, `llm-bundle.json`, and `release-audit.json` in the private run artifact. The same deterministic credential scan checks project text and actual rendered messages before preview acceptance or publication. Every actual payload also passes a local validator for the MarkdownV2 subset emitted by this renderer. Each message is bounded to 3,800 UTF-16 units; long issues split at project boundaries and preserve descriptions, jokes, and links. A single oversized card fails explicitly rather than losing its text. Capture timestamps and source decisions stay in the artifacts. Provider-enriched daily previews require validated cards; no synthetic issue summary is required. Artifacts contain only the current run, not the restored archive.

Only after approval of the preview, set `DIGEST_DELIVERY_ENABLED=true`. Recovery works while this variable is false; it never changes the variable. `Run Bot` can be enabled for collection and previews while the publication variable remains false. Real manual/technical sends and retry delivery also require the variable to be exactly `true`. Keep credentials private.

## Cadence and Actions minutes

Use `recover_weekly=true` with `dry_run=true` for an idempotent manual recovery. Sending inputs and stored-preview inputs cannot be combined with recovery. A completed slot falls back to collection. Ordinary manual previews can still be regenerated explicitly.

`bot-state/data/weekly-runs.json` records the slot, mode, run ID and uploaded artifact ID only after successful preparation, validation and artifact upload. Failed uploads cannot complete a preview. A preview receipt is separate from `delivery-state` and never marks an issue as sent. When publication is explicitly enabled, only authoritative `sent_slots` suppress delivery. State/archive writes remain serialized and reject concurrent changes. The run summary and artifact `schedule-recovery.json` explain the decision and nominal issue age.

A daily period opens at 18:00 Moscow; the manual weekly period opens Monday at 10:00 Moscow. Each edition uses a durable slot to prevent a second issue in that period. Frozen pending deliveries and acknowledged chunks are retained for safe recovery. Old checkpoint fields and published-project history are preserved.

Three-hour observation uses no provider calls and does not fetch README files. README evidence is fetched only for the selected cards. Enrichment uses the existing per-run/per-day caps and source-description fallback. Runner and provider usage still depend on collection/build time and paced requests; no promise of unlimited free capacity is made.
## Deduplication

The authoritative `delivery-state` checkpoint keeps a `covered` map of every repository ever published, keyed by the UTC date of its first appearance. It is written only after a digest has fully reached the main chat, so confirmed progress resumes on the next run, while an unconfirmed send stops for reconciliation. Progress is keyed by destination chat and digest ID; covered repositories are not lost. A credential-scan failure blocks delivery and leaves candidates uncovered. Offline and manual weekly previews may retain GitHub-description fallbacks. Daily AI delivery leaves unavailable or rejected descriptions pending. Each edition can deliver at most one issue per slot; all published chunks and pending state retain durable receipts. Legacy overview receipts remain readable but no new overview is sent.

## Evidence-based project analysis

Groq returns a concise Russian project description and an optional short joke grounded in GitHub metadata and the README excerpt. Generated descriptions must include an exact README evidence quote, validated against the supplied primary text and retained in the artifact. Quote presence proves provenance, not the truth of every generated assertion. README collection covers the whole selected pool. Missing primary evidence is recorded in source_gaps, and missing usable README evidence in readme_gaps, including previews with LLM disabled. Projects without usable README evidence retain the GitHub-description fallback for offline or manual weekly previews and stay queued for daily AI delivery. Rust supplies card titles and links, star counts, language hashtags, escaping, order and layout. Partial provider failure preserves completed descriptions; the publication audit checks credentials, message lengths, and the emitted MarkdownV2 subset. It does not certify factual accuracy or project quality.

## Configure GitHub Actions

Add these repository Actions secrets:

- `TELEGRAM_BOT_TOKEN`
- `TELEGRAM_CHAT_ID` (main chat: enabled daily or explicitly requested weekly editions are posted here)
- `TELEGRAM_TECH_CHAT_ID` (private debug supergroup for manual previews and refusal notices)
- `GROQ_API_KEY`

Groq defaults to `openai/gpt-oss-120b`; `GROQ_MODEL` overrides it. A completed enrichment pass uses one request per uncached project. There is no issue-summary request or reserved summary quota. Supported models use strict JSON Schema output and GPT-OSS uses low reasoning effort. Invalid description content can trigger one schema-repair completion; other models may retry a rejected JSON mode without that mode. Retryable HTTP failures get at most three attempts per completion. Every HTTP attempt reserves and persists its budget before sending: at most 40 requests per run and 100 per UTC day, including retries. Successful descriptions are paced 30 seconds apart; transient retries wait one and two seconds. There is no paid fallback or local inference.

Set `LLM_FREE_TIER_CONFIRMED=yes` only after confirming the provider account and current quota settings. This is an operator assertion, not an API guarantee against charges. Without the Groq key or this assertion, enrichment is skipped and GitHub descriptions remain available. Gemini credentials and model settings are not used by the bot.

No fresh candidates is a quiet outcome. Provider failures retain validated cards and leave unavailable daily descriptions pending; the credential scanner blocks unsafe publication text and attempts a technical notice. Telegram has no idempotency key; an ambiguous publication send stops for reconciliation rather than automatic retry. Cosmetic pinning and best-effort refusal notices are outside the publication checkpoint contract.

The workflow needs repository `contents: write` permission to update `delivery-state` and the `bot-state` source archive; updates are serialized. It also needs `actions: read` to download artifacts for the retry input.

## Code boundaries

- sources.rs collects Trending, Search, repository metadata and README evidence. It receives the covered set, output directory, source HTTP interface and clock explicitly.
- providers.rs owns Groq enrichment, schemas, retries, pacing and persisted request budgets. Its HTTP client, endpoint, clock and state path are explicit dependencies; production endpoints remain fixed.
- state_store.rs owns atomic state files; delivery.rs owns resumable chunk delivery. main.rs keeps CLI orchestration, ranking and deterministic rendering.
- Regression tests use loopback HTTP and a fake clock. They never need provider or Telegram credentials, real sleeps, or production state.

## Hosted validation

The repository template retains CI, action security checks, Dependabot, and its Rust toolchain configuration. CI checks formatting, compilation, Clippy, dependency advisories, dependency policy, and unused dependencies. To validate delivery without sending a Telegram message, run `Run Bot` manually with the default dry-run setting and inspect its uploaded artifact.

## Checkpoint and maintenance guarantees

State files are atomically replaced. After restoring the `bot-state` archive, the workflow
overlays the authoritative `delivery-state/weekly-gh-digest.json` checkpoint. First use migrates
the old state; authentication, transport, corrupt state, missing restore receipts, or stale
blob SHAs fail closed. Each intent and acknowledgement is synchronously committed through the
GitHub Contents API. Runner loss before the final archive push no longer loses delivery progress
or provider reservations. Source artifacts may still be absent after abrupt runner loss.

Each publication chunk has durable intent, content, destination, and receipt.
An interrupted or ambiguous send leaves `uncertain_delivery` and stops subsequent publication,
including after restart. Confirmed chunks are reused; changed content and legacy progress without
a known destination stop for reconciliation. Telegram cannot atomically commit with GitHub:
exactly-once delivery is not claimed. Local runs without the helper have local durability only.

To reconcile, stop concurrent runs and back up the checkpoint and blob SHA. Privately inspect the
identified destination and message. For a proven absent message, clear `uncertain_delivery` only.
For a proven delivered message, also record its actual ID under the matching
`delivery_progress[progress_key].project_message_ids[message_key]`. Preserve stored content and
all other state, and update using the inspected SHA. If the outcome remains unknown, keep the
intent pending. Legacy unscoped progress must be mapped to its verified destination, never reset.
This repository and its checkpoint branch must remain private because digest state contains
source material and delivery destinations. The workflow runs only from `main` with one shared
concurrency group. Enabling or disabling schedules remains an operator decision.

Dependabot batches weekly maintenance updates. CI and workflow-security gates require every constituent job to succeed; verified bot commits are matched to the exact checked head before native auto-merge. Native auto-merge is currently blocked for this private repository on GitHub Free because required branch protection is unavailable; the handler fails explicitly instead of merging without protection. An eligible plan and configured branch protection are prerequisites.

The checkout never persists its token. Legacy archive fetch/push receives `GH_TOKEN` only in
its own step and uses a process-scoped Git credential helper; no authentication is written
to the Git configuration. Dependency builds and caches receive no publication credential.
The checkpoint helper and bot receive their required credentials only during their runtime steps.

The checkpoint uses the application's typed JSON state, preserving existing fields and adding
defaults only for compatible fields. This migration creates its
single known state key only when `delivery-state` is confirmed absent. A pre-existing branch
without that key is not an empty installation: it stops for explicit initialization or
reconciliation. Adding another key later requires an intentional migration, never automatic
fallback to an empty state. The `bot-state` archive is the initial migration source only;
subsequent runs always restore the authoritative checkpoint over that archive.

After migration, use the current workflow from `main`; do not rerun historical jobs from before
the checkpoint migration, because their old code does not implement this delivery contract.

## Editorial and coverage boundaries

Card descriptions use primary repository descriptions and README excerpts. Prompts ask for concrete, concise explanations and forbid unverified safety, quality, legal, and financial guarantees. The text describes author-stated capabilities; it is not runtime, legal, or security validation. A human preview remains required before publication is enabled.

Every collection retains all parsed identities from both Trending pages and the first 30 Search results, including metadata failures; raw responses, per-candidate errors, and complete observation identities are archived. This does not mean every repository matching Search was fetched. Weekly provenance lists every retained capture and explicitly marks missing manifests. Historical early caps and missing captures cannot be reconstructed. An appearance entirely between three-hour observations can still be missed.

## Inspect actual observation coverage

Every collection and ordinary preview writes `coverage.json`. It reports the rolling seven-day window, actual capture and repository counts, observed intervals, leading/trailing gaps, missing source manifests, archived captures absent from retained state, and historical filtered captures. It records the nominal cron separately: snapshots alone cannot identify which captures were scheduled, so the report does not invent a scheduled success percentage or reconstruct missing observations.

Preview artifacts retain coverage notices and capture timestamps without adding operational explanations to reader-facing cards. `release-audit.json` records the scope of its deterministic checks explicitly; an accepted payload is not an editorial, licensing, runtime, or security approval. A project's appearance in Trending does not establish a new release, open-source licensing, or independently measured performance. Provider instructions require precise technical terminology and prohibit unsupported privacy, licensing, safety, and quality guarantees. Human review remains necessary before publication is enabled.

## Re-render a saved issue offline

Dispatch `Run Bot` with `preview_source_run_id` set to a successful run of `Run Bot` from `main`. This downloads its flat artifact as data, validates the recorded selection and publication format, and re-renders the stored cards with the current layout. Both daily and weekly artifacts are supported. Sending or recovery inputs cannot be combined with this option. No provider key or Telegram credentials are injected, even if `preview_with_llm=true` is also selected.

The local equivalent is `weekly-gh-digest preview-stored <run-directory>`. It preserves source descriptions, observation dates, and provenance, and writes the current preview files. `format-refresh.json` records the source generation time, refresh time, zero provider requests, message count, and editorial policy version; `format_refreshed_at` records the refresh in the digest. `preview-source.json` identifies the originating Actions run. This operation changes presentation only and does not generate new analyses or acknowledge publication.
