"""The trial may send only to the independently resolved technical chat."""
from datetime import datetime, timezone
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import soak_test


class SoakSafetyTests(unittest.TestCase):
    def env(self):
        return {'TELEGRAM_BOT_TOKEN':'fixture', 'TELEGRAM_TECH_CHAT_ID':'@test',
                'PRODUCTION_CHAT_ID':'@production', 'GROQ_API_KEY':'fixture',
                'LLM_FREE_TIER_CONFIRMED':'yes'}

    def setup_context(self, now='2026-10-31T12:00:00+00:00'):
        clock = patch('soak_test.datetime')
        mocked = clock.start()
        self.addCleanup(clock.stop)
        mocked.now.return_value = datetime.fromisoformat(now)
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        target = patch('soak_test.OUT', Path(directory.name))
        target.start()
        self.addCleanup(target.stop)

    def test_missing_credentials_never_call_telegram(self):
        self.setup_context()
        with patch.dict(os.environ, {}, clear=True), patch('soak_test.telegram') as call:
            with self.assertRaisesRegex(RuntimeError, 'credentials'):
                soak_test.preflight()
            call.assert_not_called()

    def test_expired_trial_never_calls_telegram(self):
        self.setup_context('2026-11-08T00:00:00+00:00')
        with patch.dict(os.environ,self.env(),clear=True), patch('soak_test.telegram') as call:
            with self.assertRaisesRegex(RuntimeError, 'expired'):
                soak_test.preflight()
            call.assert_not_called()

    def test_aliases_resolving_to_same_chat_are_rejected(self):
        self.setup_context()
        with patch.dict(os.environ,self.env(),clear=True), patch('soak_test.telegram', side_effect=[
                {'id':42}, {'id':-1001,'type':'channel'}, {'id':-1001,'type':'channel'}]) as call:
            with self.assertRaisesRegex(RuntimeError, 'resolves to production'):
                soak_test.preflight()
            self.assertEqual(call.call_count, 3)
            self.assertNotIn('DIGEST_DELIVERY_ENABLED', os.environ)

    def test_success_scopes_all_mutations_to_resolved_test_id(self):
        self.setup_context()
        with patch.dict(os.environ,self.env(),clear=True), patch('soak_test.telegram', side_effect=[
                {'id':42,'username':'test_bot'}, {'id':-1001,'type':'channel'},
                {'id':-1002,'type':'channel'}, {'status':'administrator','can_post_messages':True}]) as call:
            soak_test.preflight()
            self.assertEqual(os.environ['TELEGRAM_CHAT_ID'], '-1001')
            self.assertEqual(os.environ['TELEGRAM_TECH_CHAT_ID'], '-1001')
            self.assertNotIn('PRODUCTION_CHAT_ID', os.environ)
            self.assertEqual(os.environ['DIGEST_TEST_ONLY'], 'true')
            self.assertEqual([args.args[0] for args in call.call_args_list],
                             ['getMe','getChat','getChat','getChatMember'])
            self.assertTrue(soak_test.STATE_BRANCH.startswith('digest-soak-state-'))
            self.assertNotEqual(soak_test.STATE_BRANCH, 'delivery-state')


if __name__ == '__main__':
    unittest.main()
