# GitHub Digest

Twice-daily Russian digest of fresh GitHub projects in a single cohesive message, This-Week-in-Rust style, delivered to Telegram. Rust bot derived from [`qqrm/template-tg-bot-rs`](https://github.com/qqrm/template-tg-bot-rs). It runs on hosted GitHub Actions; there is no local or always-on bot process.

## What the workflow does

- Probes twice a day — 07:00 and 19:00 UTC (10:00 and 22:00 Moscow time) — and captures candidate metrics from the first pages of GitHub Trending (daily and weekly) and GitHub Search for repositories created in the previous seven days. This is a bounded candidate sample, not a global GitHub ranking.
- Each three-hour UTC slot delivers at most one issue, no matter how many runs, retries, or manual dispatches hit it: the issue is a single cohesive message (split into continuation chunks only when large), with a compact block per fresh project — link, star movement, language, a one-line description. At most one issue per slot is recorded in `sent_slots` on `bot-state`.
- The issue contains only projects that never appeared in any earlier issue. Days with no rotation keep both chats silent; the run log records that there are no fresh candidates.
- The `Probe Trigger` workflow acts as a safety net for GitHub's best-effort scheduler: inside each delivery window it dispatches `Run Bot` when nothing has started yet. Redundant dispatches are harmless: the slot guard keeps them quiet.
- Groq descriptions are best effort: a partial or failed pass falls back to the repository's GitHub description instead of skipping the issue. The deterministic credential scan blocks delivery and records its result in release-audit.json. There is no Gemini review or other model-based publication audit.
- Stores source responses, manifests, selection traces, and star history on the `bot-state` branch. Each run also uploads a 90-day artifact with the issue text (`issue.md`) and provenance.
- Manual `workflow_dispatch` defaults to `dry_run=true` (preview artifact); `dry_run=false` sends the issue preview to the technical chat; `deliver_to_main_chat=true` performs the slot delivery (one per slot).

A digest is not a fixed-size top: it publishes the whole fresh difference between the current pool and everything already reviewed. Only repositories with at least 5,000 total stars are interesting; anything smaller is dropped before the pool cut and never reaches an issue. The pool contains up to 30 fresh candidates across the sources, ordered deterministically: weekly Trending by displayed weekly star count (`+N`) first, then daily Trending by daily `+N`, then new repositories by total stars — three newcomers means a three-item issue, twenty-five means twenty-five. Covered repositories do not consume pool slots, so newcomers always reach the issue. A `selection.json` pool trace is stored with each run, and a retry re-validates a stored digest against it.

## Cadence and Actions minutes

Two probes per day keep the digest fresh. Each slot publishes the fresh pool, up to 30 projects, and already covered repositories do not consume pool slots. Workflow duration depends on collection, builds and paced provider requests; inspect Actions usage for actual runner cost.

## Deduplication

The `bot-state` branch keeps a `covered` map of every repository ever published, keyed by the UTC date of its first appearance. It is written only after a digest has fully reached the main chat, so an interrupted send resumes on the next run (per-message progress is keyed by digest id) instead of duplicating messages, and covered repositories are not lost. A credential-scan failure blocks delivery and leaves candidates uncovered. Missing or failed Groq descriptions use GitHub descriptions; an unavailable overview uses the deterministic fallback. Several digests can be delivered per day; each gets its own posts, linked pinned overview, and pending state.

## Evidence-based project analysis

Groq returns a Russian project description and short joke grounded in GitHub metadata and the README excerpt. One exact source excerpt is stored in the run artifact. It also attempts one overview after a completed description pass. Rust supplies card titles and links, star counts, language, source labels, escaping, order and layout. Partial provider failure preserves completed descriptions and falls back to GitHub text for the remaining projects; the publication audit is a deterministic credential scan only.

## Configure GitHub Actions

Add these repository Actions secrets:

- `TELEGRAM_BOT_TOKEN`
- `TELEGRAM_CHAT_ID` (main chat: the daily digest is posted here automatically)
- `TELEGRAM_TECH_CHAT_ID` (private debug supergroup for manual previews and refusal notices)
- `GROQ_API_KEY`

Groq defaults to `openai/gpt-oss-120b`; `GROQ_MODEL` overrides it. A completed enrichment pass uses one request per project and one overview request. Supported models use strict JSON Schema output and GPT-OSS uses low reasoning effort. Invalid description content can trigger one schema-repair completion; other models may retry a rejected JSON mode without that mode. Retryable HTTP failures get at most three attempts per completion. Every HTTP attempt reserves and persists its budget before sending: at most 40 requests per run and 100 per UTC day, including retries. Successful descriptions are paced 30 seconds apart; transient retries wait one and two seconds. There is no paid fallback or local inference.

Set `LLM_FREE_TIER_CONFIRMED=yes` only after confirming the provider account and current quota settings. This is an operator assertion, not an API guarantee against charges. Without the Groq key or this assertion, enrichment is skipped and GitHub descriptions remain available. Gemini credentials and model settings are not used by the bot.

No fresh candidates is a quiet outcome. Provider failures use the documented fallback; the credential scanner blocks unsafe publication text and attempts a technical notice. Telegram has no idempotency key, so an ambiguous send result can still duplicate the one message accepted before a runner stopped.

The workflow needs repository `contents: write` permission to update `bot-state`; updates are serialized. It also needs `actions: read` to download artifacts for the retry input.

## Code boundaries

- sources.rs collects Trending, Search, repository metadata and README evidence. It receives the covered set, output directory, source HTTP interface and clock explicitly.
- providers.rs owns Groq enrichment, schemas, retries, pacing and persisted request budgets. Its HTTP client, endpoint, clock and state path are explicit dependencies; production endpoints remain fixed.
- state_store.rs owns atomic state files; delivery.rs owns resumable chunk delivery. main.rs keeps CLI orchestration, ranking and deterministic rendering.
- Regression tests use loopback HTTP and a fake clock. They never need provider or Telegram credentials, real sleeps, or production state.

## Hosted validation

The repository template retains CI, action security checks, Dependabot, and its Rust toolchain configuration. CI checks formatting, compilation, Clippy, dependency advisories, dependency policy, and unused dependencies. To validate delivery without sending a Telegram message, run `Run Bot` manually with the default dry-run setting and inspect its uploaded artifact.

## Checkpoint and maintenance guarantees

State files are atomically replaced. Only a confirmed missing file/remote state branch permits bootstrap; unreadable, corrupt, or unreachable state stops the run. The save step only follows a successful restore and refuses to overwrite a state branch that changed during the run. Delivery checkpoints are saved after each successful chunk, and remaining chunks resume without re-sending locally recorded messages.

Telegram has no idempotency key: a crash after Telegram accepts a message but before its checkpoint is persisted can still duplicate that message. GitHub runner loss before the final state-branch push can also lose local checkpoints. Exactly-once delivery is not claimed.

Dependabot batches weekly maintenance updates. CI and workflow-security gates require every constituent job to succeed; verified bot commits are matched to the exact checked head before native auto-merge. Native auto-merge is currently blocked for this private repository on GitHub Free because required branch protection is unavailable; the handler fails explicitly instead of merging without protection. An eligible plan and configured branch protection are prerequisites.
