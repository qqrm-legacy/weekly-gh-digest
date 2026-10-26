"""Weekly recovery receipts, separate from the authoritative delivery checkpoint."""
from datetime import datetime, timedelta, timezone
import json
import os
from pathlib import Path
import sys

RECEIPTS = Path("data/weekly-runs.json")


def issue_start(now):
    now = now.astimezone(timezone.utc)
    monday = (now - timedelta(days=now.weekday())).replace(
        hour=7, minute=0, second=0, microsecond=0)
    return monday if now >= monday else monday - timedelta(days=7)


def slot(now):
    return "weekly:" + issue_start(now).date().isoformat()


def read_object(path, missing_ok=False):
    try:
        value = json.loads(Path(path).read_text(encoding="utf-8"))
    except FileNotFoundError:
        if missing_ok:
            return {}
        raise
    if not isinstance(value, dict):
        raise ValueError(f"Expected an object in {path}")
    return value


def receipt_map(document):
    if not document:
        return {}
    if document.get("format_version") != 1 or not isinstance(document.get("issues"), dict):
        raise ValueError("Invalid weekly completion receipts")
    return document["issues"]


def recovery_mode(env, now, receipts, state):
    """Called only for recognized schedules or explicit safe recovery dispatches."""
    key = slot(now)
    sent = state.get("sent_slots", {})
    if not isinstance(sent, dict):
        raise ValueError("Invalid authoritative sent_slots")
    if key in sent:
        return ("collect", False, False), "already-delivered"
    enabled = env.get("DIGEST_DELIVERY_ENABLED") == "true"
    safe_recovery = env.get("RECOVER_WEEKLY") == "true"
    receipt = receipt_map(receipts).get(key)
    if receipt is not None:
        if (not isinstance(receipt, dict)
                or receipt.get("slot") != key
                or receipt.get("mode") not in ("preview", "digest")
                or not str(receipt.get("artifact_id", "")).isascii()
                or not str(receipt.get("artifact_id", "")).isdigit()):
            raise ValueError("Invalid weekly artifact receipt")
        # A preview receipt never masquerades as a delivered issue.
        if safe_recovery or not enabled:
            return ("collect", False, False), "preview-artifact-already-saved"
    if enabled and not safe_recovery:
        return ("digest", True, True), "weekly-delivery-due"
    return ("preview", False, env.get("PREVIEW_WITH_LLM") == "true"), "weekly-preview-due"


def validate_preview(directory, expected_slot):
    directory = Path(directory)
    for name in ("summary.md", "digest.md", "selection.json", "coverage.json",
                 "weekly-window.json", "weekly-source-manifest.json", "manifest.json"):
        if not (directory / name).is_file() or not (directory / name).stat().st_size:
            raise ValueError(f"Missing weekly artifact file: {name}")
    digest = read_object(directory / "ranking-input.json")
    generated = datetime.fromisoformat(digest["generated_at"].replace("Z", "+00:00"))
    if (digest.get("format_version") != 3 or digest.get("delivery_slot") != expected_slot
            or slot(generated) != expected_slot):
        raise ValueError("Preview belongs to a different weekly slot")
    entries = digest.get("entries")
    if not isinstance(entries, list) or len(entries) > 30:
        raise ValueError("Invalid weekly selection")
    if entries:
        audit = read_object(directory / "release-audit.json")
        messages = json.loads((directory / "telegram-messages.json").read_text())
        if (audit.get("approved") is not True
                or audit.get("deterministic_scan") != "passed"
                or audit.get("telegram_format", {}).get("status") != "passed"):
            raise ValueError("Weekly preview did not pass its local publication checks")
        if not isinstance(messages, list) or not messages or any(
                not isinstance(message, str) or not message.strip()
                or len(message.encode("utf-16-le")) // 2 > 3800 for message in messages):
            raise ValueError("Invalid Telegram preview payload")
        lengths = [len(message.encode("utf-16-le")) // 2 for message in messages]
        if audit["telegram_format"].get("message_utf16_units") != lengths:
            raise ValueError("Audit does not describe the saved messages")
        if not (directory / "issue.md").is_file():
            raise ValueError("Missing Telegram-formatted preview")
    else:
        sources = read_object(directory / "manifest.json").get("sources", [])
        if not sources or not any(source.get("status") == "included" for source in sources):
            raise ValueError("Failed collection cannot complete an empty weekly issue")
    return digest


def record(directory, expected_slot, mode, run_id, artifact_id, receipts_path=RECEIPTS):
    if mode not in ("preview", "digest"):
        raise ValueError("Only weekly previews and digests can complete a slot")
    for identifier in (run_id, artifact_id):
        if not identifier or not identifier.isascii() or not identifier.isdigit():
            raise ValueError("Completion requires a real run and uploaded artifact ID")
    digest = validate_preview(directory, expected_slot)
    receipts = read_object(receipts_path, missing_ok=True)
    issues = receipt_map(receipts)
    issues[expected_slot] = {
        "slot": expected_slot, "mode": mode, "run_id": run_id,
        "artifact_id": artifact_id, "generated_at": digest["generated_at"],
        "entry_count": len(digest["entries"]),
    }
    destination = Path(receipts_path)
    destination.parent.mkdir(parents=True, exist_ok=True)
    temporary = destination.with_suffix(".tmp")
    temporary.write_text(json.dumps({"format_version": 1, "issues": issues}, indent=2) + "\n")
    temporary.replace(destination)


def main():
    operation = sys.argv[1]
    directory = Path(os.environ["RUN_DIRECTORY"])
    expected_slot = os.environ["WEEKLY_SLOT"]
    if operation == "verify":
        validate_preview(directory, expected_slot)
        (directory / "schedule-recovery.json").write_text(
            Path(os.environ["PLAN_FILE"]).read_text(), encoding="utf-8")
    elif operation == "record":
        record(directory, expected_slot, os.environ["RUN_MODE"],
               os.environ["GITHUB_RUN_ID"], os.environ["ARTIFACT_ID"])
    else:
        raise ValueError("Unknown weekly receipt operation")


if __name__ == "__main__":
    main()
