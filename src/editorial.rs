//! Bounded provider backoff and minimum editorial readiness, not a factual audit.
use anyhow::{Result, ensure};
use reqwest::{StatusCode, header::HeaderMap};
use serde_json::Value;
use std::time::Duration;

pub(super) const POLICY_VERSION: u32 = 3;

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
    let text = text.to_lowercase();
    let sources = sources.to_lowercase();
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
    Ok(())
}

pub(super) fn require_synthesis(digest: &Value) -> Result<()> {
    let requested = digest["llm_requested"] == true;
    let has_entries = digest["entries"]
        .as_array()
        .is_some_and(|rows| !rows.is_empty());
    ensure!(
        !requested
            || !has_entries
            || digest["overview"]
                .as_str()
                .is_some_and(|text| !text.trim().is_empty()),
        "requested editorial synthesis is missing; findings remain pending, no Telegram delivery"
    );
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
        if has_entries {
            validate_text(
                digest["overview"].as_str().unwrap_or(""),
                &digest["entries"].to_string(),
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
        let digest = json!({"edition":"daily", "llm_requested":true, "overview":"В этой подборке представлены инструменты анализа программ.", "entries":[{"llm_used":false,"description":"Raw README description"}]});
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
    fn a_requested_summary_cannot_silently_turn_into_a_top_three_fallback() {
        let mut digest =
            json!({"llm_requested":true,"entries":[{"full_name":"org/repo"}],"overview":""});
        assert!(require_synthesis(&digest).is_err());
        digest["overview"] = json!("В этой подборке — инструмент разбора исходников.");
        assert!(require_synthesis(&digest).is_ok());
        digest["llm_requested"] = json!(false);
        digest["overview"] = json!("");
        assert!(require_synthesis(&digest).is_ok());
    }
}
