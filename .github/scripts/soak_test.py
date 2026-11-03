"""One iteration of the explicitly authorized, time-bounded technical-chat trial.

The normal digest binary is used; only the target and durable namespace differ.
No production receipt, variable, secret, message or branch is modified.
"""
from datetime import datetime, timedelta, timezone
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import urllib.error
import urllib.request

import remote_state

REPOSITORY = 'qqrm-labs/weekly-gh-digest'
STATE_BRANCH = 'digest-soak-state-2026-10-31'
START = datetime.fromisoformat('2026-10-31T12:00:00+00:00')
UNTIL = datetime.fromisoformat('2026-11-08T00:00:00+00:00')
STATE = Path('data/state.json')
OUT = Path('soak-artifacts')
BOT = './target/debug/weekly-gh-digest'


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def read(path):
    return json.loads(Path(path).read_text())


def write(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + '.tmp')
    tmp.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
    tmp.replace(path)


def instant(value):
    return datetime.fromisoformat(value.replace('Z', '+00:00'))


def checkpoint(operation):
    env = {**os.environ, 'DELIVERY_STATE_KEY': 'weekly-gh-digest.json'}
    return subprocess.run([sys.executable, __file__, operation, str(STATE)], env=env, check=False).returncode


def telegram(method, payload):
    token = os.environ['TELEGRAM_BOT_TOKEN']
    request = urllib.request.Request('https://api.telegram.org/bot' + token + '/' + method,
        data=json.dumps(payload).encode(), headers={'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            value = json.load(response)
    except (urllib.error.URLError, TimeoutError, ValueError):
        raise RuntimeError('Telegram preflight failed; no token or response body logged') from None
    require(value.get('ok') is True, 'Telegram preflight rejected the request')
    return value['result']


def preflight():
    require(datetime.now(timezone.utc) < UNTIL, 'Test authorization expired')
    required = ['TELEGRAM_BOT_TOKEN', 'TELEGRAM_TECH_CHAT_ID', 'PRODUCTION_CHAT_ID']
    presence = {key:bool(os.environ.get(key, '').strip()) for key in required}
    presence['GROQ_API_KEY'] = bool(os.environ.get('GROQ_API_KEY', '').strip())
    presence['free_tier_confirmed'] = os.environ.get('LLM_FREE_TIER_CONFIRMED', '').lower() == 'yes'
    write(OUT / 'preflight.json', {'credentials_present':presence,'production_delivery_changed':False})
    require(all(presence[key] for key in required), 'Technical-chat credentials or production target missing; refusing fallback')
    me = telegram('getMe', {})
    tech = telegram('getChat', {'chat_id':os.environ['TELEGRAM_TECH_CHAT_ID']})
    prod = telegram('getChat', {'chat_id':os.environ['PRODUCTION_CHAT_ID']})
    require(tech['id'] != prod['id'], 'Technical chat resolves to production; refusing all sends')
    require(tech['type'] in ('channel', 'supergroup'), 'Technical chat must support the digest channel links')
    member = telegram('getChatMember', {'chat_id':tech['id'],'user_id':me['id']})
    require(member.get('status') not in ('left', 'kicked'), 'Bot is absent from the technical chat')
    if tech['type'] == 'channel':
        require(member.get('status') == 'creator' or member.get('can_post_messages') is True,
                'Bot cannot post in the technical channel')
    require(presence['GROQ_API_KEY'] and presence['free_tier_confirmed'],
            'Real editorial trial requires the configured confirmed-free LLM; refusing a misleading fallback-only trial')
    evidence = {'credentials_present':presence, 'bot_username':me.get('username'),
        'technical_target_distinct':True, 'technical_chat_type':tech['type'],
        'bot_membership':member.get('status'), 'production_delivery_changed':False,
        'expires_at':UNTIL.isoformat(), 'state_branch':STATE_BRANCH}
    write(OUT / 'preflight.json', evidence)
    os.environ['TELEGRAM_TECH_CHAT_ID'] = str(tech['id'])
    os.environ['TELEGRAM_CHAT_ID'] = str(tech['id'])
    os.environ.pop('PRODUCTION_CHAT_ID', None)
    os.environ['DIGEST_TEST_ONLY'] = 'true'
    os.environ['DIGEST_TEST_UNTIL'] = UNTIL.isoformat()
    os.environ['DIGEST_DELIVERY_ENABLED'] = 'true'
    os.environ['DIGEST_DAILY_DELIVERY_ENABLED'] = 'true'
    os.environ['DELIVERY_CHECKPOINT'] = str(Path(__file__).resolve())
    os.environ['DELIVERY_STATE_KEY'] = 'weekly-gh-digest.json'
    os.environ['TELEGRAM_RECEIPTS_PATH'] = str((OUT / 'telegram-receipts.jsonl').resolve())
    return tech


def preview(state, directory, now, daily=False):
    source = directory.parent / (directory.name + '-state.json')
    write(source, state)
    before = source.read_bytes()
    args = [BOT, 'preview-retained', str(source), str(directory), '--at=' + now.isoformat()]
    if daily:
        args.append('--daily')
    subprocess.run(args, check=True, stdout=subprocess.DEVNULL)
    require(source.read_bytes() == before, 'Offline preview mutated its input')
    return read(directory / 'ranking-input.json')


def names(digest):
    return {row['full_name'].lower() for row in digest['entries'] + digest.get('catalogue', [])}


def replay_week(production, now):
    """Seven simulated daily ACKs, followed by weekly selection; no HTTP or sends."""
    root = Path(os.environ['RUNNER_TEMP']) / 'digest-replay'
    root.mkdir(exist_ok=False)
    end = now.replace(hour=7, minute=0, second=0, microsecond=0)
    if end > now:
        end -= timedelta(days=1)
    start = end - timedelta(days=7)
    rows = [row for row in production.get('snapshots', []) if start <= instant(row['captured_at']) <= end]
    state = {'snapshots':[], 'editions':{'version':1,'daily_covered':{}}}
    daily_names = set()
    counts = []
    for day in range(1,8):
        cut = start + timedelta(days=day)
        state['snapshots'] = [row for row in rows if instant(row['captured_at']) <= cut]
        digest = preview(state, root / ('day-' + str(day)), cut, True)
        selected = names(digest)
        require(not (daily_names & selected), 'Simulated daily duplicate')
        daily_names |= selected
        state['editions']['daily_covered'].update({name:cut.date().isoformat() for name in selected})
        counts.append(len(selected))
    weekly = preview(state, root / 'weekly', end)
    seen = {row['full_name'].lower() for row in rows}
    require(daily_names <= names(weekly), 'Daily findings were consumed before the weekly summary')
    require(seen == names(weekly), 'Weekly summary omitted retained observations')
    newest = max((row['captured_at'] for row in rows), default=None)
    latest_names = {row['full_name'].lower() for row in rows if row['captured_at'] == newest}
    evidence = {'simulation':True,'telegram_calls':0,'llm_calls':0,'days':7,
        'period_start':start.isoformat(),'period_end':end.isoformat(),
        'daily_card_counts':counts,'daily_unique':len(daily_names),'weekly_unique':len(names(weekly)),
        'absent_from_last_capture_but_retained':len(seen-latest_names),
        'all_observed_findings_in_weekly':True,'daily_names_in_weekly':True}
    write(OUT / 'seven-day-replay.json', evidence)


def receipt_count():
    p = OUT / 'telegram-receipts.jsonl'
    return len(p.read_text().splitlines()) if p.exists() else 0


def run_bot(label, *args):
    with (OUT / (label + '.log')).open('w') as output:
        result = subprocess.run([BOT,*args], stdout=output, stderr=subprocess.STDOUT, check=False)
    require(result.returncode == 0, 'Bot iteration failed: ' + label + '; inspect redacted artifact log; do not blindly retry')


def receipt_check():
    p = OUT / 'telegram-receipts.jsonl'
    receipts = [json.loads(line) for line in p.read_text().splitlines()] if p.exists() else []
    require(len({row['message_id'] for row in receipts}) == len(receipts), 'Duplicate server message ID')
    for row in receipts:
        require(row['target_verified'] and isinstance(row['text'],str), 'Invalid Telegram acknowledgement')
        length = len(row['text'].encode('utf-16-le'))//2
        require(0 < length <= 3800, 'Telegram acknowledged an unexpected payload length')
        for entity in row.get('entities') or []:
            require(0 <= entity['offset'] < length and entity['offset'] + entity['length'] <= length,
                    'Invalid server entity bounds')
    return receipts


def main():
    require(os.environ.get('GITHUB_REPOSITORY') == REPOSITORY, 'Wrong repository')
    OUT.mkdir(exist_ok=True)
    tech = preflight()
    production_remote = remote_state.api('/contents/weekly-gh-digest.json?ref=delivery-state')
    require(production_remote is not None, 'Production observations unavailable; refusing to infer history')
    production = json.loads(remote_state.decode(production_remote))
    replay_week(production, datetime.now(timezone.utc))
    status = checkpoint('restore')
    if status == 3:
        write(STATE, {'editions':{'version':1}})
        require(checkpoint('save') == 0, 'Cannot bootstrap isolated test state')
    else:
        require(status == 0, 'Cannot restore isolated test state')
    state = read(STATE)
    require(not state.get('uncertain_delivery'), 'An earlier test send needs reconciliation; do not reset state')
    rows = {(row['captured_at'],row['full_name'].lower()):row for row in state.get('snapshots',[])}
    imported = 0
    for row in production.get('snapshots',[]):
        if START <= instant(row['captured_at']) <= datetime.now(timezone.utc):
            key = (row['captured_at'],row['full_name'].lower())
            if key not in rows:
                rows[key] = row
                imported += 1
    state['snapshots'] = list(rows.values())
    budget = state.setdefault('llm_daily_requests', {})
    for day, used in production.get('llm_daily_requests',{}).items():
        budget[day] = max(budget.get(day,0),used)
    write(STATE,state)
    require(checkpoint('save') == 0, 'Cannot checkpoint imported observations')
    report = {'started_at':datetime.now(timezone.utc).isoformat(),'imported_snapshots':imported,
        'production_state_blob_read':production_remote['sha'],'production_writes':0,
        'state_branch':STATE_BRANCH,'iterations':[]}
    for edition in ('daily','weekly'):
        before = read(STATE)
        count = receipt_count()
        args = ['digest','--llm'] + (['--daily'] if edition == 'daily' else [])
        run_bot(edition, *args)
        after = read(STATE)
        if edition == 'daily':
            require(before.get('covered',{}) == after.get('covered',{}), 'Daily delivery consumed weekly queue')
        else:
            require(before['editions'].get('daily_covered',{}) == after['editions'].get('daily_covered',{}),
                    'Weekly delivery consumed daily queue')
        new_ids = sorted(set(after.get('sent_digests',{})) - set(before.get('sent_digests',{})))
        sent = receipt_count() - count
        budget_after = dict(after.get('llm_daily_requests',{}))
        covered_after = json.dumps([after.get('covered',{}),after['editions'].get('daily_covered',{})],sort_keys=True)
        run_bot(edition + '-repeat', *args)
        repeated = read(STATE)
        require(receipt_count() == count+sent, 'Same-slot rerun sent duplicate messages')
        require(budget_after == repeated.get('llm_daily_requests',{}), 'Same-slot rerun spent new LLM requests')
        require(covered_after == json.dumps([repeated.get('covered',{}),repeated['editions'].get('daily_covered',{})],sort_keys=True),
                'Same-slot rerun consumed more findings')
        for digest_id in new_ids:
            capture = digest_id.split('-',1)[1]
            source = Path('data/runs') / capture
            if source.exists():
                shutil.copytree(source, OUT / edition, dirs_exist_ok=True)
            require(after['overview_ids'].get(digest_id), 'Completion omitted final overview ID')
        report['iterations'].append({'edition':edition,'new_digest_ids':new_ids,'messages_sent':sent,
            'repeat_messages':0,'repeat_llm_requests':0,'independent_queue_ack':True})
        write(OUT / 'report.json', report)
    state = read(STATE)
    receipts = receipt_check()
    report['finished_at'] = datetime.now(timezone.utc).isoformat()
    report['acknowledged_messages'] = len(receipts)
    report['sent_slots'] = state.get('sent_slots',{})
    report['pending_issues'] = len(state.get('pending_digests',{}))
    report['uncertain_delivery'] = bool(state.get('uncertain_delivery'))
    report['daily_acknowledged'] = len(state['editions'].get('daily_covered',{}))
    report['weekly_acknowledged'] = len(state.get('covered',{}))
    report['observed_repositories'] = len({row['full_name'].lower() for row in state.get('snapshots',[])})
    report['message_links'] = ['https://t.me/c/' + str(tech['id'])[4:] + '/' + str(row['message_id']) for row in receipts]
    write(OUT / 'report.json', report)
    print(json.dumps({key:report[key] for key in ['acknowledged_messages','daily_acknowledged','weekly_acknowledged','pending_issues','uncertain_delivery']}))


if __name__ == '__main__':
    if len(sys.argv) == 3 and sys.argv[1] in ('restore','save'):
        require(os.environ.get('GITHUB_REPOSITORY') == REPOSITORY, 'Wrong checkpoint repository')
        remote_state.BRANCH = STATE_BRANCH
        os.environ['DELIVERY_STATE_KEY'] = 'weekly-gh-digest.json'
        remote_state.main()
    else:
        try:
            main()
        except Exception as error:
            OUT.mkdir(exist_ok=True)
            write(OUT / 'failure.json', {'error':str(error),'production_writes':0})
            print(str(error), file=sys.stderr)
            sys.exit(1)
