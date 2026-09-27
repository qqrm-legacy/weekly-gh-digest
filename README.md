# Weekly GitHub Digest

Rust Telegram bot derived from [`qqrm/template-tg-bot-rs`](https://github.com/qqrm/template-tg-bot-rs). It runs on hosted GitHub Actions; there is no local or always-on bot process.

## What the workflow does

- Runs every day at 07:17 UTC and captures metrics from the first pages of GitHub Trending (daily and weekly) and GitHub Search for repositories created in the previous seven days. The weekly issue analyzes exactly the ten highest `stars this week` rows on the captured weekly Trending page; it does not mix in daily Trending or new-repository search results.
- On Mondays, prepares a Russian digest. With delivery enabled, Actions sends it through Telegram Bot API. Manual `workflow_dispatch` defaults to `dry_run=true` and uploads a preview artifact.
- Stores source responses, manifests, and star history on the `bot-state` branch. Each run also uploads a 90-day artifact with the digest preview and provenance.

Three measurements stay separate: observed star change for tracked repositories with a baseline; GitHub Trending's displayed weekly-star count; and new repositories sorted by total stars. It does not invent a weekly baseline when no history exists.

## Evidence-based project analysis

Each of the ten projects gets two independent model reviews (Gemini and Groq), each asked to check ten technical and editorial dimensions: what it does, intended user, use case, setup friction, architecture, license/dependencies when documented, demonstrated behavior versus marketing claims, tradeoffs, unknowns, and a technically grounded joke. Gemini then synthesizes each pair into a concise card and writes a cross-project editorial summary. Every card must contain an exact excerpt from that repository's description or README; the program rejects missing or inexact quotes. Criticism and jokes must be grounded in the supplied evidence. Stars are presented as attention, never as a proxy for quality.

If either provider is unavailable or any evidence check fails, the preview marks the analysis incomplete and falls back to factual GitHub descriptions; it does not present partial model output as a reviewed card. A `digest` run refuses Telegram delivery unless all ten cards and the final editorial synthesis are complete. It never falls back to filler like “open the README and decide.”

## Configure GitHub Actions

Add these repository Actions secrets:

- `TELEGRAM_BOT_TOKEN`
- `TELEGRAM_CHAT_ID`
- `GEMINI_API_KEY`
- `GROQ_API_KEY`

The defaults are Gemini `gemini-3.8-flash` and Groq `openai/gpt-oss-120b`; optional Actions variables `GEMINI_MODEL` and `GROQ_MODEL` can override them. A full weekly run makes up to 31 calls: one Gemini review and one Groq review per project, one Gemini synthesis per project, and one final Gemini editorial synthesis. Calls are paced to stay within documented free-tier throughput. Hard caps are 40 LLM calls per run and 100 calls per UTC day, persisted in `bot-state`; a run refuses to begin if the daily budget cannot cover all 31 requests. There is no paid fallback and no local inference. Set the Actions variable `LLM_FREE_TIER_CONFIRMED=yes` only after confirming both API projects are on free tiers and billing is not enabled. This is an operator assertion; the APIs do not provide a universal guarantee that a key cannot incur charges. Without both API keys and this variable, the bot makes no model requests.

The Groq free-plan table currently lists 1,000 requests/day for `openai/gpt-oss-120b`; one normal weekly run uses at most ten Groq calls. Provider limits and model availability can change, and Gemini quotas are project-specific: check the active limits in Google AI Studio before enabling the secrets.

On Mondays, scheduled runs send automatically when both `TELEGRAM_BOT_TOKEN` and `TELEGRAM_CHAT_ID` secrets are configured; otherwise they create a preview and do not send. No separate delivery variable is needed. A manual run with `dry_run=false` sends immediately, so use the default dry run for review.

The workflow needs repository `contents: write` permission to update `bot-state`. It serializes updates to that branch. Telegram does not provide an idempotency key for `sendMessage`, so a retry after an ambiguous network result can duplicate a message.

## Hosted validation

The repository template retains CI, action security checks, Dependabot, and its Rust toolchain configuration. CI checks formatting, compilation, Clippy, dependency advisories, dependency policy, and unused dependencies. To validate delivery without sending a Telegram message, run `Run Bot` manually with the default dry-run setting and inspect its uploaded artifact.
