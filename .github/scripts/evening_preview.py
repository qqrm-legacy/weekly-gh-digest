"""Validate an actual daily AI preview on a local copy; never send or save remote state."""
import json
import os
from pathlib import Path
import shutil
import subprocess

import daily_run
import remote_state

OUT = Path('validation-artifacts')


def validate_ai_cards(digest):
    if not digest.get('entries'):
        raise RuntimeError('A nonempty daily AI preview is required')
    if not all(row.get('llm_used') is True for row in digest['entries']):
        raise RuntimeError('Raw source fallback reached daily preview')


def main():
    if os.environ.get('GITHUB_REPOSITORY') != 'qqrm/weekly-gh-digest':
        raise RuntimeError('Unexpected repository')
    if any(os.environ.get(key) for key in ('TELEGRAM_BOT_TOKEN', 'TELEGRAM_CHAT_ID', 'TELEGRAM_TECH_CHAT_ID', 'DELIVERY_CHECKPOINT')):
        raise RuntimeError('Read-only preview must not receive delivery credentials or a remote writer')
    if not os.environ.get('GROQ_API_KEY') or os.environ.get('LLM_FREE_TIER_CONFIRMED', '').lower() != 'yes':
        raise RuntimeError('Confirmed free provider is required')
    OUT.mkdir(exist_ok=True)
    remote = remote_state.api('/contents/weekly-gh-digest.json?ref=delivery-state')
    if remote is None:
        raise RuntimeError('Authoritative observations unavailable')
    source = remote_state.decode(remote)
    Path('data').mkdir(exist_ok=True)
    Path('data/state.json').write_bytes(source)
    (OUT / 'provenance.json').write_text(json.dumps({
        'source_state_blob': remote['sha'], 'production_state_writes': 0,
        'telegram_calls': 0, 'llm_provider': 'Groq', 'paid_fallback': False}, indent=2))
    output = Path(os.environ['RUNNER_TEMP']) / 'evening-output'
    output.write_text('')
    # Only this credential-free dry-run child uses local checkpoints. Production
    # execution still requires its remote checkpoint and explicit send permission.
    env = {**os.environ, 'GITHUB_ACTIONS': 'false', 'GITHUB_OUTPUT': str(output),
           'DIGEST_DELIVERY_ENABLED': 'false', 'DIGEST_DAILY_DELIVERY_ENABLED': 'false'}
    with (OUT / 'preview.log').open('w') as log:
        subprocess.run(['./target/debug/weekly-gh-digest', 'preview', '--daily', '--dry-run', '--llm'],
                       env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    paths = [line.split('=', 1)[1] for line in output.read_text().splitlines()
             if line.startswith('run_directory=')]
    directory = Path(paths[-1])
    digest = json.loads((directory / 'ranking-input.json').read_text())
    daily_run.validate_preview(directory, digest['delivery_slot'])
    validate_ai_cards(digest)
    shutil.copytree(directory, OUT / 'daily', dirs_exist_ok=True)
    result = {'cards': len(digest['entries']),
              'all_cards_ai': True, 'llm_budget': digest['llm_budget'], 'queue': digest['queue'],
              'telegram_calls': 0, 'production_state_writes': 0}
    (OUT / 'report.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n')
    print(json.dumps(result, ensure_ascii=False))


if __name__ == '__main__':
    try:
        main()
    except Exception:
        if Path('data/runs').exists():
            OUT.mkdir(exist_ok=True)
            shutil.copytree('data/runs', OUT / 'failed-runs', dirs_exist_ok=True)
        raise
