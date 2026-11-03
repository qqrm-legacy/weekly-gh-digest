"""One bounded editorial follow-up; reuse captured primary sources, never resend cards."""
import json
import os
from pathlib import Path
import shutil
import subprocess

import soak_test as soak

REVISION = 'test-editorial-summary-2026-10-31-v2'
SOURCE_RUN = '41199584189'
SOURCE_ARTIFACT = 'digest-trial-41199584189-1'


def main():
    soak.preflight()
    soak.require(soak.checkpoint('restore') == 0, 'Isolated trial checkpoint missing')
    before = soak.read(soak.STATE)
    soak.require(not before.get('uncertain_delivery'), 'Uncertain send requires reconciliation')
    if REVISION in before.get('sent_digests', {}):
        soak.write(soak.OUT / 'editorial-followup.json', {'already_acknowledged':True,
            'message_id':before['sent_digests'][REVISION],'new_messages':0,'new_llm_requests':0})
        return
    source = Path(os.environ['RUNNER_TEMP']) / 'editorial-source'
    source.mkdir(exist_ok=False)
    metadata = json.loads(subprocess.check_output(['gh','run','view',SOURCE_RUN,'--repo',soak.REPOSITORY,
        '--json','workflowName,headSha,conclusion'],text=True))
    soak.require(metadata == {'workflowName':'Digest technical-chat trial',
        'headSha':'ffb84311ab6be9da6876536810ab0d41857074f4','conclusion':'success'},
        'Unexpected editorial source run')
    subprocess.run(['gh','run','download',SOURCE_RUN,'--repo',soak.REPOSITORY,
        '--name',SOURCE_ARTIFACT,'--dir',str(source)],check=True)
    destination = soak.OUT / 'editorial-summary'
    shutil.copytree(source / 'weekly',destination)
    parent = before.get('sent_slots',{}).get('weekly:2026-10-26')
    soak.require(isinstance(parent,int) and parent > 0, 'Missing acknowledged test parent')
    count = soak.receipt_count()
    soak.run_bot('editorial-followup','test-summary',str(destination),str(parent))
    after = soak.read(soak.STATE)
    for field in ('covered','sent_slots'):
        soak.require(before.get(field,{}) == after.get(field,{}),'Editorial follow-up changed publication receipts')
    soak.require(before['editions']['daily_covered'] == after['editions']['daily_covered'],
                 'Editorial follow-up changed daily queue')
    soak.require(soak.receipt_count() == count+1, 'Follow-up should send exactly one summary')
    budget = after.get('llm_daily_requests',{})
    soak.run_bot('editorial-followup-repeat','test-summary',str(destination),str(parent))
    repeated = soak.read(soak.STATE)
    soak.require(soak.receipt_count() == count+1, 'Follow-up repeated its message')
    soak.require(repeated.get('llm_daily_requests',{}) == budget, 'Follow-up rerun spent provider requests')
    soak.receipt_check()
    soak.write(soak.OUT / 'editorial-followup.json', {'message_id':after['sent_digests'][REVISION],
        'new_messages':1,'repeat_messages':0,'repeat_llm_requests':0,'queues_unchanged':True,
        'production_writes':0,'source_run':SOURCE_RUN})


if __name__ == '__main__':
    main()
