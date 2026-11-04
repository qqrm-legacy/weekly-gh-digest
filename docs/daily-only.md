# Daily-only automatic delivery (2026-11-03)

This operating policy supersedes the weekly automatic schedule described in older
README/rollout notes. The existing daily delivery gates remain enabled by the
operator; this change does not write variables, secrets or runtime state.

## Automatic schedule

- `17 */3 * * *`: collect observations; after 15:00 UTC recover the daily issue
  only when its slot is not acknowledged. Before the deadline and after a
  successful delivery, collect without Telegram or LLM calls.
- `0 15 * * *`: prepare the daily AI digest at 18:00 Moscow.

There is no Monday weekly publication trigger. Collection ticks never fall back
to weekly synthesis or delivery, even when the weekly slot is missing and the
global delivery gate is enabled. A delayed event using the retired Monday cron
is collection-only. Weekly Recovery has only a manual, preview-only trigger:
no timer and no push-triggered dispatch.

## What readers receive

The daily format contains up to 12 concise, validated Russian AI explanations.
Each card has a linked repository name, roughly three or four substantive sentences
(usually 350–550 characters, at most 650), and a compact line
with a language hashtag and available star metrics. Humor is optional; there is no
issue synopsis or separate final message. Source and capture details remain in
artifacts rather than publication text. A daily publication slot may contain several Telegram messages; it
is one issue, not one Telegram message. Existing provider caps, editorial checks,
retry handling and refusal to send raw description fallbacks remain unchanged.

The retired automatic weekly format offered up to 30 detailed cards plus an
index of other pending findings. Its independent acknowledgements allowed it
to recap projects already sent in daily issues; it was not a separate source
of discoveries. Manual weekly previews/delivery remain available only through
an explicit manual request and existing safety gates.

## Retention and compatibility

No observations, cached analyses, pending payloads or publication receipts are
removed or marked as delivered by this change. Daily overflow and rejected or
unprocessed explanations remain pending for later daily issues, even after
leaving Trending. Weekly Trending remains an input source: removing the weekly
publication does not remove its observations from daily selection.

Daily and legacy weekly ledgers remain separate and backward compatible. The
ordinary daily command resumes only daily pending payloads. Uncertain delivery
still requires reconciliation; disabling weekly automation does not clear it.
The daily queue includes older undelivered work, not just projects first seen
on the current date. Coverage remains limited to actually observed sources.

Tests exercise two simulated weeks of collection ticks, daily ACK suppression,
disabled daily permission, stale Monday events, manual recovery, source-history
preservation and removal of all automatic weekly triggers. This is a scheduling
regression test, not a claim of two weeks of live operation.
