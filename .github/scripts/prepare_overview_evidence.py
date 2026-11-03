"""One-time exact source edits; export and commit the validated result, not this helper."""
from pathlib import Path

p = Path('src/providers.rs')
text = p.read_text()
old = '        let overview_payload = source_overview_payload(&overview_entries);'
new = '''        let mut overview_payload = source_overview_payload(&overview_entries);
        // Prefer the exact README quotation already checked for this card.
        // The first README paragraph may be a table of contents, not evidence.
        // Generated card prose is deliberately not treated as a primary source.
        for item in overview_payload["entries"].as_array_mut().into_iter().flatten() {
            if let Some(quote) = item["repository"].as_str()
                .and_then(|name| evidence_quotes.get(name)) {
                item["evidence"] = json!(quote);
                item["evidence_source"] = json!("verified_exact_readme_quote");
            }
        }'''
assert text.count(old) == 1
p.write_text(text.replace(old, new))
p = Path('src/providers/tests.rs')
text = p.read_text()
old = '    assert_eq!(payload["entries"][0]["repository"], "org/ready");'
new = old + '''
    assert_eq!(payload["entries"][0]["evidence"], README_EVIDENCE);
    assert_eq!(payload["entries"][0]["evidence_source"], "verified_exact_readme_quote");'''
assert text.count(old) == 1
p.write_text(text.replace(old, new))
