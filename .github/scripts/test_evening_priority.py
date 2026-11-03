"""Evening delivery cannot be starved by a failed weekly synthesis."""
from datetime import datetime, timezone
import unittest

import daily_run
import run_mode


class EveningPriorityTests(unittest.TestCase):
    def setUp(self):
        self.env = {'EVENT_NAME':'schedule', 'EVENT_SCHEDULE':run_mode.COLLECTION_CRON,
                    'DIGEST_DELIVERY_ENABLED':'true', 'DIGEST_DAILY_DELIVERY_ENABLED':'true'}
        self.evening = datetime(2026, 11, 3, 15, 17, tzinfo=timezone.utc)

    def test_missing_weekly_ack_does_not_starve_evening_daily(self):
        chosen, _ = run_mode.select_plan(self.env, self.evening, {}, {}, {})
        self.assertEqual(chosen, ('daily-digest', True, True))

    def test_weekly_recovers_after_daily_ack(self):
        state = {'sent_slots':{daily_run.slot(self.evening):42}}
        chosen, _ = run_mode.select_plan(self.env, self.evening, {}, {}, state)
        self.assertEqual(chosen, ('digest', True, True))

    def test_morning_and_explicit_weekly_cron_stay_weekly(self):
        morning = self.evening.replace(hour=12)
        self.assertEqual(run_mode.select_plan(self.env, morning, {}, {}, {})[0], ('digest', True, True))
        env = {**self.env, 'EVENT_SCHEDULE':run_mode.PUBLICATION_CRON}
        self.assertEqual(run_mode.select_plan(env, self.evening, {}, {}, {})[0], ('digest', True, True))

    def test_safe_weekly_recovery_never_sends_daily(self):
        env = {**self.env, 'EVENT_NAME':'workflow_dispatch', 'RECOVER_WEEKLY':'true'}
        chosen, _ = run_mode.select_plan(env, self.evening, {}, {}, {})
        self.assertEqual(chosen, ('preview', False, False))


if __name__ == '__main__':
    unittest.main()
