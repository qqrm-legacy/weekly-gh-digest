# GitHub Digest

Twice-daily Russian digest of fresh GitHub projects, delivered to Telegram. Rust bot derived from [`qqrm/template-tg-bot-rs`](https://github.com/qqrm/template-tg-bot-rs). It runs on hosted GitHub Actions; there is no local or always-on bot process.

## What the workflow does

- Probes twice a day — 07:00 and 19:00 UTC (10:00 and 22:00 Moscow time) — and captures candidate metrics from the first pages of GitHub Trending (daily and weekly) and GitHub Search for repositories created in the previous seven days. This is a bounded candidate sample, not a global GitHub ranking.
- After each probe, prepares a Russian digest of the freshest candidates and sends it to the main Telegram chat automatically — but only the projects that never appeared in any earlier digest. Days with no rotation produce a short "nothing new" notice instead, so both daily slots always print something.
- The `Probe Trigger` workflow acts as a safety net for GitHub's best-effort scheduler: inside each delivery window it dispatches `Run Bot` when nothing has started yet. A redundant dispatch is harmless because digest selection is deduplicated against `bot-state` and an extra run exits quietly.
- Groq writes one Russian technical description per project from GitHub metadata and the README excerpt; one more Groq request produces the overview intro and per-project teasers. Gemini runs an independent publication audit. The digest is sent only when every description is complete, the deterministic credential scan is clean, and the audit approves; otherwise the reason is reported to the technical chat and nothing is published.
- Stores source responses, manifests, selection traces, and star history on the `bot-state` branch. Each run also uploads a 90-day artifact with the digest preview and provenance.
- Manual `workflow_dispatch` defaults to `dry_run=true` and uploads a preview artifact; `dry_run=false` delivers a full debug preview (project posts, linked overview, audit) to the private technical chat; `deliver_to_main_chat=true` performs a scheduled delivery immediately.

Every probe takes at most ten projects, ordered deterministically: weekly Trending by displayed weekly star count (`+N`) first, then daily Trending by daily `+N`, then new repositories by total stars. Repositories that were covered but rotated out of the top wait for the next slot's cap rather than displacing fresher candidates. Current total stars are shown separately and do not determine order. A `selection.json` pool trace is stored with each run, and a retry re-validates a stored digest against it.

## Cadence and Actions minutes

Trending rotates substantially within a day, so two probes (morning and evening) keep the digest fresh while posting at a comfortable pace: each slot posts up to ten projects that have never been covered, and everything new that did not fit waits for the next slot. The probe with the Rust build cache takes roughly two to four minutes, so ~60 runner minutes per month go to the digest itself, far below the private-repo free allowance; the `Probe Trigger` safety net adds a handful of short runs per week.

## Deduplication

The `bot-state` branch keeps a `covered` map of every repository ever published, keyed by the UTC date of its first appearance. It is written only after a digest has fully reached the main chat, so an interrupted send resumes on the next run (per-message progress is keyed by digest id) instead of duplicating messages, and covered repositories are not lost. A digest whose descriptions, credential scan, or audit fail is not delivered and does not mark its candidates covered — they re-enter the next probe's selection. Several digests can be delivered per day; each gets its own posts, linked pinned overview, and pending state.

## Evidence-based project analysis

Groq generates one field per project: a compact Russian technical analysis grounded in GitHub metadata and the README excerpt, with one verbatim source excerpt stored in the run artifact. Rust deterministically supplies the card title and link, rank, star counts, language, source kind, labels, MarkdownV2 escaping, project order, and overview layout with Telegram post links. Gemini does not write cards or format the overview; it is used only for the separate publication audit. A failed or incomplete analysis or overview blocks delivery instead of publishing partial model output.

## Configure GitHub Actions

Add these repository Actions secrets:

- `TELEGRAM_BOT_TOKEN`
- `TELEGRAM_CHAT_ID` (main chat: the daily digest is posted here automatically)
- `TELEGRAM_TECH_CHAT_ID` (private debug supergroup for manual previews and refusal notices)
- `GEMINI_API_KEY`
- `GROQ_API_KEY`

The defaults are Gemini `gemini-3.8-flash` and Groq `openai/gpt-oss-120b`; optional Actions variables `GEMINI_MODEL` and `GROQ_MODEL` can override them. A normal daily run makes one Groq request per project plus one overview request and one Gemini audit request. Supported Groq models use strict JSON Schema output with low reasoning effort for GPT-OSS; malformed project content may trigger one schema-repair request. Other models use JSON object mode and may retry once without it. The worst-case budget reserves 40 Groq calls plus one Gemini audit call. Calls are paced to stay within documented free-tier throughput. Hard caps are 40 Groq calls per run and 100 total calls per UTC day, persisted in `bot-state`. There is no paid fallback and no local inference. Set the Actions variable `LLM_FREE_TIER_CONFIRMED=yes` only after confirming the API projects are on free tiers and billing is not enabled. This is an operator assertion; the APIs do not provide a universal guarantee that a key cannot incur charges. Groq is required for project descriptions and overview text; Gemini is required for the publication audit.

The Groq free-plan table currently lists 1,000 requests/day for `openai/gpt-oss-120b`; a normal daily run uses up to eleven Groq calls, with at most one JSON-mode fallback per project. Provider limits and model availability can change, and Gemini quotas are project-specific: check the active limits in Google AI Studio before enabling the secrets.

Every refusal on the scheduled path (no fresh candidates after deduplication, incomplete descriptions, credential scan finding, audit rejection) is reported to the technical chat, so a silent main chat always has a written reason. Telegram does not provide an idempotency key, so a retry after an ambiguous network result can still duplicate the one message whose send succeeded just before a runner stopped.

The workflow needs repository `contents: write` permission to update `bot-state`; updates are serialized. It also needs `actions: read` to download artifacts for the retry input.

## Hosted validation

The repository template retains CI, action security checks, Dependabot, and its Rust toolchain configuration. CI checks formatting, compilation, Clippy, dependency advisories, dependency policy, and unused dependencies. To validate delivery without sending a Telegram message, run `Run Bot` manually with the default dry-run setting and inspect its uploaded artifact.
