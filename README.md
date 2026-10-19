# GitHub Digest

Weekly Russian GitHub summary delivered to Telegram, with daily source collection on hosted GitHub Actions.

## Weekly issue and candidate selection

- Collect candidate metadata daily at 06:17 UTC (09:17 Moscow), without LLM calls or Telegram credentials.
- Prepare one issue on Monday at 07:00 UTC (10:00 Moscow). GitHub scheduling is best effort: a delayed run retains the same weekly issue identity. The Monday-only Probe Trigger can recover a missed run after publication is enabled.
- Sources remain the first returned pages of weekly Trending, daily Trending, and GitHub Search for repositories created in the past seven days. This is a bounded sample, not a global ranking or a code/security audit.
- Keep the latest real observation per repository from the preceding seven days. A candidate seen earlier in the week remains available even if absent from Monday's Trending page. Retained observation timestamps and raw source archives preserve provenance; daily and weekly star counts are never added together.
- There is no minimum total-star threshold, including in the Search query. Deduplication still excludes previously published repositories. The bounded collection pool and issue each contain up to 30 candidates, with stable priority: weekly Trending +N, daily Trending +N, then new repositories by total stars. Every cap/exclusion remains recorded.
- The issue includes a Russian synthesis of the week's observed projects and compact source-backed cards. Metrics retain their capture dates. Model output does not establish project quality, safety, or actual runtime behavior.

## Preview before publication

Publication is **off by default**. Leave repository variable `DIGEST_DELIVERY_ENABLED=false` until the generated preview has been reviewed. Both workflow routing and every sending CLI entry point enforce the gate. Preview/collection receive no Telegram token or destination.

Run `Run Bot` manually with `dry_run=true`, `preview_with_llm=true`, and `deliver_to_main_chat=false` for a real provider-enriched artifact without sending any message. The existing `LLM_FREE_TIER_CONFIRMED=yes` assertion and persisted request budgets remain mandatory; there is no paid fallback. Default dry runs do not call the provider.

Review `summary.md` (overview and full cards), `issue.md` (Telegram-formatted body), `ranking-input.json`, `selection.json`, `weekly-observations.json`, `manifest.json`, `llm-bundle.json`, and `release-audit.json` in the private run artifact. The same deterministic credential scan checks both the overview and project text before preview acceptance or publication.

Only after approval of the preview, set `DIGEST_DELIVERY_ENABLED=true` and enable `Probe Trigger` if recovery is wanted. `Run Bot` can be enabled for collection and previews while the publication variable remains false. Real manual/technical sends and retry delivery also require the variable to be exactly `true`. Keep credentials private.

## Cadence and Actions minutes

A weekly period opens Monday at 10:00 Moscow. Retries, late runs and manual sends share one durable slot, preventing a second issue that week. Pending legacy or earlier-week deliveries stop for reconciliation rather than being replayed into the new period. Old checkpoint fields and published-project history are preserved.

Daily collection uses no provider calls. Weekly enrichment uses the existing per-run/per-day caps and source-description fallback. Runner and provider usage still depend on collection/build time and paced requests; no promise of unlimited free capacity is made.
## Deduplication

The authoritative `delivery-state` checkpoint keeps a `covered` map of every repository ever published, keyed by the UTC date of its first appearance. It is written only after a digest has fully reached the main chat, so confirmed progress resumes on the next run, while an unconfirmed send stops for reconciliation. Progress is keyed by destination chat and digest ID; covered repositories are not lost. A credential-scan failure blocks delivery and leaves candidates uncovered. Missing or failed Groq descriptions use GitHub descriptions; an unavailable overview uses the deterministic fallback. At most one issue can be delivered per weekly period; its chunks, overview and pending state retain durable receipts.

## Evidence-based project analysis

Groq returns a Russian project description and short joke grounded in GitHub metadata and the README excerpt. One exact source excerpt is stored in the run artifact. It also attempts one overview after a completed description pass. Rust supplies card titles and links, star counts, language, source labels, escaping, order and layout. Partial provider failure preserves completed descriptions and falls back to GitHub text for the remaining projects; the publication audit is a deterministic credential scan only.

## Configure GitHub Actions

Add these repository Actions secrets:

- `TELEGRAM_BOT_TOKEN`
- `TELEGRAM_CHAT_ID` (main chat: the weekly digest is posted here after publication is enabled)
- `TELEGRAM_TECH_CHAT_ID` (private debug supergroup for manual previews and refusal notices)
- `GROQ_API_KEY`

Groq defaults to `openai/gpt-oss-120b`; `GROQ_MODEL` overrides it. A completed enrichment pass uses one request per project and one overview request. Supported models use strict JSON Schema output and GPT-OSS uses low reasoning effort. Invalid description content can trigger one schema-repair completion; other models may retry a rejected JSON mode without that mode. Retryable HTTP failures get at most three attempts per completion. Every HTTP attempt reserves and persists its budget before sending: at most 40 requests per run and 100 per UTC day, including retries. Successful descriptions are paced 30 seconds apart; transient retries wait one and two seconds. There is no paid fallback or local inference.

Set `LLM_FREE_TIER_CONFIRMED=yes` only after confirming the provider account and current quota settings. This is an operator assertion, not an API guarantee against charges. Without the Groq key or this assertion, enrichment is skipped and GitHub descriptions remain available. Gemini credentials and model settings are not used by the bot.

No fresh candidates is a quiet outcome. Provider failures use the documented fallback; the credential scanner blocks unsafe publication text and attempts a technical notice. Telegram has no idempotency key; an ambiguous publication send stops for reconciliation rather than automatic retry. Cosmetic pinning and best-effort refusal notices are outside the publication checkpoint contract.

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

Each publication chunk and final overview has durable intent, content, destination, and receipt.
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
