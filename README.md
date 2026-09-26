# Weekly GitHub Digest

Rust Telegram bot derived from [`qqrm/template-tg-bot-rs`](https://github.com/qqrm/template-tg-bot-rs). It runs on hosted GitHub Actions; there is no local or always-on bot process.

## What the workflow does

- Runs every day at 07:17 UTC and captures candidate metrics from the first pages of GitHub Trending (daily and weekly) and GitHub Search for repositories created in the previous seven days. This is a bounded candidate sample, not a global GitHub ranking.
- On Mondays, prepares a Russian digest. With delivery enabled, Actions sends it through Telegram Bot API. Manual `workflow_dispatch` defaults to `dry_run=true` and uploads a preview artifact.
- Stores source responses, manifests, and star history on the `bot-state` branch. Each run also uploads a 90-day artifact with the digest preview and provenance.

Three measurements stay separate: observed star change for tracked repositories with a baseline; GitHub Trending's displayed weekly-star count; and new repositories sorted by total stars. It does not invent a weekly baseline when no history exists.

## Evidence-based project analysis

The model reviews up to ten candidates in two analysis calls (five projects per call), then a synthesizer compares the analyzed projects. An analysis card must include a verbatim excerpt from that repository's GitHub description or README; the program rejects cards when the excerpt is missing or does not match the supplied source. The card states what the project does, a concrete use case, and any evidenced design tradeoff. The synthesizer may add a short development joke. It may not invent repository facts or explain star growth without evidence.

If analysis is unavailable, the digest uses the original GitHub description and clearly says the model did not produce a verified summary. It never falls back to filler like “open the README and decide.”

## Configure GitHub Actions

Add these repository Actions secrets:

- `TELEGRAM_BOT_TOKEN`
- `TELEGRAM_CHAT_ID`
- `CLOUDFLARE_ACCOUNT_ID`
- `CLOUDFLARE_API_TOKEN`

The inference provider is Cloudflare Workers AI `@cf/openai/gpt-oss-120b`. The workflow only calls it when the repository Actions variable `CLOUDFLARE_FREE_TIER_CONFIRMED` is set to `yes`, all Cloudflare credentials are non-empty, and the preflight estimate stays under 4,000 Neurons for the complete run. There is no paid fallback. The confirmation variable is an operator assertion: the API does not certify that the account cannot bill beyond its free allowance. Without the variable, the bot makes no model requests.

Scheduled Telegram delivery is gated separately. Set `TELEGRAM_DELIVERY_ENABLED=true` only after adding the Telegram secrets and verifying the chat. Until then Monday runs create previews and do not send. A manual run with `dry_run=false` sends immediately, so use the default dry run for review.

The workflow needs repository `contents: write` permission to update `bot-state`. It serializes updates to that branch. Telegram does not provide an idempotency key for `sendMessage`, so a retry after an ambiguous network result can duplicate a message.

## Hosted validation

The repository template retains CI, action security checks, Dependabot, and its Rust toolchain configuration. CI checks formatting, compilation, Clippy, dependency advisories, dependency policy, and unused dependencies. To validate delivery without sending a Telegram message, run `Run Bot` manually with the default dry-run setting and inspect its uploaded artifact.
