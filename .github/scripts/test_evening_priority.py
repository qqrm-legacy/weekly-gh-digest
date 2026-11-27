"""Split schedules never fall back to weekly delivery or mix collection and sending."""
from copy import deepcopy
from datetime import datetime, timedelta, timezone
from pathlib import Path
import unittest
from unittest.mock import patch

import daily_run
import run_mode
import weekly_run


class EveningPriorityTests(unittest.TestCase):
    def setUp(self):
        self.env = {"EVENT_NAME": "schedule", "EVENT_SCHEDULE": run_mode.COLLECTION_CRON,
                    "RUN_OPERATION": "collect", "DIGEST_DELIVERY_ENABLED": "true",
                    "DIGEST_DAILY_DELIVERY_ENABLED": "true"}
        self.publish = {**self.env, "EVENT_SCHEDULE": run_mode.DAILY_CRON,
                        "RUN_OPERATION": "publish"}
        self.evening = datetime(2026, 11, 3, 15, 17, tzinfo=timezone.utc)

    def test_only_publisher_can_recover_a_daily_issue(self):
        for hour in (0, 12, 15, 18, 23):
            now = self.evening.replace(hour=hour)
            self.assertEqual(run_mode.select_plan(self.env, now, {}, {}, {})[0],
                             ("collect", False, False))
            self.assertEqual(run_mode.select_plan(self.publish, now, {}, {}, {})[0],
                             ("daily-digest", True, True))
            state = {"sent_slots": {daily_run.slot(now): 42}}
            self.assertEqual(run_mode.select_plan(self.publish, now, {}, {}, state)[0],
                             ("skip", False, False))

    def test_retired_crons_never_send(self):
        for cron in (run_mode.PUBLICATION_CRON, run_mode.LEGACY_COLLECTION_CRON):
            env = {**self.env, "RUN_OPERATION": "", "EVENT_SCHEDULE": cron}
            self.assertEqual(run_mode.select_plan(env, self.evening, {}, {}, {})[0],
                             ("collect", False, False))

    def test_safe_explicit_weekly_recovery_is_still_preview_only(self):
        env = {**self.env, "RUN_OPERATION": "", "EVENT_NAME": "workflow_dispatch",
               "RECOVER_WEEKLY": "true", "DIGEST_FORMAT": "weekly"}
        chosen, _ = run_mode.select_plan(env, self.evening, {}, {}, {})
        self.assertEqual(chosen, ("preview", False, False))

    def test_two_weeks_of_collection_never_select_any_sender_or_mutate_history(self):
        monday = datetime(2026, 11, 2, tzinfo=timezone.utc)
        state = {"snapshots": [{"full_name": "org/retained", "captured_at": "2026-10-01T00:00:00Z"}],
                 "covered": {"org/old": "2026-10-01"},
                 "editions": {"version": 1, "daily_covered": {}, "analyses": {"org/ready": {}}},
                 "pending_digests": {"weekly-existing": {"edition": "weekly"}},
                 "sent_slots": {}}
        with patch.object(weekly_run, "recovery_mode", side_effect=AssertionError("automatic weekly recovery")):
            for day in range(14):
                for hour in range(0, 24, 2):
                    now = monday + timedelta(days=day, hours=hour, minutes=17)
                    before = deepcopy(state)
                    chosen, _ = run_mode.select_plan(self.env, now, {}, {}, state)
                    self.assertEqual(chosen, ("collect", False, False), now.isoformat())
                    self.assertEqual(state, before)

    def test_disabled_daily_gate_never_falls_back_to_weekly_delivery(self):
        for value in ("", "false", "TRUE"):
            env = {**self.publish, "DIGEST_DAILY_DELIVERY_ENABLED": value}
            with patch.object(weekly_run, "recovery_mode", side_effect=AssertionError("weekly fallback")):
                self.assertEqual(run_mode.select_plan(env, self.evening, {}, {}, {})[0],
                                 ("daily-preview", False, False))

    def test_manual_preview_and_collect_never_send_even_with_delivery_enabled(self):
        for operation, expected in (("preview", "daily-offline-preview"), ("collect", "collect")):
            env = {**self.env, "RUN_OPERATION": operation, "EVENT_NAME": "workflow_dispatch",
                   "DELIVER_TO_MAIN_CHAT": "true", "PREVIEW_WITH_LLM": "true"}
            self.assertEqual(run_mode.select_plan(env, self.evening, {}, {}, {})[0],
                             (expected, False, False))
        with self.assertRaises(ValueError):
            run_mode.choose({**self.env, "RUN_OPERATION": "publish"})

    def test_weekly_recovery_has_no_automatic_trigger(self):
        workflows = Path(__file__).parents[1] / "workflows"
        recovery = (workflows / "weekly-recovery.yml").read_text()
        triggers = recovery.split("on:\n", 1)[1].split("\npermissions:", 1)[0]
        self.assertEqual(triggers.strip(), "workflow_dispatch:")


if __name__ == "__main__":
    unittest.main()
