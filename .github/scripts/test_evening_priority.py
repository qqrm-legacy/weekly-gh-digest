"""Automatic publication is daily-only, including recovery and stale cron events."""
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
                    "DIGEST_DELIVERY_ENABLED": "true", "DIGEST_DAILY_DELIVERY_ENABLED": "true"}
        self.evening = datetime(2026, 11, 3, 15, 17, tzinfo=timezone.utc)

    def test_missing_weekly_ack_does_not_starve_evening_daily(self):
        chosen, _ = run_mode.select_plan(self.env, self.evening, {}, {}, {})
        self.assertEqual(chosen, ("daily-digest", True, True))

    def test_after_daily_ack_only_collection_remains(self):
        state = {"sent_slots": {daily_run.slot(self.evening): 42}}
        chosen, _ = run_mode.select_plan(self.env, self.evening, {}, {}, state)
        self.assertEqual(chosen, ("collect", False, False))

    def test_morning_and_retired_weekly_cron_only_collect(self):
        morning = self.evening.replace(hour=12)
        self.assertEqual(run_mode.select_plan(self.env, morning, {}, {}, {})[0],
                         ("collect", False, False))
        env = {**self.env, "EVENT_SCHEDULE": run_mode.PUBLICATION_CRON}
        self.assertEqual(run_mode.select_plan(env, self.evening, {}, {}, {})[0],
                         ("collect", False, False))

    def test_safe_explicit_weekly_recovery_is_still_preview_only(self):
        env = {**self.env, "EVENT_NAME": "workflow_dispatch", "RECOVER_WEEKLY": "true",
               "DIGEST_FORMAT": "weekly"}
        chosen, _ = run_mode.select_plan(env, self.evening, {}, {}, {})
        self.assertEqual(chosen, ("preview", False, False))

    def test_two_weeks_of_ticks_never_select_weekly_or_mutate_history(self):
        monday = datetime(2026, 11, 2, tzinfo=timezone.utc)
        state = {"snapshots": [{"full_name": "org/retained", "captured_at": "2026-10-01T00:00:00Z"}],
                 "covered": {"org/old": "2026-10-01"},
                 "editions": {"version": 1, "daily_covered": {}, "analyses": {"org/ready": {}}},
                 "pending_digests": {"weekly-existing": {"edition": "weekly"}},
                 "sent_slots": {}}
        with patch.object(weekly_run, "recovery_mode", side_effect=AssertionError("automatic weekly recovery")):
            for day in range(14):
                for hour in (0, 3, 6, 7, 9, 12, 14, 15, 18, 21):
                    now = monday + timedelta(days=day, hours=hour, minutes=17)
                    before = deepcopy(state)
                    chosen, _ = run_mode.select_plan(self.env, now, {}, {}, state)
                    expected = ("collect", False, False) if hour < 15 else ("daily-digest", True, True)
                    self.assertEqual(chosen, expected, now.isoformat())
                    self.assertEqual(state, before)
                    acknowledged = deepcopy(state)
                    acknowledged["sent_slots"][daily_run.slot(now)] = 42
                    self.assertEqual(run_mode.select_plan(self.env, now, {}, {}, acknowledged)[0],
                                     ("collect", False, False))

    def test_disabled_daily_gate_never_falls_back_to_weekly_delivery(self):
        for value in ("", "false", "TRUE"):
            env = {**self.env, "DIGEST_DAILY_DELIVERY_ENABLED": value}
            with patch.object(weekly_run, "recovery_mode", side_effect=AssertionError("weekly fallback")):
                self.assertEqual(run_mode.select_plan(env, self.evening, {}, {}, {})[0],
                                 ("daily-preview", False, False))

    def test_weekly_recovery_has_no_automatic_trigger(self):
        workflows = Path(__file__).parents[1] / "workflows"
        recovery = (workflows / "weekly-recovery.yml").read_text()
        triggers = recovery.split("on:\n", 1)[1].split("\npermissions:", 1)[0]
        self.assertEqual(triggers.strip(), "workflow_dispatch:")


if __name__ == "__main__":
    unittest.main()
