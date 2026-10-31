"""Exact, idempotent integration edits for the test branch; export the tested source."""
from pathlib import Path
p = Path('src/main.rs')
s = p.read_text()
def replace(old, new):
    global s
    if new in s:
        return
    if s.count(old) != 1:
        raise RuntimeError('Integration anchor changed; refusing to guess')
    s = s.replace(old, new)
replace('mod telegram_format;\n', 'mod telegram_format;\nmod test_delivery;\n')
replace('fn telegram_send_reply_to(chat: &str, reply_to: Option<i64>, text: &str) -> Result<i64> {\n',
        'fn telegram_send_reply_to(chat: &str, reply_to: Option<i64>, text: &str) -> Result<i64> {\n    test_delivery::guard(chat)?;\n    if env::var("DIGEST_TEST_ONLY").as_deref() == Ok("true") {\n        std::thread::sleep(StdDuration::from_millis(3100));\n    }\n')
replace('    resp["result"]["message_id"]\n        .as_i64()',
        '    test_delivery::receipt(&resp["result"], text)?;\n    resp["result"]["message_id"]\n        .as_i64()')
replace('fn telegram_pin_in(chat: &str, message_id: i64) -> Result<()> {\n',
        'fn telegram_pin_in(chat: &str, message_id: i64) -> Result<()> {\n    test_delivery::guard(chat)?;\n')
replace('    let footer = format!(\n        "_{}_",',
        '    let header = if env::var("DIGEST_TEST_ONLY").as_deref() == Ok("true") {\n        format!("*ТЕСТОВЫЙ ВЫПУСК — наблюдения с 31\\\\.10\\\\.2026*\\n{header}")\n    } else { header };\n    let footer = format!(\n        "_{}_",')
old='''    let weekly = schedule::candidates(&state, now);
    write_json(&gathered.dir.join("weekly-observations.json"), &weekly)?;'''
new='''    // Keep collecting on retries, but do not spend provider requests for a
    // publication slot that already has a durable acknowledgement.
    if command == "digest" && !dry && state.sent_slots.contains_key(&slot_key) {
        println!("Slot {slot_key} already delivered; collection retained, no provider calls or sends");
        return Ok(());
    }
    let weekly = schedule::candidates(&state, now);
    write_json(&gathered.dir.join("weekly-observations.json"), &weekly)?;'''
replace(old, new)
old='''    if let Err(error) = telegram_pin(first_id) {
        // Pinning is cosmetic; the issue is delivered regardless.
        println!("Pin skipped: {error:#}");
    }
'''
if old in s:
    s=s.replace(old,'',1)
old='''    state.pending_digests.remove(digest_id);
    write_json(&state_path(), state)?;
    Ok(())
}'''
new='''    state.pending_digests.remove(digest_id);
    write_json(&state_path(), state)?;
    // Pinning is cosmetic and must follow complete, durable delivery.
    if let Err(error) = telegram_pin(first_id) {
        println!("Pin skipped: {error:#}");
    }
    Ok(())
}'''
replace(old,new)
p.write_text(s)
