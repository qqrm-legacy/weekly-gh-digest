//! Optional test-only target guard and server-acknowledged payload evidence.
//! No secrets or chat metadata are copied into the receipt artifact.
use anyhow::{Context, Result, ensure};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use std::{env, fs, io::Write, path::Path};

fn validate_target(chat: &str, expected: &str, until: &str, now: DateTime<Utc>) -> Result<()> {
    ensure!(
        !expected.is_empty() && chat == expected,
        "test delivery target mismatch"
    );
    let until = crate::parse_time(until).context("test delivery deadline missing or invalid")?;
    ensure!(now < until, "test delivery authorization expired");
    Ok(())
}

pub(super) fn guard(chat: &str) -> Result<()> {
    if env::var("DIGEST_TEST_ONLY").as_deref() == Ok("true") {
        validate_target(
            chat,
            &env::var("TELEGRAM_TECH_CHAT_ID").unwrap_or_default(),
            &env::var("DIGEST_TEST_UNTIL").unwrap_or_default(),
            Utc::now(),
        )?;
    }
    Ok(())
}

pub(super) fn receipt(message: &Value, requested_text: &str) -> Result<()> {
    if env::var("DIGEST_TEST_ONLY").as_deref() != Ok("true") {
        return Ok(());
    }
    let path = env::var("TELEGRAM_RECEIPTS_PATH").context("test receipt path missing")?;
    let chat = message["chat"]["id"]
        .as_i64()
        .context("Telegram response omitted chat ID")?
        .to_string();
    guard(&chat)?;
    ensure!(
        message["message_id"].as_i64().is_some(),
        "Telegram response omitted message ID"
    );
    let path = Path::new(&path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    serde_json::to_writer(
        &mut file,
        &json!({
            "message_id":message["message_id"], "date":message["date"],
            "text":message["text"], "entities":message["entities"],
            "reply_to_message_id":message["reply_to_message"]["message_id"],
            "requested_markdown":requested_text, "target_verified":true
        }),
    )?;
    writeln!(file)?;
    file.sync_all()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_target_is_explicit_and_expires() {
        let now = crate::parse_time("2026-10-31T12:00:00Z").unwrap();
        assert!(validate_target("-100123", "-100123", "2026-11-08T00:00:00Z", now).is_ok());
        assert!(validate_target("-100456", "-100123", "2026-11-08T00:00:00Z", now).is_err());
        assert!(validate_target("", "", "2026-11-08T00:00:00Z", now).is_err());
        assert!(validate_target("-100123", "-100123", "invalid", now).is_err());
        assert!(validate_target("-100123", "-100123", "2026-10-31T12:00:00Z", now).is_err());
    }
}
