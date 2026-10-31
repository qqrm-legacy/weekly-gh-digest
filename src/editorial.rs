//! Bounded provider backoff and minimum editorial readiness, not a factual audit.
use anyhow::{Result, ensure};
use reqwest::{StatusCode, header::HeaderMap};
use serde_json::Value;
use std::time::Duration;

pub(super) const POLICY_VERSION: u32 = 2;

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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
