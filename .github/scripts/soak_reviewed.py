"""One reviewed correction of the acknowledged test summary, without new posts.

The edit has its own durable intent; an ambiguous result is never retried blindly.
The original model response remains in its immutable Actions artifact.
"""
from pathlib import Path

import soak_test as soak

REVISION = 'test-editorial-reviewed-2026-10-31-v3'
ORIGINAL = 'test-editorial-summary-2026-10-31-v2'
TEXT = Path('docs/test-summary-reviewed-2026-10-31.txt')


def main():
    tech = soak.preflight()
    soak.require(soak.checkpoint('restore') == 0, 'Isolated checkpoint missing')
    state = soak.read(soak.STATE)
    soak.require(not state.get('uncertain_delivery'), 'Uncertain test delivery requires reconciliation')
    if REVISION in state.get('sent_digests', {}):
        soak.write(soak.OUT / 'reviewed-repeat.json', {'message_id':state['sent_digests'][REVISION],
            'new_messages':0,'new_edits':0,'new_llm_requests':0,'already_acknowledged':True})
        return
    message_id = state.get('sent_digests', {}).get(ORIGINAL)
    soak.require(isinstance(message_id,int) and message_id > 0, 'Original test summary is not acknowledged')
    text = TEXT.read_text().strip()
    soak.require(0 < len(text.encode('utf-16-le'))//2 <= 3800, 'Reviewed text is too long')
    state['uncertain_delivery'] = REVISION + ':' + str(message_id)
    soak.write(soak.STATE,state)
    soak.require(soak.checkpoint('save') == 0, 'Cannot checkpoint editorial edit intent')
    message = soak.telegram('editMessageText', {'chat_id':tech['id'],'message_id':message_id,
        'text':text,'link_preview_options':{'is_disabled':True}})
    soak.require(isinstance(message,dict) and message.get('message_id') == message_id
        and message.get('chat',{}).get('id') == tech['id'] and message.get('text') == text,
        'Editorial edit acknowledgement did not match the requested technical-chat message')
    receipt = {'revision':REVISION,'operation':'editMessageText','message_id':message_id,
        'text':message['text'],'entities':message.get('entities',[]),'edit_date':message.get('edit_date'),
        'target_verified':True,'authorship':'manual primary-source review','production_writes':0,
        'new_messages':0,'new_edits':1,'repository_queues_unchanged':True}
    soak.write(soak.OUT / 'reviewed-telegram-receipt.json',receipt)
    (soak.OUT / 'reviewed-summary.md').write_text(message['text']+'\n')
    state['uncertain_delivery'] = None
    state.setdefault('sent_digests',{})[REVISION] = message_id
    soak.write(soak.STATE,state)
    soak.require(soak.checkpoint('save') == 0, 'Cannot checkpoint acknowledged editorial edit')
    print('Reviewed test summary edited with a verified Telegram acknowledgement; no new post')


if __name__ == '__main__':
    main()
