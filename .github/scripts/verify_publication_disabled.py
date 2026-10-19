"""Exercise every sending CLI entry point with publication explicitly disabled."""
import os
import subprocess
import sys


binary = sys.argv[1]
env = {key: value for key, value in os.environ.items()
       if not key.startswith("TELEGRAM_") and key not in ("GROQ_API_KEY", "GH_TOKEN", "GITHUB_TOKEN")}
env.update(DIGEST_DELIVERY_ENABLED="false", DELIVERY_CHECKPOINT="must-not-be-called.py")
for args in [["digest"], ["debug"], ["debug-delivery", "must-not-be-read"],
             ["debug-delivery", "must-not-be-read", "--dry-run"]]:
    result = subprocess.run([binary, *args], env=env, text=True, capture_output=True,
                            timeout=10, check=False)
    assert result.returncode != 0, args
    assert "publication is disabled pending preview review" in result.stderr, (args, result.stderr)
print("All sending entry points refused execution before collection or Telegram access.")
