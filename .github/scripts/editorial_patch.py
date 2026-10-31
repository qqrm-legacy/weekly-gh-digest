"""Apply auditable editorial regression fixes before exporting exact tested files."""
from pathlib import Path


def edit(name, changes):
    p = Path(name)
    text = p.read_text()
    for old, new in changes:
        if text.count(old) != 1:
            raise RuntimeError('Editorial integration anchor changed in ' + name)
        text = text.replace(old, new)
    p.write_text(text)


edit('src/main.rs', [
    ('mod editions;\n', 'mod editions;\nmod editorial;\n'),
    ('    if command == "preview-stored" {', '    if command == "test-summary" {\n        return test_delivery::summary(&args);\n    }\n    if command == "preview-stored" {'),
    ('"llm_used":complete,', '"llm_used":complete,\n        "llm_requested":do_llm,'),
    ('    // The deterministic credential scan blocks unsafe publication text.\n', '    editorial::require_synthesis(digest)?;\n    // The deterministic credential scan blocks unsafe publication text.\n'),
    ('    let description = entry["description"].as_str().unwrap_or("").trim();',
     '    let description = entry["description"].as_str().unwrap_or("").trim();\n    let description = if entry["llm_used"] == false {\n        format!("Исходное описание, без редакторского разбора: {description}")\n    } else { description.to_string() };'),
    ('        render_markdown_v2_inline(description)\n', '        render_markdown_v2_inline(&description)\n'),
])
edit('src/providers.rs', [
    ('Если сведений мало, не заполняй объём догадками. Не добавляй заголовки и Markdown.',
     'Начни с названия проекта и его задачи, без вводной «В этой подборке». Если сведений мало, не заполняй объём догадками. Не добавляй заголовки и Markdown. Поле evidence_quote можно дословно скопировать из переданного evidence_quote, но не изменять и не переводить его.'),
    ('Для общего итога начни словами «В этой подборке» и дай 4–6 конкретных предложений о представленных задачах; группируй только общие явно указанные свойства.',
     'Только для запроса общего итога начни словами «В этой подборке» и дай 4–6 конкретных предложений; в карточке отдельного проекта эта вводная запрещена. Группируй только явно общие свойства. Свойство отдельного модуля не переносится на весь продукт: например, детектор без LLM не означает работу всех агентных команд без LLM. Local-first и локальное хранение не означают отсутствие облачных вызовов; подключаемые API-модели требуют обращения к провайдеру. При конфликтующих числах опускай цифру или явно сообщай расхождение.'),
    ('            let payload = serde_json::to_string(&bundle)?;',
     '            if calls >= MAX_LLM_REQUESTS - 6 {\n                failure_reason = "description budget reserved for the final synthesis".into();\n                continue;\n            }\n            let payload = serde_json::to_string(&bundle)?;'),
    ('        for attempt in 0..MAX_PROVIDER_ATTEMPTS {\n            self.reserve_request(state, calls)?;',
     '        for attempt in 0..MAX_PROVIDER_ATTEMPTS {\n            self.reserve_request(state, calls)?;\n            let mut delay = StdDuration::from_secs(2_u64.pow(attempt as u32));'),
    ('                    let status = response.status();\n                    let code = response',
     '                    let status = response.status();\n                    delay = crate::editorial::retry_delay(status, response.headers(), attempt)?;\n                    let code = response'),
    ('                self.clock\n                    .sleep(StdDuration::from_secs(2_u64.pow(attempt as u32)));',
     '                self.clock.sleep(delay);'),
    ('            if let Ok(text) = self.issue_overview(&overview_payload, state, &mut calls) {\n                overview = text;\n            }',
     '            match self.issue_overview(&overview_payload, state, &mut calls) {\n                Ok(text) => overview = text,\n                Err(error) => failure_reason.push_str(&format!("; overview unavailable: {error:#}")),\n            }'),
])
edit('src/providers/tests.rs', [
    ('[StdDuration::from_secs(1), StdDuration::from_secs(2)]',
     '[StdDuration::from_secs(60), StdDuration::from_secs(2)]'),
])
edit('src/editions.rs', [
    ('pub(super) struct Analysis {\n',
     'pub(super) struct Analysis {\n    #[serde(default)]\n    policy_version: u32,\n'),
    ('            analysis.source_description == snapshot.description',
     '            analysis.policy_version == crate::editorial::POLICY_VERSION\n                && analysis.source_description == snapshot.description'),
    ('            Analysis {\n                source_description:',
     '            Analysis {\n                policy_version: crate::editorial::POLICY_VERSION,\n                source_description:'),
])
