"""Publication requires an explicit repository gate; preview never selects a sender."""
import unittest
import os
from pathlib import Path
import re
import subprocess
import tempfile
import json

from run_mode import COLLECTION_CRON, PUBLICATION_CRON, DAILY_CRON, choose
from test_state_workflow import step_script


class RunModeTests(unittest.TestCase):
    def manual(self, **values):
        return choose({"EVENT_NAME": "workflow_dispatch", **values})

    def test_default_dispatch_and_llm_preview_do_not_send(self):
        self.assertEqual(self.manual(), ("preview", False, False))
        self.assertEqual(self.manual(PREVIEW_WITH_LLM="true"), ("preview", False, True))
        self.assertEqual(self.manual(PREVIEW_WITH_LLM="true",
                         DIGEST_DELIVERY_ENABLED="true"), ("preview", False, True))

    def test_stored_preview_never_selects_a_sender_or_provider_and_rejects_conflicts(self):
        for edition in ["daily", "weekly"]:
            for enabled in ["true", "false"]:
                for llm in ["true", "false"]:
                    self.assertEqual(self.manual(PREVIEW_SOURCE_RUN_ID="123", DIGEST_FORMAT=edition,
                                     DIGEST_DELIVERY_ENABLED=enabled, PREVIEW_WITH_LLM=llm),
                                     ("preview-stored", False, False))
        for conflict in [{"RETRY_DELIVERY_RUN_ID":"123"}, {"DELIVER_TO_MAIN_CHAT":"true"},
                         {"MANUAL_DRY_RUN":"false"}, {"RECOVER_WEEKLY":"true"}]:
            with self.assertRaises(ValueError):
                self.manual(PREVIEW_SOURCE_RUN_ID="123", **conflict)
        for value in ["../123", "１２３", "1;echo bad"]:
            with self.assertRaises(ValueError):
                self.manual(PREVIEW_SOURCE_RUN_ID=value)

    def test_every_send_path_is_blocked_until_review(self):
        for values in [{"DELIVER_TO_MAIN_CHAT": "true"}, {"MANUAL_DRY_RUN": "false"},
                       {"RETRY_DELIVERY_RUN_ID": "123"}]:
            for enabled in ["", "false", "yes", "TRUE"]:
                with self.subTest(values=values, enabled=enabled):
                    with self.assertRaises(ValueError):
                        self.manual(DIGEST_DELIVERY_ENABLED=enabled, **values)

    def test_allowed_send_paths_still_need_explicit_intent(self):
        for values, expected in [
            ({"DELIVER_TO_MAIN_CHAT": "true"}, ("digest", True, True)),
            ({"MANUAL_DRY_RUN": "false"}, ("debug", True, True)),
            ({"RETRY_DELIVERY_RUN_ID": "123"}, ("retry", True, False)),
        ]:
            self.assertEqual(self.manual(DIGEST_DELIVERY_ENABLED="true", **values), expected)

    def test_collection_never_uses_providers_or_telegram(self):
        for enabled in ["true", "false"]:
            self.assertEqual(choose({"EVENT_NAME": "schedule", "EVENT_SCHEDULE": COLLECTION_CRON,
                             "DIGEST_DELIVERY_ENABLED": enabled}), ("collect", False, False))

    def test_retired_monday_schedule_only_collects_even_with_delivery_enabled(self):
        event = {"EVENT_NAME": "schedule", "EVENT_SCHEDULE": PUBLICATION_CRON}
        self.assertEqual(choose(event), ("collect", False, False))
        self.assertEqual(choose({**event, "DIGEST_DELIVERY_ENABLED": "true"}),
                         ("collect", False, False))

    def test_unknown_event_schedule_or_retry_fails_closed(self):
        for event in [{"EVENT_NAME": "push"}, {"EVENT_NAME": "schedule"},
                      {"EVENT_NAME": "schedule", "EVENT_SCHEDULE": "0 19 * * *"}]:
            with self.assertRaises(ValueError):
                choose(event)
        for value in ["1;echo bad", "１２３", "../123"]:
            with self.assertRaises(ValueError):
                self.manual(RETRY_DELIVERY_RUN_ID=value, DIGEST_DELIVERY_ENABLED="true")

    def test_workflow_schedules_and_secret_injection_match_the_gate(self):
        workflow = (Path(__file__).parents[1] / "workflows/run-bot.yml").read_text()
        self.assertEqual(re.findall(r"cron: '([^']+)'", workflow),
                         [COLLECTION_CRON, DAILY_CRON])
        for secret in ["TELEGRAM_BOT_TOKEN", "TELEGRAM_CHAT_ID", "TELEGRAM_TECH_CHAT_ID"]:
            self.assertIn("steps.mode.outputs.send == 'true' && secrets." + secret, workflow)
        self.assertIn("steps.mode.outputs.llm == 'true' && secrets.GROQ_API_KEY", workflow)

    def test_actual_preview_shell_cannot_choose_a_sending_command(self):
        for llm in ["true", "false"]:
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                binary = root / "target/release/weekly-gh-digest"
                binary.parent.mkdir(parents=True)
                binary.write_text("#!/bin/sh\nprintf '%s\\n' \"$@\" > arguments\n")
                binary.chmod(0o755)
                result = subprocess.run(
                    ["bash", "-c", step_script("Collect, preview, or deliver digest")],
                    cwd=root, env={**os.environ, "RUN_MODE": "preview", "USE_LLM": llm},
                    capture_output=True, text=True, timeout=10, check=False)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual((root / "arguments").read_text().splitlines(),
                                 ["preview", "--dry-run"] + (["--llm"] if llm == "true" else []))

    def test_stored_preview_shell_rejects_wrong_workflow_branch_or_failed_run(self):
        valid = {"workflowName":"Run Bot", "headBranch":"main", "conclusion":"success"}
        for invalid in [{"workflowName":"Untrusted"}, {"headBranch":"pull-request"},
                        {"conclusion":"failure"}]:
            with self.subTest(invalid=invalid), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                (root / "metadata").write_text(json.dumps({**valid, **invalid}))
                gh = root / "gh"
                gh.write_text("#!/bin/sh\nprintf '%s\\n' \"$*\" >> commands\n"
                              "if [ \"$1\" = run ] && [ \"$2\" = view ]; then cat metadata; else exit 90; fi\n")
                gh.chmod(0o755)
                result = subprocess.run(["bash", "-c", step_script("Collect, preview, or deliver digest")],
                    cwd=root, env={**os.environ, "PATH":str(root)+os.pathsep+os.environ["PATH"],
                        "RUN_MODE":"preview-stored", "PREVIEW_SOURCE_RUN_ID":"123",
                        "GITHUB_REPOSITORY":"fixture/weekly"},
                    capture_output=True, text=True, timeout=10, check=False)
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn("command not found", result.stderr)
                self.assertEqual(len((root / "commands").read_text().splitlines()), 1)


if __name__ == "__main__":
    unittest.main()
