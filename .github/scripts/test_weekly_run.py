"""Recover lost/delayed triggers without treating collection or preview as delivery."""
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from run_mode import COLLECTION_CRON, PUBLICATION_CRON, choose
import weekly_run


def instant(value):
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


class WeeklyRecoveryTests(unittest.TestCase):
    def setUp(self):
        self.now = instant("2026-10-26T13:54:57Z")
        self.key = "weekly:2026-10-26"
        self.receipts = {"format_version": 1, "issues": {self.key: {
            "slot": self.key, "mode": "preview", "run_id": "100", "artifact_id": "200"}}}

    def test_identity_survives_seven_hour_delay_and_changes_only_at_monday_seven(self):
        for time in ["2026-10-26T07:00:00Z", "2026-10-26T13:54:57Z",
                     "2026-10-27T12:00:00Z", "2026-11-02T06:59:59Z"]:
            self.assertEqual(weekly_run.slot(instant(time)), self.key)
        self.assertEqual(weekly_run.slot(instant("2026-10-26T06:59:59Z")), "weekly:2026-10-19")
        self.assertEqual(weekly_run.slot(instant("2026-11-02T07:00:00Z")), "weekly:2026-11-02")

    def test_any_scheduled_tick_recovers_missing_preview_even_on_tuesday(self):
        for cron in [COLLECTION_CRON, PUBLICATION_CRON]:
            for time in [self.now, instant("2026-10-27T16:00:00Z")]:
                env = {"EVENT_NAME": "schedule", "EVENT_SCHEDULE": cron}
                self.assertEqual(weekly_run.recovery_mode(env, time, {}, {})[0],
                                 ("preview", False, False))

    def test_repeat_or_delayed_primary_event_uses_the_uploaded_artifact(self):
        self.assertEqual(weekly_run.recovery_mode({}, self.now, self.receipts, {})[0],
                         ("collect", False, False))
        self.assertEqual(weekly_run.recovery_mode({}, instant("2026-11-02T07:00:00Z"),
                                                 self.receipts, {})[0],
                         ("preview", False, False))

    def test_preview_receipt_never_claims_delivery_when_operator_enables_it(self):
        env = {"DIGEST_DELIVERY_ENABLED": "true"}
        self.assertEqual(weekly_run.recovery_mode(env, self.now, self.receipts, {})[0],
                         ("digest", True, True))
        state = {"sent_slots": {self.key: 900}}
        self.assertEqual(weekly_run.recovery_mode(env, self.now, {}, state)[0],
                         ("collect", False, False))

    def test_safe_recovery_never_sends_even_with_publication_enabled(self):
        for enabled in ["true", "false"]:
            env = {"EVENT_NAME": "workflow_dispatch", "RECOVER_WEEKLY": "true",
                   "DIGEST_DELIVERY_ENABLED": enabled}
            self.assertEqual(choose(env), ("preview", False, False))
            self.assertEqual(weekly_run.recovery_mode(env, self.now, {}, {})[0],
                             ("preview", False, False))
            for conflict in [{"MANUAL_DRY_RUN": "false"}, {"DELIVER_TO_MAIN_CHAT": "true"},
                             {"RETRY_DELIVERY_RUN_ID": "123"}, {"PREVIEW_SOURCE_RUN_ID": "123"}]:
                with self.assertRaises(ValueError):
                    choose({**env, **conflict})

    def test_corrupt_receipts_and_delivery_memory_fail_closed(self):
        for receipts in [{"format_version": 2, "issues": {}},
                         {"format_version": 1, "issues": {self.key: {"artifact_id": ""}}}]:
            with self.assertRaises(ValueError):
                weekly_run.recovery_mode({}, self.now, receipts, {})
        with self.assertRaises(ValueError):
            weekly_run.recovery_mode({}, self.now, {}, {"sent_slots": []})

    def fixture(self, root):
        run = root / "run"
        run.mkdir()
        for name in ["summary.md", "digest.md", "selection.json", "coverage.json",
                     "weekly-window.json", "weekly-source-manifest.json", "issue.md"]:
            (run / name).write_text("{}")
        (run / "manifest.json").write_text('{"sources":[{"status":"included"}]}')
        (run / "ranking-input.json").write_text(json.dumps({
            "format_version": 3, "delivery_slot": self.key,
            "generated_at": "2026-10-26T13:55:37Z", "entries": [{"full_name": "org/project"}]}))
        (run / "telegram-messages.json").write_text('["valid"]')
        (run / "release-audit.json").write_text(json.dumps({
            "approved": True, "deterministic_scan": "passed",
            "telegram_format": {"status": "passed", "message_utf16_units": [5]}}))
        return run

    def test_completion_requires_uploaded_artifact_and_preserves_other_weeks(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            run = self.fixture(root)
            receipts = root / "receipts.json"
            for artifact_id in ["", "１２３", "missing"]:
                with self.assertRaises(ValueError):
                    weekly_run.record(run, self.key, "preview", "123", artifact_id, receipts)
                self.assertFalse(receipts.exists())
            old = {"format_version": 1, "issues": {"weekly:2026-10-19": {"preserved": True}}}
            receipts.write_text(json.dumps(old))
            weekly_run.record(run, self.key, "preview", "123", "456", receipts)
            saved = weekly_run.read_object(receipts)
            self.assertEqual(saved["issues"][self.key]["artifact_id"], "456")
            self.assertEqual(saved["issues"]["weekly:2026-10-19"], {"preserved": True})
            self.assertNotIn("sent_slots", saved)

    def test_wrong_week_failed_audit_missing_or_corrupt_payload_cannot_complete(self):
        for fault in ["wrong-week", "missing-summary", "failed-audit", "oversize", "wrong-length"]:
            with self.subTest(fault=fault), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                run = self.fixture(root)
                key = self.key
                if fault == "wrong-week": key = "weekly:2026-11-02"
                if fault == "missing-summary": (run / "summary.md").unlink()
                if fault == "failed-audit": (run / "release-audit.json").write_text('{"approved":false}')
                if fault == "oversize": (run / "telegram-messages.json").write_text(json.dumps(["x" * 3801]))
                if fault == "wrong-length": (run / "telegram-messages.json").write_text('["different"]')
                with self.assertRaises(ValueError):
                    weekly_run.record(run, key, "preview", "123", "456", root / "receipt.json")
                self.assertFalse((root / "receipt.json").exists())

    def test_failed_collection_is_not_a_completed_empty_issue(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            run = self.fixture(root)
            digest = weekly_run.read_object(run / "ranking-input.json")
            digest["entries"] = []
            (run / "ranking-input.json").write_text(json.dumps(digest))
            (run / "manifest.json").write_text('{"sources":[{"status":"failed"}]}')
            with self.assertRaises(ValueError):
                weekly_run.record(run, self.key, "preview", "123", "456", root / "receipt.json")
            (run / "manifest.json").write_text('{"sources":[{"status":"included"}]}')
            weekly_run.record(run, self.key, "preview", "123", "456", root / "receipt.json")

    def test_real_workflow_orders_restore_verify_upload_receipt_and_archive(self):
        workflow = (Path(__file__).parents[1] / "workflows/run-bot.yml").read_text()
        stages = ["Restore persistent state", "Restore authoritative delivery checkpoint",
                  "Select collection", "Collect, preview", "Verify weekly preview artifact",
                  "Upload run preview", "Record uploaded weekly completion", "Save state"]
        positions = [workflow.index("      - name: " + stage) for stage in stages]
        self.assertEqual(positions, sorted(positions))
        self.assertIn("steps.verify-preview.outcome == 'success' && steps.upload.outcome == 'success'", workflow)
        self.assertIn("if-no-files-found: error", workflow)

    def test_actual_mode_entrypoint_recovers_once_after_restore(self):
        script = Path(__file__).with_name("run_mode.py").resolve()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "data").mkdir()
            (root / "data/state.json").write_text("{}")
            now = datetime.now(timezone.utc)
            key = weekly_run.slot(now)
            env = {**os.environ, "EVENT_NAME": "workflow_dispatch", "RECOVER_WEEKLY": "true",
                   "DIGEST_DELIVERY_ENABLED": "true", "MANUAL_DRY_RUN": "true",
                   "DELIVER_TO_MAIN_CHAT": "false", "RETRY_DELIVERY_RUN_ID": "",
                   "PREVIEW_SOURCE_RUN_ID": "", "PREVIEW_WITH_LLM": "false",
                   "PLAN_FILE": str(root / "plan.json"), "GITHUB_OUTPUT": str(root / "output"),
                   "GITHUB_STEP_SUMMARY": str(root / "summary")}
            subprocess.run([os.sys.executable, str(script)], cwd=root, env=env,
                           capture_output=True, check=True, timeout=10)
            plan = json.loads((root / "plan.json").read_text())
            self.assertEqual((plan["mode"], plan["send"]), ("preview", False))
            receipt = {"format_version": 1, "issues": {key: {
                "slot": key, "mode": "preview", "run_id": "123", "artifact_id": "456"}}}
            (root / "data/weekly-runs.json").write_text(json.dumps(receipt))
            subprocess.run([os.sys.executable, str(script)], cwd=root, env=env,
                           capture_output=True, check=True, timeout=10)
            plan = json.loads((root / "plan.json").read_text())
            self.assertEqual((plan["mode"], plan["send"]), ("collect", False))
            self.assertEqual(plan["reason"], "preview-artifact-already-saved")

    def test_watchdog_passes_only_safe_inputs_without_clock_or_delivery_gate(self):
        workflow = (Path(__file__).parents[1] / "workflows/weekly-recovery.yml").read_text()
        self.assertNotIn("DIGEST_DELIVERY_ENABLED", workflow)
        self.assertNotIn("date -u", workflow)
        script = workflow.split("        run: |\n", 1)[1]
        script = "\n".join(line[10:] for line in script.splitlines())
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            gh = root / "gh"
            gh.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > arguments\n')
            gh.chmod(0o755)
            subprocess.run(["bash", "-c", script], cwd=root, check=True,
                           env={**os.environ, "PATH": str(root) + os.pathsep + os.environ["PATH"],
                                "GITHUB_REPOSITORY": "fixture/digest"}, timeout=10)
            args = (root / "arguments").read_text().splitlines()
            self.assertEqual(args, ["workflow", "run", "run-bot.yml", "--repo", "fixture/digest",
                                    "--ref", "main", "-f", "dry_run=true", "-f",
                                    "recover_weekly=true", "-f", "deliver_to_main_chat=false"])


if __name__ == "__main__":
    unittest.main()
