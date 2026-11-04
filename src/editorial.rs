//! Bounded provider backoff and minimum editorial readiness, not a factual audit.
use anyhow::{Result, ensure};
use reqwest::{StatusCode, header::HeaderMap};
use serde_json::Value;
use std::time::Duration;

pub(super) const POLICY_VERSION: u32 = 5;

pub(super) fn retry_delay(
    status: StatusCode,
    headers: &HeaderMap,
    attempt: usize,
) -> Result<Duration> {
    if status != StatusCode::TOO_MANY_REQUESTS {
        return Ok(Duration::from_secs(2_u64.pow(attempt as u32)));
    }
    let seconds = headers
        .get("retry-after")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value.ceil() as u64)
        .unwrap_or(60);
    // Do not sleep through a daily limit, or retry sooner than requested.
    ensure!(
        seconds <= 120,
        "provider cooldown exceeds this run's retry window; retain queue for a later run"
    );
    Ok(Duration::from_secs(seconds.max(1)))
}

/// Conservative check for invented Latin product/integration names. This is
/// not semantic entailment: lowercase names and unrelated claims need review.
fn check_named_terms(text: &str, sources: &str) -> Result<()> {
    const COMMON: &[&str] = &["README", "JSON", "HTTP", "HTTPS", "HTML", "API"];
    for word in text.split(|ch: char| !ch.is_ascii_alphanumeric()) {
        let named = word
            .chars()
            .next()
            .is_some_and(|ch| ch.is_ascii_uppercase());
        if word.len() >= 4 && named && !COMMON.contains(&word) {
            ensure!(
                sources.contains(&word.to_ascii_lowercase()),
                "named product or integration is absent from the supplied primary sources"
            );
        }
    }
    Ok(())
}

/// Language and observed terminology regressions only; not a factual audit.
pub(super) fn validate_text(text: &str, sources: &str) -> Result<()> {
    let letters = text.chars().filter(|ch| ch.is_alphabetic()).count();
    let cyrillic = text
        .chars()
        .filter(|ch| matches!(ch, '\u{0400}'..='\u{04ff}'))
        .count();
    ensure!(
        cyrillic >= 10 && cyrillic * 4 >= letters,
        "editorial text must be Russian"
    );
    let sources = sources.to_lowercase();
    check_named_terms(text, &sources)?;
    let text = text.to_lowercase();
    let multi_agent = sources.contains("multi-agent")
        || sources.contains("multi agent")
        || sources.contains("multiagent");
    let multi_user = sources.contains("multi-user")
        || sources.contains("multi user")
        || sources.contains("multiuser")
        || sources.contains("многопользовательск");
    ensure!(
        !(multi_agent && !multi_user && text.contains("многопользовательск")),
        "multi-agent is not multi-user"
    );
    let local_first = sources.contains("local-first") || sources.contains("local first");
    let offline_claim = [
        "полностью офлайн",
        "без интернета",
        "не требует интернета",
        "не обращается к облаку",
    ]
    .iter()
    .any(|claim| text.contains(claim));
    ensure!(
        !(local_first
            && offline_claim
            && !sources.contains("offline")
            && !sources.contains("офлайн")),
        "local-first does not prove offline operation"
    );
    let normalized = sources
        .split_whitespace()
        .filter(|word| *word != ">")
        .collect::<Vec<_>>()
        .join(" ");
    let broken_skill = [
        "нарушить работу навыка",
        "навык не работает",
        "навык перестаёт работать",
    ]
    .iter()
    .any(|claim| text.contains(claim));
    ensure!(
        !(normalized.contains("skill still works") && broken_skill),
        "unavailable supplemental checklists do not mean the skill stops working"
    );
    Ok(())
}

/// Requested daily issues need edited cards, not an issue-wide synthesis.
pub(super) fn require_synthesis(digest: &Value) -> Result<()> {
    let requested = digest["llm_requested"] == true;
    if requested && digest["edition"] == "daily" {
        for entry in digest["entries"].as_array().into_iter().flatten() {
            ensure!(
                entry["llm_used"] == true,
                "daily source fallback must remain queued, not be published"
            );
            validate_text(
                entry["description"].as_str().unwrap_or(""),
                &entry["latest"].to_string(),
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn invented_named_integrations_are_rejected() {
        let text = "Плагины позволяют подключить Salesforce для работы с заявками клиентов.";
        assert!(validate_text(text, "Plugins with Slack, Notion and Jira connectors.").is_err());
        assert!(validate_text(text, "Plugins include a Salesforce connector.").is_ok());
        assert!(
            validate_text(
                "По README инструмент возвращает JSON с результатами анализа.",
                "A source analysis tool."
            )
            .is_ok()
        );
    }

    #[test]
    fn missing_supplement_does_not_turn_into_a_broken_skill() {
        let source =
            "The skill still\n> works, but supplementary shared checklists are unavailable.";
        assert!(
            validate_text(
                "Отсутствие общих чек-листов может нарушить работу навыка.",
                source
            )
            .is_err()
        );
        assert!(
            validate_text(
                "Навык продолжает работать, но общие чек-листы недоступны.",
                source
            )
            .is_ok()
        );
    }

    #[test]
    fn observed_translation_regressions_are_rejected_not_rewritten() {
        assert!(
            validate_text(
                "Многопользовательский инструмент для автоматического анализа.",
                "A multi-agent framework"
            )
            .is_err()
        );
        assert!(
            validate_text(
                "Многоагентный инструмент для автоматического анализа.",
                "A multi-agent framework"
            )
            .is_ok()
        );
        assert!(
            validate_text(
                "Многопользовательский инструмент для автоматического анализа.",
                "multi-agent and multi-user"
            )
            .is_ok()
        );
        assert!(
            validate_text(
                "Работает полностью офлайн без интернета и внешних сервисов.",
                "local-first data store with cloud APIs"
            )
            .is_err()
        );
        assert!(
            validate_text(
                "This tool is used to process source files and build graphs.",
                "source"
            )
            .is_err()
        );
    }

    #[test]
    fn daily_fallback_is_not_an_editorial_card() {
        let digest = json!({"edition":"daily", "llm_requested":true, "entries":[{"llm_used":false,"description":"Raw README description"}]});
        assert!(require_synthesis(&digest).is_err());
    }

    #[test]
    fn rate_limit_honors_retry_after_and_never_retries_a_long_cooldown_early() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "17.2".parse().unwrap());
        assert_eq!(
            retry_delay(StatusCode::TOO_MANY_REQUESTS, &headers, 0)
                .unwrap()
                .as_secs(),
            18
        );
        headers.insert("retry-after", "121".parse().unwrap());
        assert!(retry_delay(StatusCode::TOO_MANY_REQUESTS, &headers, 0).is_err());
        for value in ["invalid", "NaN", "-1"] {
            headers.insert("retry-after", value.parse().unwrap());
            assert_eq!(
                retry_delay(StatusCode::TOO_MANY_REQUESTS, &headers, 0)
                    .unwrap()
                    .as_secs(),
                60
            );
        }
        assert_eq!(
            retry_delay(StatusCode::TOO_MANY_REQUESTS, &HeaderMap::new(), 0)
                .unwrap()
                .as_secs(),
            60
        );
        assert_eq!(
            retry_delay(StatusCode::INTERNAL_SERVER_ERROR, &headers, 1)
                .unwrap()
                .as_secs(),
            2
        );
    }

    #[test]
    fn edited_cards_do_not_require_an_issue_summary() {
        let mut digest = json!({"edition":"daily","llm_requested":true,"entries":[{
            "full_name":"org/repo","llm_used":true,
            "description":"Инструмент разбирает исходные файлы и показывает их зависимости.",
            "latest":{"description":"Parses source files and shows their dependencies."}
        }]});
        assert!(require_synthesis(&digest).is_ok());
        // A legacy summary is no longer rendered or part of card readiness.
        digest["overview"] = json!("unused legacy text");
        assert!(require_synthesis(&digest).is_ok());
        digest["entries"][0]["description"] = json!("An untranslated source description.");
        assert!(require_synthesis(&digest).is_err());
        digest["entries"][0]["description"] =
            json!("Инструмент подключается к Salesforce для обработки заявок клиентов.");
        assert!(require_synthesis(&digest).is_err());
        digest["edition"] = json!("weekly");
        assert!(require_synthesis(&digest).is_ok());
        digest["edition"] = json!("daily");
        digest["llm_requested"] = json!(false);
        assert!(require_synthesis(&digest).is_ok());
    }
}
