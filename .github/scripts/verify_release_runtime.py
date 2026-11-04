"""Run a relocated binary offline, including its actual working-directory state guard."""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile


def verify(binary):
    binary = Path(binary).resolve(strict=True)
    env = {
        "PATH": os.environ.get("PATH", "/usr/bin:/bin"),
        "LANG": "C.UTF-8",
        "GITHUB_ACTIONS": "false",
        "DIGEST_DELIVERY_ENABLED": "false",
        "DIGEST_DAILY_DELIVERY_ENABLED": "false",
    }
    with tempfile.TemporaryDirectory(prefix="digest-release-runtime-") as directory:
        root = Path(directory)

        def run(*args, overrides=None):
            return subprocess.run(
                [str(binary), *args], cwd=root, env={**env, **(overrides or {})},
                capture_output=True, text=True, timeout=15, check=False,
            )

        version = run("--version", overrides={"GITHUB_ACTIONS": "true"})
        assert version.returncode == 0, version.stderr
        assert re.fullmatch(
            r"weekly-gh-digest \d+\.\d+\.\d+ \((?:[0-9a-f]{40}|unknown)\)\n",
            version.stdout,
        ), version.stdout
        assert not list(root.iterdir()), "Reading the version must have no filesystem side effects"

        stored = root / "stored"
        stored.mkdir()
        description = (
            "Локально показывает зависимости исходных файлов.\n\n"
            "Помогает оценить объём изменений перед обновлением."
        )
        digest = {
            "format_version": 3, "edition": "weekly",
            "generated_at": "2026-11-04T15:00:00Z", "llm_used": False,
            "entries": [{
                "full_name": "example/portable", "description": description,
                "joke": "", "latest": {"language": "Rust", "stars": 100},
            }],
        }
        (stored / "ranking-input.json").write_text(json.dumps(digest), encoding="utf-8")
        (stored / "selection.json").write_text(
            json.dumps({"pool": [{"full_name": "example/portable"}]}), encoding="utf-8",
        )
        preview = run("preview-stored", str(stored))
        assert preview.returncode == 0, preview.stderr
        refreshed = json.loads((stored / "ranking-input.json").read_text(encoding="utf-8"))
        assert refreshed["entries"][0]["description"] == description
        refresh = json.loads((stored / "format-refresh.json").read_text(encoding="utf-8"))
        assert refresh["provider_requests"] == 0 and refresh["message_count"] == 1
        message = (stored / "issue.md").read_text(encoding="utf-8")
        assert "\\#Rust" in message and "\n\nПомогает" in message

        # Corrupt state must be read from this new working directory and stop the
        # delivery path before any credential lookup or possible network access.
        state = root / "data/state.json"
        state.parent.mkdir()
        state.write_text("{invalid-state", encoding="utf-8")
        rejected = run(
            "debug-delivery", str(stored),
            overrides={"DIGEST_DELIVERY_ENABLED": "true"},
        )
        assert rejected.returncode != 0
        assert "stored state is invalid; refusing empty dedup memory" in rejected.stderr, rejected.stderr
        assert state.read_text(encoding="utf-8") == "{invalid-state"
        assert not (stored / "debug-delivery.json").exists()
    print("Relocated binary: version, offline paragraph rendering and local-state guard passed.")


if __name__ == "__main__":
    verify(sys.argv[1])
