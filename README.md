# Weekly GitHub Digest

Rust Telegram bot derived from [`qqrm/template-tg-bot-rs`](https://github.com/qqrm/template-tg-bot-rs). It runs on hosted GitHub Actions; there is no local or always-on bot process.

## What the workflow does

- Runs every day at 07:17 UTC and captures metrics from the first pages of GitHub Trending (daily and weekly) and GitHub Search for repositories created in the previous seven days. The weekly issue analyzes exactly the ten highest weekly Trending star-count (`+N`) rows on the captured page; total repository stars are displayed separately and do not determine order. It does not mix in daily Trending or new-repository search results.
- On Mondays, prepares ten separate Russian project posts in the private technical Telegram group, then posts and pins an overview linked to those Telegram messages. It runs a deterministic credential scan and a Gemini publication audit; the verdict and source post IDs are stored with the run artifact. This debug stage never posts to the curated channel.
- A separate, manually triggered `Release Digest` workflow accepts the successful debug run ID. It requires the stored audit to pass, copies the exact reviewed project messages from the technical group into the curated channel, then posts and pins a linked overview. Release delivery is idempotent across normal workflow retries.
- Stores source responses, manifests, and star history on the `bot-state` branch. Each run also uploads a 90-day artifact with the digest preview and provenance.

Three measurements stay separate: observed star change for tracked repositories with a baseline; GitHub Trending's displayed weekly-star count; and new repositories sorted by total stars. It does not invent a weekly baseline when no history exists.

## Project descriptions and deterministic format

Groq generates one field per project: a compact Russian technical analysis grounded in GitHub metadata and the README excerpt. Rust deterministically supplies the card title and link, rank, star counts, language, labels, MarkdownV2 escaping, project order, and overview. Gemini does not write cards, jokes, or the overview; it is used only for the separate publication audit. The exact source excerpt stays in the run artifact for review. A failed or incomplete analysis blocks delivery instead of publishing partial model output. The run artifact contains `posts/01.md` through `posts/10.md` as individual message previews. After debug delivery, `overview.md` contains the actual Telegram post links.

## Configure GitHub Actions

Add these repository Actions secrets:

- `TELEGRAM_BOT_TOKEN`
- `TELEGRAM_CHAT_ID`
- `TELEGRAM_TECH_CHAT_ID` (private debug group)
- `GEMINI_API_KEY`
- `GROQ_API_KEY`

The defaults are Gemini `gemini-3.8-flash` and Groq `openai/gpt-oss-120b`; optional Actions variables `GEMINI_MODEL` and `GROQ_MODEL` can override them. A full weekly debug run normally makes ten Groq description requests and one Gemini publication-audit request. Each Groq request may fall back once without forced JSON mode; malformed description JSON may trigger one schema-repair request. The worst-case budget reserves 40 Groq calls plus one Gemini audit call. Calls are paced to stay within documented free-tier throughput. Hard caps are 40 Groq calls per run and 100 total calls per UTC day, persisted in `bot-state`; release is held if the review cannot finish. There is no paid fallback and no local inference. Set the Actions variable `LLM_FREE_TIER_CONFIRMED=yes` only after confirming the API projects are on free tiers and billing is not enabled. This is an operator assertion; the APIs do not provide a universal guarantee that a key cannot incur charges. Groq is required to generate project descriptions; Gemini is required for the separate publication audit.

The Groq free-plan table currently lists 1,000 requests/day for `openai/gpt-oss-120b`; a normal weekly run uses ten Groq calls, with at most one JSON-mode fallback per project. Provider limits and model availability can change, and Gemini quotas are project-specific: check the active limits in Google AI Studio before enabling the secrets.

On Mondays, `Run Bot` posts to the private technical group and records its audit result. To publish, inspect the technical posts and audit message, then run `Release Digest` manually with the successful `Run Bot` run ID. The release workflow refuses runs from other workflows, unsuccessful runs, branches other than `main`, missing audit artifacts, failed credential scans, and non-approved AI audits. Manual `Run Bot` runs default to `dry_run=true`; set it to `false` to create a live debug candidate. The scheduled pipeline never publishes to the curated channel by itself.

The workflows need `contents: write` permission to update `bot-state`; the release workflow also needs `actions: read` to download the selected debug artifact. They serialize updates to that branch. Telegram does not provide an idempotency key, so a retry after an ambiguous network result can still duplicate the one message whose send succeeded just before a runner stopped.

## Hosted validation

The repository template retains CI, action security checks, Dependabot, and its Rust toolchain configuration. CI checks formatting, compilation, Clippy, dependency advisories, dependency policy, and unused dependencies. To validate delivery without sending a Telegram message, run `Run Bot` manually with the default dry-run setting and inspect its uploaded artifact.
