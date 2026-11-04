# Bounded technical-chat trial: 31 October–7 November 2026

The owner authorized live test delivery, not production publication or merging PR #20.
The trial runs only on `feat/retained-daily-weekly-digests-2026-10-31` and refuses
sends at or after `2026-11-08T00:00:00Z`. Both publication variables remain unchanged
in repository settings. The test process uses the existing technical-chat secret;
preflight resolves technical and production targets and rejects identical IDs.

## State and acknowledgement

Only `digest-soak-state-2026-10-31` is written by the trial. Production observations
are read from `delivery-state`, never acknowledged or overwritten there. Repeated
daily/weekly commands collect but neither regenerate nor resend a completed slot.
Confirmed chunks survive runner restarts; an ambiguous send or editorial edit leaves
a durable intent requiring operator reconciliation. Never clear that marker to retry.

Every successful `sendMessage` response is captured with server message ID, accepted
text/entities and reply ID in `telegram-receipts.jsonl`, without bot credentials or
chat metadata. Historical reviewed edits retain their acknowledgements in
`reviewed-telegram-receipt.json`; the original model text remains in older artifacts.
The one-off summary generation and editing commands are retired without sending.
Current trials exercise card delivery and restart behavior only.
This proves API acceptance, not client-side layout, read receipts or later chat edits.

## Evidence and limits

`seven-day-replay.json` is a simulation on retained historical observations. It tests
seven daily acknowledgements followed by the weekly union, not seven days of live
execution. `report.json` contains the actual day's new posts, independent queue
checks and duplicate-run assertions. `editorial-followup.json` and reviewed receipts
are separate from repository publication receipts and never consume discoveries.

The initial live test exposed rate limiting, missing synthesized overviews, invalid
source quotations and overbroad model claims. Provider backoff respects bounded
Retry-After. Current issues no longer generate or require an overview; daily AI
delivery instead requires validated Russian cards. Policy-versioned analysis
caches prevent old editorial text from being reused after a policy change.
Raw-source fallbacks stay queued for daily delivery. These are safeguards, not a general factual-correctness
proof: automated editorial quality is not accepted as equivalent to the owner's
TWIR bot merely because transport and tests pass. Review semantic claims and
translation terminology against source bundles during the week.

The bot is a member of the technical supergroup; initial posting succeeded but
Telegram rejected pinning. Pinning is cosmetic and occurs only after durable
completion. Do not grant new permissions automatically.

The owner-requested follow-up task checks morning/evening through 7 November,
uses only the verified trial workflow and asks for feedback on selected evenings.
No scheduled trial is added to the production branch. Stop or pause the follow-up
task to end the experiment early; the hard deadline independently rejects sends.
