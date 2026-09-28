# Weekly GitHub Digest

Rust Telegram bot derived from [`qqrm/template-tg-bot-rs`](https://github.com/qqrm/template-tg-bot-rs). It runs on hosted GitHub Actions; there is no local or always-on bot process.

## What the workflow does

- Runs every day at 07:17 UTC and captures metrics from the first pages of GitHub Trending (daily and weekly) and GitHub Search for repositories created in the previous seven days. The weekly issue analyzes exactly the ten highest weekly Trending star-count (`+N`) rows on the captured page; total repository stars are displayed separately and do not determine order. It does not mix in daily Trending or new-repository search results.
- On Mondays, prepares a Russian digest and posts the candidate to a private technical Telegram group. It then runs a deterministic credential scan and a Gemini publication audit; the verdict and source post IDs are stored with the run artifact. This debug stage never posts to the curated channel.
- A separate, manually triggered `Release Digest` workflow accepts the successful debug run ID. It requires the stored audit to pass, copies the exact reviewed project messages from the technical group into the curated channel, then posts and pins a linked overview. Release delivery is idempotent across normal workflow retries.
- Stores source responses, manifests, and star history on the `bot-state` branch. Each run also uploads a 90-day artifact with the digest preview and provenance.

Three measurements stay separate: observed star change for tracked repositories with a baseline; GitHub Trending's displayed weekly-star count; and new repositories sorted by total stars. It does not invent a weekly baseline when no history exists.

## Evidence-based project analysis

Each of the ten projects gets a Groq technical review of its purpose, user, use case, setup, architecture, documented dependencies, demonstrated behavior, tradeoffs, unknowns, and a technically grounded joke. Gemini writes the cross-project editorial summary. Every card must contain an exact excerpt from that repository's description or README; the program rejects missing or inexact quotes. Criticism and jokes must be grounded in the supplied evidence. Stars are presented as attention, never as a proxy for quality.

If either provider is unavailable or any evidence check fails, the preview marks the analysis incomplete and falls back to factual GitHub descriptions; it does not present partial model output as a reviewed card. A `digest` run refuses Telegram delivery unless all ten cards and the final editorial synthesis are complete. It never falls back to filler like “open the README and decide.”

## Configure GitHub Actions

Add these repository Actions secrets:

- `TELEGRAM_BOT_TOKEN`
- `TELEGRAM_CHAT_ID`
- `TELEGRAM_TECH_CHAT_ID` (private debug group)
- `GEMINI_API_KEY`
- `GROQ_API_KEY`

The defaults are Gemini `gemini-3.8-flash` and Groq `openai/gpt-oss-120b`; optional Actions variables `GEMINI_MODEL` and `GROQ_MODEL` can override them. A full weekly debug run normally makes 12 requests: ten Groq project reviews, one Gemini editorial summary, and one Gemini release audit. Groq gets one fallback without forced JSON mode if its JSON validator rejects a response; Gemini gets one strict-format retry if editorial schema validation fails. The safety budget reserves up to 23 requests for those retries (and audit), while a normal run uses 12. Calls are paced to stay within documented free-tier throughput. Hard caps are 40 LLM calls per run and 100 calls per UTC day, persisted in `bot-state`; release is held if the review cannot finish. There is no paid fallback and no local inference. Set the Actions variable `LLM_FREE_TIER_CONFIRMED=yes` only after confirming both API projects are on free tiers and billing is not enabled. This is an operator assertion; the APIs do not provide a universal guarantee that a key cannot incur charges. Without both API keys and this variable, the bot makes no model requests.

The Groq free-plan table currently lists 1,000 requests/day for `openai/gpt-oss-120b`; a normal weekly run uses ten Groq calls, with at most one JSON-mode fallback per project. Provider limits and model availability can change, and Gemini quotas are project-specific: check the active limits in Google AI Studio before enabling the secrets.

On Mondays, `Run Bot` posts to the private technical group and records its audit result. To publish, inspect the technical posts and audit message, then run `Release Digest` manually with the successful `Run Bot` run ID. The release workflow refuses runs from other workflows, unsuccessful runs, branches other than `main`, missing audit artifacts, failed credential scans, and non-approved AI audits. Manual `Run Bot` runs default to `dry_run=true`; set it to `false` to create a live debug candidate. The scheduled pipeline never publishes to the curated channel by itself.

The workflows need `contents: write` permission to update `bot-state`; the release workflow also needs `actions: read` to download the selected debug artifact. They serialize updates to that branch. Telegram does not provide an idempotency key, so a retry after an ambiguous network result can still duplicate the one message whose send succeeded just before a runner stopped.

## Hosted validation

The repository template retains CI, action security checks, Dependabot, and its Rust toolchain configuration. CI checks formatting, compilation, Clippy, dependency advisories, dependency policy, and unused dependencies. To validate delivery without sending a Telegram message, run `Run Bot` manually with the default dry-run setting and inspect its uploaded artifact.
