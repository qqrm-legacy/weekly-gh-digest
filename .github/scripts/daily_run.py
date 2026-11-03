"""Daily completion receipts. Preview completion is never a delivery ACK."""
from datetime import timedelta, timezone
import json
import os
from pathlib import Path
import sys

import weekly_run

RECEIPTS = Path("data/daily-runs.json")


def issue_start(now):
    now = now.astimezone(timezone.utc)
    boundary = now.replace(hour=15, minute=0, second=0, microsecond=0)
    return boundary if now >= boundary else boundary - timedelta(days=1)


def slot(now):
    return "daily:" + issue_start(now).date().isoformat()


def delivery_enabled(env):
    return (env.get("DIGEST_DELIVERY_ENABLED") == "true"
            and env.get("DIGEST_DAILY_DELIVERY_ENABLED") == "true")


def recovery_mode(env, now, receipts, state):
    # Never turn morning collection into an early daily publication.
    if now.astimezone(timezone.utc).hour < 15:
        return ("collect", False, False), "daily-before-evening-deadline"
    key = slot(now)
    sent = state.get("sent_slots", {})
    if not isinstance(sent, dict):
        raise ValueError("Invalid authoritative sent_slots")
    if key in sent:
        return ("collect", False, False), "daily-already-delivered"
    receipt = weekly_run.receipt_map(receipts).get(key)
    if receipt is not None:
        if (not isinstance(receipt, dict) or receipt.get("slot") != key
                or receipt.get("mode") not in ("daily-preview", "daily-digest")
                or not str(receipt.get("artifact_id", "")).isascii()
                or not str(receipt.get("artifact_id", "")).isdigit()):
            raise ValueError("Invalid daily artifact receipt")
        if not delivery_enabled(env):
            return ("collect", False, False), "daily-preview-artifact-already-saved"
    if delivery_enabled(env):
        return ("daily-digest", True, True), "daily-delivery-due"
    return ("daily-preview", False, env.get("PREVIEW_WITH_LLM") == "true"), "daily-preview-due"


def validate_preview(directory, expected_slot):
    digest = weekly_run.validate_preview(directory, expected_slot, slot_resolver=slot)
    if digest.get("edition") != "daily" or len(digest["entries"]) > 12:
        raise ValueError("Invalid daily edition")
    if digest.get("catalogue"):
        raise ValueError("Daily preview cannot contain a weekly index")
    queue = weekly_run.read_object(Path(directory) / "queue.json")
    if queue.get("edition") != "daily":
        raise ValueError("Invalid daily queue artifact")
    return digest


def record(directory, expected_slot, mode, run_id, artifact_id, receipts_path=RECEIPTS):
    if mode not in ("daily-preview", "daily-digest"):
        raise ValueError("Not a daily completion")
    for identifier in (run_id, artifact_id):
        if not identifier or not identifier.isascii() or not identifier.isdigit():
            raise ValueError("Completion requires actual run and uploaded artifact IDs")
    digest = validate_preview(directory, expected_slot)
    receipts = weekly_run.read_object(receipts_path, missing_ok=True)
    issues = weekly_run.receipt_map(receipts)
    issues[expected_slot] = {"slot": expected_slot, "mode": mode, "run_id": run_id,
                            "artifact_id": artifact_id, "generated_at": digest["generated_at"],
                            "entry_count": len(digest["entries"])}
    destination = Path(receipts_path)
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_suffix(".tmp")
    temporary.write_text(json.dumps({"format_version": 1, "issues": issues}, indent=2) + "\n")
    temporary.replace(destination)


def main():
    directory = Path(os.environ["RUN_DIRECTORY"])
    expected_slot = os.environ["WEEKLY_SLOT"]
    if sys.argv[1] == "verify":
        validate_preview(directory, expected_slot)
        (directory / "schedule-recovery.json").write_text(Path(os.environ["PLAN_FILE"]).read_text())
    elif sys.argv[1] == "record":
        record(directory, expected_slot, os.environ["RUN_MODE"],
               os.environ["GITHUB_RUN_ID"], os.environ["ARTIFACT_ID"])
    else:
        raise ValueError("Unknown receipt operation")


if __name__ == "__main__":
    main()
