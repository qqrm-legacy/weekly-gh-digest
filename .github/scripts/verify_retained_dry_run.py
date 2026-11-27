"""Check actual retained state read-only before exposing a new stable release."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

import remote_state


def verify(binary):
    binary = Path(binary).resolve(strict=True)
    remote = remote_state.api("/contents/weekly-gh-digest.json?ref=delivery-state")
    if remote is None:
        raise RuntimeError("Authoritative retained state unavailable")
    content = remote_state.decode(remote)
    env = {"PATH": os.environ.get("PATH", "/usr/bin:/bin"), "LANG": "C.UTF-8",
           "GITHUB_ACTIONS": "false", "DIGEST_DELIVERY_ENABLED": "false",
           "DIGEST_DAILY_DELIVERY_ENABLED": "false"}
    with tempfile.TemporaryDirectory(prefix="digest-retained-dry-run-") as temporary:
        root = Path(temporary)
        state = root / "state.json"
        state.write_bytes(content)
        before = hashlib.sha256(content).hexdigest()
        output = root / "preview"
        subprocess.run([str(binary), "preview-retained", str(state), str(output), "--daily"],
                       cwd=root, env=env, check=True, timeout=60)
        if hashlib.sha256(state.read_bytes()).hexdigest() != before:
            raise RuntimeError("Read-only dry run changed checkpoint bytes")
        digest = json.loads((output / "ranking-input.json").read_text())
        queue = json.loads((output / "queue.json").read_text())
        if digest.get("edition") != "daily" or queue.get("edition") != "daily":
            raise RuntimeError("Unexpected preview edition")
        if not isinstance(digest.get("entries"), list) or len(digest["entries"]) > 12:
            raise RuntimeError("Invalid daily selection")
        print(json.dumps({"state_sha256": before, "delivery_slot": digest["delivery_slot"],
                          "selected": len(digest["entries"]), "queue": queue["summary"],
                          "telegram_calls": 0, "provider_calls": 0, "checkpoint_writes": 0}))


if __name__ == "__main__":
    verify(sys.argv[1])
