"""Automate daily editions only; weekly commands require an explicit manual request."""
import os
import json
from datetime import datetime, timezone
from pathlib import Path

import daily_run
import weekly_run

SPLIT_WORKFLOW_CONTRACT = 1
COLLECTION_CRON = "17 */2 * * *"
LEGACY_COLLECTION_CRON = "17 */3 * * *"
PUBLICATION_CRON = "0 7 * * 1"  # Retired; delayed events are collection-only.
DAILY_CRON = "0 15 * * *"


def choose(env):
    operation = env.get("RUN_OPERATION", "")
    if operation:
        event = env.get("EVENT_NAME")
        if operation not in ("collect", "preview", "publish"):
            raise ValueError("Unknown workflow operation")
        if event == "schedule":
            expected = {"collect": COLLECTION_CRON, "publish": DAILY_CRON}.get(operation)
            if expected is None or env.get("EVENT_SCHEDULE") != expected:
                raise ValueError("Schedule does not match workflow operation")
        elif event != "workflow_dispatch":
            raise ValueError("Unsupported workflow operation event")
        if operation == "collect":
            return "collect", False, False
        if operation == "preview":
            return "daily-offline-preview", False, False
        return ("daily-digest", True, True) if daily_run.delivery_enabled(env) else ("daily-preview", False, False)
    enabled = env.get("DIGEST_DELIVERY_ENABLED") == "true"
    edition = env.get("DIGEST_FORMAT", "") or "weekly"
    if edition not in ("daily", "weekly"):
        raise ValueError("Unknown digest format")
    if env.get("EVENT_NAME") == "schedule":
        schedule = env.get("EVENT_SCHEDULE")
        if schedule in (COLLECTION_CRON, LEGACY_COLLECTION_CRON):
            return "collect", False, False
        if schedule == DAILY_CRON:
            return ("daily-digest", True, True) if daily_run.delivery_enabled(env) else ("daily-preview", False, False)
        if schedule != PUBLICATION_CRON:
            raise ValueError("Unknown schedule; refusing publication")
        # An old queued Monday event must not re-enable weekly publication.
        return "collect", False, False
    if env.get("EVENT_NAME") != "workflow_dispatch":
        raise ValueError("Unsupported event")
    retry = env.get("RETRY_DELIVERY_RUN_ID", "")
    preview_source = env.get("PREVIEW_SOURCE_RUN_ID", "")
    if preview_source:
        if not preview_source.isascii() or not preview_source.isdigit():
            raise ValueError("preview_source_run_id must contain ASCII digits only")
        if (retry or env.get("DELIVER_TO_MAIN_CHAT") == "true"
                or env.get("MANUAL_DRY_RUN") == "false" or env.get("RECOVER_WEEKLY") == "true"):
            raise ValueError("Stored preview cannot be combined with sending or recovery inputs")
        # Re-render either edition from its saved cards. Do not inject a provider
        # key, even if preview_with_llm was also selected in the dispatch form.
        return "preview-stored", False, False
    if edition == "daily":
        if retry or env.get("RECOVER_WEEKLY") == "true":
            raise ValueError("Daily mode cannot use legacy weekly replay/recovery inputs")
        if env.get("MANUAL_DRY_RUN") == "false" and env.get("DELIVER_TO_MAIN_CHAT") != "true":
            raise ValueError("Daily delivery requires explicit main-chat intent")
        if env.get("DELIVER_TO_MAIN_CHAT") == "true":
            if not daily_run.delivery_enabled(env):
                raise ValueError("Daily publication disabled pending preview review")
            return "daily-digest", True, True
        return "daily-preview", False, env.get("PREVIEW_WITH_LLM") == "true"
    if env.get("RECOVER_WEEKLY") == "true":
        if (retry or preview_source or env.get("DELIVER_TO_MAIN_CHAT") == "true"
                or env.get("MANUAL_DRY_RUN") == "false"):
            raise ValueError("Weekly recovery cannot be combined with sending or stored-preview inputs")
        return "preview", False, env.get("PREVIEW_WITH_LLM") == "true"
    if retry:
        if not retry.isascii() or not retry.isdigit():
            raise ValueError("retry_delivery_run_id must contain ASCII digits only")
        mode = "retry"
    elif env.get("DELIVER_TO_MAIN_CHAT") == "true":
        mode = "digest"
    elif env.get("MANUAL_DRY_RUN") == "false":
        mode = "debug"
    else:
        return "preview", False, env.get("PREVIEW_WITH_LLM") == "true"
    if not enabled:
        raise ValueError("Publication disabled pending preview review")
    return mode, True, mode != "retry"


def select_plan(env, now, weekly_receipts, daily_receipts, state):
    chosen = choose(env)
    operation = env.get("RUN_OPERATION", "")
    if operation == "collect":
        return chosen, "daily-collection-only"
    if operation == "preview":
        return chosen, "daily-offline-preview"
    if operation == "publish":
        return daily_run.recovery_mode(env, now, daily_receipts, state)
    if env.get("EVENT_NAME") == "schedule":
        if env.get("EVENT_SCHEDULE") == DAILY_CRON:
            return daily_run.recovery_mode(env, now, daily_receipts, state)
        return chosen, "daily-collection-only"
    if env.get("RECOVER_WEEKLY") == "true":
        return weekly_run.recovery_mode(env, now, weekly_receipts, state)
    return chosen, "explicit-manual-request"


def main():
    choose(os.environ)
    now = datetime.now(timezone.utc)
    (mode, send, llm), reason = select_plan(
        os.environ, now,
        weekly_run.read_object(weekly_run.RECEIPTS, missing_ok=True),
        weekly_run.read_object(daily_run.RECEIPTS, missing_ok=True),
        weekly_run.read_object("data/state.json"))
    daily = (mode.startswith("daily-") or reason.startswith("daily-")
             or (mode == "preview-stored" and os.environ.get("DIGEST_FORMAT") == "daily"))
    resolver = daily_run if daily else weekly_run
    key = resolver.slot(now)
    plan = {"slot": key, "mode": mode, "send": send, "llm": llm, "reason": reason,
            "event": os.environ.get("EVENT_NAME"), "cron": os.environ.get("EVENT_SCHEDULE"),
            "edition": "daily" if daily else "weekly",
            "delivery_enabled": os.environ.get("DIGEST_DELIVERY_ENABLED") == "true",
            "daily_delivery_enabled": daily_run.delivery_enabled(os.environ),
            "issue_due_at": resolver.issue_start(now).isoformat(), "selected_at": now.isoformat(),
            "issue_age_seconds": int((now - resolver.issue_start(now)).total_seconds())}
    Path(os.environ["PLAN_FILE"]).write_text(json.dumps(plan, indent=2) + "\n")
    print(json.dumps(plan))
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.write(f"mode={mode}\nsend={str(send).lower()}\nllm={str(llm).lower()}\nslot={key}\n")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
            summary.write(f"### Edition recovery\n\n{key}: `{mode}` ({reason}); "
                          f"Telegram enabled for this run: `{str(send).lower()}`.\n\n")


if __name__ == "__main__":
    main()
