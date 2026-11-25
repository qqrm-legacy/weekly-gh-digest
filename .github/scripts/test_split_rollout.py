"""Exercise actual workflow commands and enforce offline preview boundaries."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from test_state_workflow import step_script


class SplitRolloutTests(unittest.TestCase):
    def test_offline_preview_leaves_state_unchanged_and_never_sends(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "data").mkdir()
            state = root / "data/state.json"
            state.write_text('{"sent_slots":{"daily:2026-11-22":168}}')
            before = state.read_bytes()
            binary = root / "bin/weekly-gh-digest"
            binary.parent.mkdir()
            binary.write_text('#!/bin/sh\nset -eu\n[ "$1" = preview-retained ]\n[ "$2" = data/state.json ]\n[ "$4" = --daily ]\n[ -z "${DELIVERY_CHECKPOINT:-}" ]\nmkdir -p "$3"\necho "{}" > "$3/ranking-input.json"\necho "{}" > "$3/queue.json"\n')
            binary.chmod(0o755)
            env = {**os.environ, "RUN_MODE": "daily-offline-preview", "RUN_OPERATION": "preview",
                   "GITHUB_RUN_ID": "42", "GITHUB_RUN_ATTEMPT": "1", "GITHUB_OUTPUT": str(root / "out"),
                   "TELEGRAM_BOT_TOKEN": "", "TELEGRAM_CHAT_ID": "", "TELEGRAM_TECH_CHAT_ID": "",
                   "GROQ_API_KEY": "", "DELIVERY_CHECKPOINT": "must-not-run"}
            result = subprocess.run(["bash", "-c", step_script("Collect, preview, or deliver digest")],
                                    cwd=root, env=env, capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(state.read_bytes(), before)
            self.assertEqual((root / "out").read_text(), "run_directory=data/runs/preview-42-1\n")
            env["TELEGRAM_BOT_TOKEN"] = "fixture-should-be-rejected"
            result = subprocess.run(["bash", "-c", step_script("Collect, preview, or deliver digest")],
                                    cwd=root, env=env, capture_output=True, text=True, timeout=10)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(state.read_bytes(), before)

    def test_publish_uses_retained_data_and_skip_does_no_work(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "bin/weekly-gh-digest"
            binary.parent.mkdir()
            binary.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > arguments\n')
            binary.chmod(0o755)
            env = {**os.environ, "RUN_MODE": "daily-digest", "RUN_OPERATION": "publish"}
            result = subprocess.run(["bash", "-c", step_script("Collect, preview, or deliver digest")],
                                    cwd=root, env=env, capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual((root / "arguments").read_text().splitlines(), ["digest", "--daily", "--llm", "--retained-only"])
            (root / "arguments").unlink()
            env["RUN_MODE"] = "skip"
            result = subprocess.run(["bash", "-c", step_script("Collect, preview, or deliver digest")],
                                    cwd=root, env=env, capture_output=True, text=True, timeout=10)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse((root / "arguments").exists())
