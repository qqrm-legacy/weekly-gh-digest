"""Choose collection/preview/publication without trusting implicit dispatch defaults."""
import os
import json
from datetime import datetime, timezone
from pathlib import Path

import weekly_run

COLLECTION_CRON = "17 */3 * * *"
PUBLICATION_CRON = "0 7 * * 1"


def choose(env):
    enabled = env.get("DIGEST_DELIVERY_ENABLED") == "true"
    if env.get("EVENT_NAME") == "schedule":
        schedule = env.get("EVENT_SCHEDULE")
        if schedule == COLLECTION_CRON:
            return "collect", False, False
        if schedule != PUBLICATION_CRON:
            raise ValueError("Unknown schedule; refusing publication")
        return ("digest", True, True) if enabled else ("preview", False, False)
    if env.get("EVENT_NAME") != "workflow_dispatch":
        raise ValueError("Unsupported event")
    retry = env.get("RETRY_DELIVERY_RUN_ID", "")
    preview_source = env.get("PREVIEW_SOURCE_RUN_ID", "")
    if env.get("RECOVER_WEEKLY") == "true":
        if (retry or preview_source or env.get("DELIVER_TO_MAIN_CHAT") == "true"
                or env.get("MANUAL_DRY_RUN") == "false"):
            raise ValueError("Weekly recovery cannot be combined with sending or stored-preview inputs")
        return "preview", False, env.get("PREVIEW_WITH_LLM") == "true"
    if preview_source:
        if not preview_source.isascii() or not preview_source.isdigit():
            raise ValueError("preview_source_run_id must contain ASCII digits only")
        if retry or env.get("DELIVER_TO_MAIN_CHAT") == "true" or env.get("MANUAL_DRY_RUN") == "false":
            raise ValueError("Stored preview cannot be combined with sending inputs")
        return "preview-stored", False, True
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


def main():
    mode, send, llm = choose(os.environ)
    now = datetime.now(timezone.utc)
    key = weekly_run.slot(now)
    reason = "explicit-manual-request"
    if os.environ.get("EVENT_NAME") == "schedule" or os.environ.get("RECOVER_WEEKLY") == "true":
        (mode, send, llm), reason = weekly_run.recovery_mode(
            os.environ, now, weekly_run.read_object(weekly_run.RECEIPTS, missing_ok=True),
            weekly_run.read_object("data/state.json"))
    plan = {"slot": key, "mode": mode, "send": send, "llm": llm, "reason": reason,
            "event": os.environ.get("EVENT_NAME"), "cron": os.environ.get("EVENT_SCHEDULE"),
            "delivery_enabled": os.environ.get("DIGEST_DELIVERY_ENABLED") == "true",
            "issue_due_at": weekly_run.issue_start(now).isoformat(),
            "selected_at": now.isoformat(),
            "issue_age_seconds": int((now - weekly_run.issue_start(now)).total_seconds())}
    Path(os.environ["PLAN_FILE"]).write_text(json.dumps(plan, indent=2) + "\n")
    print(json.dumps(plan))
    with open(os.environ["GITHUB_OUTPUT"], "a", encoding="utf-8") as output:
        output.write(f"mode={mode}\nsend={str(send).lower()}\nllm={str(llm).lower()}\n")
        output.write(f"slot={key}\n")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as summary:
            summary.write(f"### Weekly recovery\n\n{key}: `{mode}` ({reason}); "
                          f"Telegram enabled for this run: `{str(send).lower()}`.\n\n")


if __name__ == "__main__":
    main()
