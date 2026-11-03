"""Independent gates and slots; automatic work is daily-only."""
from datetime import datetime, timezone
import unittest
from unittest.mock import patch
import tempfile
from pathlib import Path
import json
import os
import subprocess

import daily_run
import run_mode
import weekly_run
from test_state_workflow import step_script


class DailyRunTests(unittest.TestCase):
    def setUp(self):
        self.now = datetime(2026, 10, 31, 16, tzinfo=timezone.utc)
        self.key = daily_run.slot(self.now)

    def test_daily_boundary_and_namespace(self):
        self.assertEqual(daily_run.slot(datetime(2027, 1, 1, 14, 59, tzinfo=timezone.utc)), "daily:2026-12-31")
        self.assertEqual(daily_run.slot(datetime(2027, 1, 1, 15, tzinfo=timezone.utc)), "daily:2027-01-01")
        self.assertNotEqual(self.key, weekly_run.slot(self.now))

    def test_morning_collection_does_not_send_yesterday_or_today(self):
        env = {"DIGEST_DELIVERY_ENABLED": "true", "DIGEST_DAILY_DELIVERY_ENABLED": "true"}
        before = datetime(2026, 11, 3, 14, 59, 59, tzinfo=timezone.utc)
        self.assertEqual(daily_run.recovery_mode(env, before, {}, {})[0], ("collect", False, False))
        after = datetime(2026, 11, 3, 15, tzinfo=timezone.utc)
        self.assertEqual(daily_run.recovery_mode(env, after, {}, {})[0], ("daily-digest", True, True))
        sent = {"sent_slots": {daily_run.slot(after): 12}}
        self.assertEqual(daily_run.recovery_mode(env, after, {}, sent)[0], ("collect", False, False))
        self.assertEqual(run_mode.DAILY_CRON, "0 15 * * *")

    def test_daily_requires_two_explicit_gates(self):
        event = {"EVENT_NAME": "schedule", "EVENT_SCHEDULE": run_mode.DAILY_CRON}
        for global_gate, daily_gate in [("", ""), ("true", ""), ("false", "true"), ("true", "TRUE")]:
            env = {**event, "DIGEST_DELIVERY_ENABLED": global_gate, "DIGEST_DAILY_DELIVERY_ENABLED": daily_gate}
            self.assertEqual(run_mode.choose(env), ("daily-preview", False, False))
        self.assertEqual(run_mode.choose({**event, "DIGEST_DELIVERY_ENABLED": "true", "DIGEST_DAILY_DELIVERY_ENABLED": "true"}), ("daily-digest", True, True))

    def test_manual_daily_preview_stays_safe_with_gates_enabled(self):
        env = {"EVENT_NAME": "workflow_dispatch", "DIGEST_FORMAT": "daily", "DIGEST_DELIVERY_ENABLED": "true", "DIGEST_DAILY_DELIVERY_ENABLED": "true"}
        self.assertEqual(run_mode.choose(env), ("daily-preview", False, False))
        self.assertEqual(run_mode.choose({**env, "DELIVER_TO_MAIN_CHAT": "true"}), ("daily-digest", True, True))
        with self.assertRaises(ValueError):
            run_mode.choose({**env, "DIGEST_DAILY_DELIVERY_ENABLED": "false", "DELIVER_TO_MAIN_CHAT": "true"})
        for field in ["RECOVER_WEEKLY", "RETRY_DELIVERY_RUN_ID", "PREVIEW_SOURCE_RUN_ID"]:
            with self.assertRaises(ValueError):
                run_mode.choose({**env, field: "true" if field == "RECOVER_WEEKLY" else "12"})

    def test_weekly_receipt_does_not_complete_daily(self):
        state = {"sent_slots": {weekly_run.slot(self.now): 42}}
        self.assertEqual(daily_run.recovery_mode({}, self.now, {}, state)[0], ("daily-preview", False, False))
        state["sent_slots"][self.key] = 43
        self.assertEqual(daily_run.recovery_mode({}, self.now, {}, state)[0], ("collect", False, False))

    def test_preview_receipt_is_not_delivery_ack(self):
        receipts = {"format_version": 1, "issues": {self.key: {"slot": self.key, "mode": "daily-preview", "artifact_id": "10"}}}
        self.assertEqual(daily_run.recovery_mode({}, self.now, receipts, {})[0], ("collect", False, False))
        enabled = {"DIGEST_DELIVERY_ENABLED": "true", "DIGEST_DAILY_DELIVERY_ENABLED": "true"}
        self.assertEqual(daily_run.recovery_mode(enabled, self.now, receipts, {})[0], ("daily-digest", True, True))

    def test_collection_tick_only_recovers_daily_preview(self):
        env = {"EVENT_NAME": "schedule", "EVENT_SCHEDULE": run_mode.COLLECTION_CRON}
        with patch.object(weekly_run, "recovery_mode", side_effect=AssertionError("automatic weekly recovery")):
            for state in [{}, {"sent_slots": {weekly_run.slot(self.now): 42}}]:
                mode, _ = run_mode.select_plan(env, self.now, {}, {}, state)
                self.assertEqual(mode, ("daily-preview", False, False))

    def test_explicit_weekly_recovery_never_runs_daily(self):
        env = {"EVENT_NAME": "workflow_dispatch", "RECOVER_WEEKLY": "true"}
        state = {"sent_slots": {weekly_run.slot(self.now): 42}}
        self.assertEqual(run_mode.select_plan(env, self.now, {}, {}, state)[0], ("collect", False, False))

    def test_unknown_format_fails_closed(self):
        with self.assertRaises(ValueError):
            run_mode.choose({"EVENT_NAME": "workflow_dispatch", "DIGEST_FORMAT": "typo"})

    def test_daily_preview_shell_only_calls_dry_run(self):
        for llm in ["true", "false"]:
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                binary = root / "target/release/weekly-gh-digest"
                binary.parent.mkdir(parents=True)
                binary.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\" > arguments\n")
                binary.chmod(0o755)
                result = subprocess.run(["bash", "-c", step_script("Collect, preview, or deliver digest")], cwd=root,
                    env={**os.environ, "RUN_MODE": "daily-preview", "USE_LLM": llm}, capture_output=True, text=True, timeout=10)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual((root / "arguments").read_text().splitlines(), ["preview", "--daily", "--dry-run"] + (["--llm"] if llm == "true" else []))

    def test_daily_receipt_cannot_clear_publication_state(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            state = root / "state.json"
            state.write_text('{"sent_slots": {}}')
            before = state.read_bytes()
            digest = {"generated_at": self.now.isoformat(), "entries": []}
            with patch.object(daily_run, "validate_preview", return_value=digest):
                daily_run.record(root, self.key, "daily-preview", "10", "20", root / "daily-runs.json")
            self.assertEqual(state.read_bytes(), before)
            self.assertEqual(json.loads((root / "daily-runs.json").read_text())["issues"][self.key]["mode"], "daily-preview")


if __name__ == "__main__":
    unittest.main()
