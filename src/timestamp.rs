//! UTC instants and the timestamp encodings already stored in digest checkpoints.
//!
//! Wire formats are deliberate: observation identities use whole seconds and `Z`,
//! checkpoint serde uses 0/3/6/9 fractional digits and `Z`, and diagnostic strings
//! use the same fractional precision with `+00:00`.
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    borrow::Cow,
    cmp::Ordering,
    ops::{Add, AddAssign, Sub},
    str::FromStr,
};
use time::format_description::{Component, FormatItem, well_known::Rfc3339};
use time::macros::format_description;
use time::{Date, Month, OffsetDateTime, PlainDateTime, SignedDuration, Time, UtcOffset};

const WIRE: &[FormatItem<'_>] =
    format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]");
const ARCHIVE: &[FormatItem<'_>] =
    format_description!("[year][month][day]T[hour][minute][second]Z");

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Timestamp {
    // Always UTC. For a leap second, keep the preceding second and its original
    // fractional part; time's RFC3339 parser otherwise aliases :60 to :59.999999999.
    inner: OffsetDateTime,
    leap_second: bool,
}

impl Timestamp {
    pub(super) fn now() -> Self {
        Self {
            inner: OffsetDateTime::now_utc(),
            leap_second: false,
        }
    }

    pub(super) fn parse(input: &str) -> Result<Self> {
        ensure!(
            matches!(input.as_bytes().get(10), Some(b'T' | b't' | b' ')),
            "invalid RFC3339 date/time separator"
        );
        let leap_second = input.as_bytes().get(17..19) == Some(b"60");
        let mut normalized = Cow::Borrowed(input);
        if leap_second {
            normalized.to_mut().replace_range(17..19, "59");
        }
        // The legacy reader also accepted U+2212 as the offset's minus sign.
        if input.len() >= 8
            && input.as_bytes().get(input.len() - 8..input.len() - 5) == Some("−".as_bytes())
        {
            normalized
                .to_mut()
                .replace_range(input.len() - 8..input.len() - 5, "-");
        }
        let inner = OffsetDateTime::parse(&normalized, &Rfc3339)?
            .checked_to_offset(UtcOffset::UTC)
            .context("timestamp outside supported UTC range")?;
        Ok(Self { inner, leap_second })
    }

    pub(super) fn date(self) -> Date {
        self.inner.date()
    }

    pub(super) fn date_time(self) -> PlainDateTime {
        PlainDateTime::new(self.inner.date(), self.inner.time())
    }

    pub(super) fn from_utc(value: PlainDateTime) -> Self {
        Self {
            inner: value.assume_utc(),
            leap_second: false,
        }
    }

    pub(super) fn format(self, description: &[FormatItem<'_>]) -> String {
        if !self.leap_second {
            return self
                .inner
                .format(description)
                .expect("valid timestamp format");
        }
        description
            .iter()
            .map(|item| self.format_item(item))
            .collect()
    }

    fn format_item(self, item: &FormatItem<'_>) -> String {
        match item {
            FormatItem::Component(Component::Second(_)) => "60".to_owned(),
            FormatItem::Compound(items) => self.format(items),
            FormatItem::Optional(item) => self.format_item(item),
            FormatItem::First(items) => items
                .first()
                .map(|item| self.format_item(item))
                .unwrap_or_default(),
            _ => self.inner.format(item).expect("valid timestamp format"),
        }
    }

    pub(super) fn rfc3339_seconds(self) -> String {
        format!("{}Z", self.format(WIRE))
    }

    pub(super) fn rfc3339(self) -> String {
        format!("{}+00:00", self.fractional_timestamp())
    }

    fn fractional_timestamp(self) -> String {
        let mut value = self.format(WIRE);
        let nanos = self.inner.nanosecond();
        if nanos == 0 {
            return value;
        }
        let fraction = if nanos.is_multiple_of(1_000_000) {
            format!(".{:03}", nanos / 1_000_000)
        } else if nanos.is_multiple_of(1_000) {
            format!(".{:06}", nanos / 1_000)
        } else {
            format!(".{nanos:09}")
        };
        value.push_str(&fraction);
        value
    }

    pub(super) fn parse_stamp(value: &str) -> Option<Self> {
        if value.len() != 16
            || value.as_bytes()[8] != b'T'
            || value.as_bytes()[15] != b'Z'
            || !value
                .bytes()
                .enumerate()
                .all(|(i, byte)| i == 8 || i == 15 || byte.is_ascii_digit())
        {
            return None;
        }
        let input = format!(
            "{}-{}-{}T{}:{}:{}Z",
            &value[..4],
            &value[4..6],
            &value[6..8],
            &value[9..11],
            &value[11..13],
            &value[13..15]
        );
        let parsed = Self::parse(&input).ok()?;
        (parsed.format(ARCHIVE) == value).then_some(parsed)
    }

    // The old checkpoint deserializer accepted a relaxed RFC3339 grammar. Keep
    // that reader distinct from strict external timestamps, which reject these
    // extensions. Date validity and offset arithmetic remain time's responsibility.
    fn parse_checkpoint(input: &str) -> Result<Self> {
        if let Ok(value) = Self::parse(input) {
            return Ok(value);
        }
        let mut remaining = input.trim();
        let year_sign = match remaining.as_bytes().first() {
            Some(b'-') => {
                remaining = &remaining[1..];
                -1
            }
            Some(b'+') => {
                remaining = &remaining[1..];
                1
            }
            _ => 0,
        };
        let year = number(&mut remaining, if year_sign == 0 { 4 } else { 6 })?
            * if year_sign == -1 { -1 } else { 1 };
        ensure!(
            (-262_143..=262_142).contains(&year),
            "checkpoint year outside legacy range"
        );
        separator(&mut remaining, '-')?;
        let month = number(&mut remaining, 2)?;
        separator(&mut remaining, '-')?;
        let day = number(&mut remaining, 2)?;
        ensure!(
            matches!(remaining.as_bytes().first(), Some(b'T' | b't' | b' ')),
            "invalid checkpoint date/time separator"
        );
        remaining = &remaining[1..];
        let hour = number(&mut remaining, 2)?;
        separator(&mut remaining, ':')?;
        let minute = number(&mut remaining, 2)?;
        separator(&mut remaining, ':')?;
        let second = number(&mut remaining, 2)?;
        let mut nanosecond = 0;
        if let Some(fraction) = remaining.strip_prefix('.') {
            let count = fraction.bytes().take_while(u8::is_ascii_digit).count();
            ensure!(count > 0, "empty checkpoint fraction");
            let kept = count.min(9);
            nanosecond = fraction[..kept].parse::<u32>()? * 10_u32.pow((9 - kept) as u32);
            remaining = &fraction[count..];
        }
        let offset = checkpoint_offset(remaining.trim())?;
        let date = Date::from_calendar_date(year, Month::try_from(month as u8)?, day as u8)?;
        ensure!(second <= 60, "invalid checkpoint second");
        let time = Time::from_hms_nano(hour as u8, minute as u8, second.min(59) as u8, nanosecond)?;
        let inner = PlainDateTime::new(date, time)
            .assume_offset(offset)
            .checked_to_offset(UtcOffset::UTC)
            .context("checkpoint outside supported UTC range")?;
        ensure!(
            (-262_143..=262_142).contains(&inner.year()),
            "checkpoint UTC year outside legacy range"
        );
        Ok(Self {
            inner,
            leap_second: second == 60,
        })
    }

    fn ordering_key(self) -> (i64, u32) {
        (
            self.inner.unix_timestamp(),
            self.inner.nanosecond() + u32::from(self.leap_second) * 1_000_000_000,
        )
    }
}

fn number(input: &mut &str, max_digits: usize) -> Result<i32> {
    *input = input.trim_start();
    let count = input
        .bytes()
        .take(max_digits)
        .take_while(u8::is_ascii_digit)
        .count();
    ensure!(count > 0, "missing checkpoint date/time component");
    let value = input[..count].parse()?;
    *input = &input[count..];
    Ok(value)
}

fn separator(input: &mut &str, expected: char) -> Result<()> {
    *input = input
        .trim_start()
        .strip_prefix(expected)
        .context("invalid checkpoint separator")?;
    Ok(())
}

fn checkpoint_offset(value: &str) -> Result<UtcOffset> {
    if value.eq_ignore_ascii_case("z") || value.eq_ignore_ascii_case("utc") {
        return Ok(UtcOffset::UTC);
    }
    let (sign, rest) = if let Some(rest) = value.strip_prefix('+') {
        (1, rest)
    } else if let Some(rest) = value.strip_prefix('-').or_else(|| value.strip_prefix('−')) {
        (-1, rest)
    } else {
        anyhow::bail!("checkpoint has no UTC offset");
    };
    ensure!(
        rest.as_bytes()
            .get(..2)
            .is_some_and(|digits| digits.iter().all(u8::is_ascii_digit)),
        "invalid checkpoint offset hour"
    );
    let hours = rest[..2].parse::<i32>()?;
    let minutes = rest[2..].trim_start_matches(|c: char| c == ':' || c.is_whitespace());
    ensure!(
        minutes.len() == 2 && minutes.bytes().all(|c| c.is_ascii_digit()),
        "invalid checkpoint offset minute"
    );
    let minutes = minutes.parse::<i32>()?;
    ensure!(
        hours <= 23 && minutes <= 59,
        "invalid checkpoint UTC offset"
    );
    Ok(UtcOffset::from_whole_seconds(
        sign * (hours * 3_600 + minutes * 60),
    )?)
}

impl Default for Timestamp {
    fn default() -> Self {
        Self {
            inner: OffsetDateTime::UNIX_EPOCH,
            leap_second: false,
        }
    }
}

impl FromStr for Timestamp {
    type Err = anyhow::Error;
    fn from_str(value: &str) -> Result<Self> {
        Self::parse(value)
    }
}

impl Serialize for Timestamp {
    fn serialize<S: Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&format!("{}Z", self.fractional_timestamp()))
    }
}

impl<'de> Deserialize<'de> for Timestamp {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse_checkpoint(&value).map_err(serde::de::Error::custom)
    }
}

impl Ord for Timestamp {
    fn cmp(&self, other: &Self) -> Ordering {
        self.ordering_key().cmp(&other.ordering_key())
    }
}

impl PartialOrd for Timestamp {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Add<SignedDuration> for Timestamp {
    type Output = Self;
    fn add(self, duration: SignedDuration) -> Self {
        if self.leap_second {
            let fractional = i128::from(self.inner.nanosecond()) + duration.whole_nanoseconds();
            if (0..1_000_000_000).contains(&fractional) {
                return Self {
                    inner: self
                        .inner
                        .replace_nanosecond(fractional as u32)
                        .expect("leap fraction in range"),
                    leap_second: true,
                };
            }
            // Preserve the legacy elapsed-time rule: a duration leaving this
            // leap second counts it once, without introducing a leap calendar.
            let adjustment = if fractional < 0 {
                SignedDuration::seconds(1)
            } else {
                SignedDuration::ZERO
            };
            return Self {
                inner: self.inner + duration + adjustment,
                leap_second: false,
            };
        }
        Self {
            inner: self.inner + duration,
            leap_second: false,
        }
    }
}

impl AddAssign<SignedDuration> for Timestamp {
    fn add_assign(&mut self, duration: SignedDuration) {
        *self = *self + duration;
    }
}

impl Sub<SignedDuration> for Timestamp {
    type Output = Self;
    fn sub(self, duration: SignedDuration) -> Self {
        self + -duration
    }
}

impl Sub for Timestamp {
    type Output = SignedDuration;
    fn sub(self, other: Self) -> SignedDuration {
        // The previous implementation subtracts civil dates and times
        // separately. Its time-of-day difference counts a leap second crossed
        // between the operands, including when their dates are different.
        let crossed_leap =
            if self.inner.time().as_hms() > other.inner.time().as_hms() && other.leap_second {
                1
            } else if self.inner.time().as_hms() < other.inner.time().as_hms() && self.leap_second {
                -1
            } else {
                0
            };
        self.inner - other.inner
            + SignedDuration::seconds(
                i64::from(self.leap_second) - i64::from(other.leap_second) + crossed_leap,
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn time(input: &str) -> Timestamp {
        Timestamp::parse(input).unwrap()
    }

    #[test]
    fn legacy_wire_formats_keep_fraction_groups_and_utc_suffixes() {
        // Golden outputs from the previous chrono 0.4.45 implementation.
        for (fraction, expected) in [
            ("", ""),
            (".0", ""),
            (".1", ".100"),
            (".12", ".120"),
            (".1234", ".123400"),
            (".1234567", ".123456700"),
            (".123456789123", ".123456789"),
        ] {
            let timestamp = time(&format!("2026-11-21T15:00:00{fraction}+03:00"));
            assert_eq!(timestamp.rfc3339_seconds(), "2026-11-21T12:00:00Z");
            assert_eq!(
                timestamp.rfc3339(),
                format!("2026-11-21T12:00:00{expected}+00:00")
            );
            let encoded = serde_json::to_string(&timestamp).unwrap();
            assert_eq!(encoded, format!("\"2026-11-21T12:00:00{expected}Z\""));
            assert_eq!(
                serde_json::from_str::<Timestamp>(&encoded).unwrap(),
                timestamp
            );
        }
    }

    #[test]
    fn strict_parser_preserves_accepted_and_rejected_input_shapes() {
        for input in [
            "2026-11-21T12:00:00Z",
            "2026-11-21t12:00:00z",
            "2026-11-21 12:00:00+00:00",
            "2026-11-21T12:00:00-00:00",
            "2026-11-21T09:00:00−03:00",
        ] {
            assert_eq!(time(input), time("2026-11-21T12:00:00Z"));
        }
        for input in [
            "2026-11-21X12:00:00Z",
            " 2026-11-21T12:00:00Z",
            "2026-11-21T12:00:00Z ",
            "2026-11-21T12:00:00+0000",
            "2026-11-21T12:00:00 UTC",
            "2026-11-21T12:00:00",
            "2026-1-2T3:4:5Z",
            "2026-02-29T12:00:00Z",
            "2026-11-21T24:00:00Z",
            "2026-11-21T12:00:00+24:00",
            "2026−11-21T12:00:00Z",
            "+10000-01-01T00:00:00Z",
        ] {
            assert!(Timestamp::parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn checkpoint_reader_keeps_legacy_relaxed_inputs() {
        for input in [
            " 2026-11-21T12:00:00Z ",
            "2026-11-21T12:00:00+0000",
            "2026-11-21T12:00:00 UTC",
            "2026 -11-21T12:00:00Z",
            "2026-11-21T15:00:00+03 00",
            "2026-11-21T15:00:00−03:00",
        ] {
            let value: Timestamp = serde_json::from_value(serde_json::json!(input)).unwrap();
            let expected = if input.contains('−') {
                "2026-11-21T18:00:00Z"
            } else {
                "2026-11-21T12:00:00Z"
            };
            assert_eq!(value.rfc3339_seconds(), expected);
        }
        let value: Timestamp = serde_json::from_str("\"2026-1-2T3:4:5Z\"").unwrap();
        assert_eq!(value.rfc3339_seconds(), "2026-01-02T03:04:05Z");
        for input in [
            "+262142-12-31T23:59:59-00:01",
            "-262143-01-01T00:00:00+00:01",
        ] {
            assert!(serde_json::from_value::<Timestamp>(serde_json::json!(input)).is_err());
        }
    }

    #[test]
    fn leap_seconds_keep_identity_fraction_order_and_elapsed_rules() {
        let leap = time("2016-12-31T23:59:60.5Z");
        assert_eq!(leap.rfc3339_seconds(), "2016-12-31T23:59:60Z");
        assert_eq!(leap.rfc3339(), "2016-12-31T23:59:60.500+00:00");
        assert_eq!(
            serde_json::to_string(&leap).unwrap(),
            "\"2016-12-31T23:59:60.500Z\""
        );
        assert!(time("2016-12-31T23:59:59.999999999Z") < leap);
        assert!(leap < time("2017-01-01T00:00:00Z"));
        for (millis, expected) in [
            (-1_000, "2016-12-31T23:59:59.500+00:00"),
            (-500, "2016-12-31T23:59:60+00:00"),
            (0, "2016-12-31T23:59:60.500+00:00"),
            (500, "2017-01-01T00:00:00+00:00"),
            (1_000, "2017-01-01T00:00:00.500+00:00"),
        ] {
            assert_eq!(
                (leap + SignedDuration::milliseconds(millis)).rfc3339(),
                expected
            );
        }
        assert_eq!(
            (leap + SignedDuration::days(1)).rfc3339(),
            "2017-01-01T23:59:59.500+00:00"
        );
        assert_eq!(
            (leap - SignedDuration::days(1)).rfc3339(),
            "2016-12-31T00:00:00.500+00:00"
        );
        assert_eq!(
            (leap - time("2017-01-01T00:00:00Z")).whole_milliseconds(),
            500
        );
        // The old parser accepts :60 at any minute, without a leap-second table.
        assert_eq!(
            time("2026-11-21T12:59:60Z").rfc3339_seconds(),
            "2026-11-21T12:59:60Z"
        );
    }

    #[test]
    fn elapsed_time_keeps_legacy_leap_crossing_rules_across_dates() {
        for (left, right, nanoseconds) in [
            (
                "2016-12-31T03:00:00Z",
                "2016-12-31T02:59:60Z",
                1_000_000_000_i128,
            ),
            (
                "2016-12-31T03:00:60Z",
                "2016-12-31T02:59:60Z",
                61_000_000_000,
            ),
            (
                "2016-12-31T23:59:59.500Z",
                "2026-11-21T12:01:60.123400+23:59",
                -311_947_379_623_400_000,
            ),
        ] {
            assert_eq!((time(left) - time(right)).whole_nanoseconds(), nanoseconds);
            assert_eq!((time(right) - time(left)).whole_nanoseconds(), -nanoseconds);
        }
    }

    #[test]
    fn archives_are_exact_and_keep_leap_second_names() {
        for value in ["20261121T120000Z", "20161231T235960Z"] {
            assert_eq!(
                Timestamp::parse_stamp(value).unwrap().format(ARCHIVE),
                value
            );
        }
        for value in [
            "20261121t120000Z",
            "20261121T120000z",
            "20261121T120000Z ",
            "20260229T120000Z",
            "20261121T240000Z",
            "2026112T120000Z",
            "20261121T120000.1Z",
            "20261121T12é00Z",
        ] {
            assert!(Timestamp::parse_stamp(value).is_none(), "{value}");
        }
    }

    #[test]
    fn epoch_and_extended_utc_dates_keep_legacy_values() {
        assert_eq!(Timestamp::default(), time("1970-01-01T00:00:00Z"));
        assert_eq!(
            (time("1969-12-31T23:59:59.5Z") + SignedDuration::milliseconds(500)),
            Timestamp::default()
        );
        assert_eq!(
            time("0000-01-01T00:00:00+23:59").rfc3339_seconds(),
            "-0001-12-31T00:01:00Z"
        );
        assert_eq!(
            time("9999-12-31T23:59:59-23:59").rfc3339_seconds(),
            "+10000-01-01T23:58:59Z"
        );
        for value in ["-0001-01-01T00:00:00Z", "+10000-01-01T00:00:00Z"] {
            let parsed: Timestamp = serde_json::from_value(serde_json::json!(value)).unwrap();
            assert_eq!(
                serde_json::to_value(parsed).unwrap(),
                serde_json::json!(value)
            );
        }
    }
}
