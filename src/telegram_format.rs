//! Offline validation of the MarkdownV2 subset emitted by this bot.
//!
//! Supported: escaped text, bold/italic, inline code and HTTP(S) inline links
//! with escaped plain labels. Unsupported MarkdownV2 constructs fail closed.
//! This is neither a complete Telegram parser nor an API acceptance test.

use anyhow::{Result, anyhow, ensure};

pub(super) const VALIDATION_SCOPE: &str = "Offline emitted MarkdownV2 subset: escaped text, bold/italic, inline code and HTTP(S) links; not the complete Telegram parser or API acceptance";

/// Validate the actual message before preview acceptance or any network send.
/// Diagnostics identify positions and error classes, never publication text.
pub(super) fn validate_message(text: &str, limit: usize) -> Result<()> {
    ensure!(!text.is_empty(), "Telegram message is empty");
    ensure!(
        text.encode_utf16().count() <= limit,
        "Telegram message exceeds {limit} UTF-16 units"
    );
    let bytes = text.as_bytes();
    let mut index = 0;
    let mut entities = Vec::<(u8, usize)>::new();
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index = escaped(bytes, index, None)?,
            b'`' => {
                ensure!(
                    entities.is_empty(),
                    "MarkdownV2 code cannot be nested in formatting at byte {index}"
                );
                index = code(bytes, index)?;
            }
            b'[' => index = link(bytes, index)?,
            marker @ (b'*' | b'_') => {
                if entities.last().is_some_and(|(open, _)| *open == marker) {
                    entities.pop();
                } else {
                    ensure!(
                        !entities.iter().any(|(open, _)| *open == marker),
                        "MarkdownV2 formatting entities cross at byte {index}"
                    );
                    entities.push((marker, index));
                }
                index += 1;
            }
            byte if reserved(byte) => {
                return Err(anyhow!(
                    "MarkdownV2 has an unescaped reserved character at byte {index}"
                ));
            }
            byte => {
                valid_literal(byte, index)?;
                index += 1;
            }
        }
    }
    ensure!(
        entities.is_empty(),
        "MarkdownV2 has an unclosed formatting entity"
    );
    Ok(())
}

fn reserved(byte: u8) -> bool {
    b"_*[]()~`>#+-=|{}.!\\".contains(&byte)
}

fn valid_literal(byte: u8, index: usize) -> Result<()> {
    ensure!(
        byte >= b' ' || matches!(byte, b'\n' | b'\r' | b'\t'),
        "MarkdownV2 has an unsupported control character at byte {index}"
    );
    Ok(())
}

fn escaped(bytes: &[u8], index: usize, allowed: Option<&[u8]>) -> Result<usize> {
    let next = bytes
        .get(index + 1)
        .ok_or_else(|| anyhow!("MarkdownV2 has a trailing escape at byte {index}"))?;
    ensure!(
        allowed.map_or((1..=126).contains(next), |values| values.contains(next)),
        "MarkdownV2 has an unsupported escape at byte {index}"
    );
    Ok(index + 2)
}

fn code(bytes: &[u8], start: usize) -> Result<usize> {
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index = escaped(bytes, index, Some(b"\\`"))?,
            b'`' => {
                ensure!(
                    index > start + 1,
                    "MarkdownV2 has an empty code entity at byte {start}"
                );
                return Ok(index + 1);
            }
            byte => {
                valid_literal(byte, index)?;
                index += 1;
            }
        }
    }
    Err(anyhow!(
        "MarkdownV2 has an unclosed code entity at byte {start}"
    ))
}

fn link(bytes: &[u8], start: usize) -> Result<usize> {
    let mut index = start + 1;
    loop {
        let byte = *bytes
            .get(index)
            .ok_or_else(|| anyhow!("MarkdownV2 has an unclosed link label at byte {start}"))?;
        if byte == b'\\' {
            index = escaped(bytes, index, None)?;
        } else if byte == b']' {
            ensure!(
                index > start + 1,
                "MarkdownV2 has an empty link label at byte {start}"
            );
            ensure!(
                bytes.get(index + 1) == Some(&b'('),
                "MarkdownV2 link label has no URL at byte {start}"
            );
            index += 2;
            break;
        } else {
            ensure!(
                !reserved(byte),
                "MarkdownV2 link label is outside the emitted plain-label subset at byte {index}"
            );
            valid_literal(byte, index)?;
            index += 1;
        }
    }
    let url_start = index;
    ensure!(
        bytes[url_start..].starts_with(b"https://") || bytes[url_start..].starts_with(b"http://"),
        "MarkdownV2 link is outside the emitted HTTP(S) URL subset at byte {url_start}"
    );
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index = escaped(bytes, index, Some(b"\\)"))?,
            b')' => return Ok(index + 1),
            byte => {
                ensure!(
                    !byte.is_ascii_whitespace() && !byte.is_ascii_control(),
                    "MarkdownV2 link URL contains whitespace or control text at byte {index}"
                );
                index += 1;
            }
        }
    }
    Err(anyhow!(
        "MarkdownV2 has an unclosed link URL at byte {start}"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_the_observed_fallback_parentheses_and_empty_issue_period() {
        assert!(validate_message("Главное в выпуске: org/repo (\\+123)", 3800).is_err());
        assert!(validate_message("Выпуск без новых проектов.", 3800).is_err());
        assert!(validate_message("Главное в выпуске: org/repo \\(\\+123\\)", 3800).is_ok());
        assert!(validate_message("Выпуск без новых проектов\\.", 3800).is_ok());
    }

    #[test]
    fn validates_the_emitted_subset_without_treating_link_punctuation_as_text() {
        let message = "*Заголовок*\n_Оговорка\\._\n[org/repo\\-1](https://github.com/org/repo-1)\n`@scope/pkg` и `C:\\\\tmp` \\(пример\\)\\.";
        assert!(validate_message(message, 3800).is_ok());
        assert!(validate_message("*bold _italic_*", 3800).is_ok());
        assert!(validate_message("[link](https://example.org/a\\)b)", 3800).is_ok());
    }

    #[test]
    fn rejects_incomplete_entities_escapes_and_unsupported_constructs() {
        for message in [
            "*unclosed",
            "_unclosed",
            "`unclosed",
            "[unclosed",
            "[label](https://example.org",
            "dangling\\",
            "*cross _entities* _",
            "[label](https://example.org/a b)",
            "``",
            "~unsupported~",
        ] {
            assert!(validate_message(message, 3800).is_err(), "{message:?}");
        }
    }

    #[test]
    fn limits_utf16_units_instead_of_unicode_scalar_count() {
        let message = "😀".repeat(1900);
        assert!(validate_message(&message, 3800).is_ok());
        assert!(validate_message(&(message + "😀"), 3800).is_err());
        assert!(validate_message("", 3800).is_err());
    }
}
