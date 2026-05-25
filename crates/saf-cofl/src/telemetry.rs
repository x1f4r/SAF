use crate::text::no_color_codes;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoflTelemetryUpdate {
    pub connection_id: Option<String>,
    pub cofl_ping_ms: Option<u64>,
    pub cofl_delay_ms: Option<u64>,
    pub cofl_tier: Option<String>,
    pub cofl_expires_at: Option<u64>,
}

impl CoflTelemetryUpdate {
    pub fn from_message(message: &str) -> Option<Self> {
        let cleaned = no_color_codes(message);
        let connection_id = parse_connection_id(&cleaned);
        let cofl_ping_ms = parse_cofl_ping_ms(&cleaned);
        let cofl_delay_ms = parse_cofl_delay_ms(&cleaned);
        let account_info = parse_account_info(&cleaned);
        (connection_id.is_some()
            || cofl_ping_ms.is_some()
            || cofl_delay_ms.is_some()
            || account_info.is_some())
        .then(|| {
            let (cofl_tier, cofl_expires_at) = account_info.unwrap_or_default();
            Self {
                connection_id,
                cofl_ping_ms,
                cofl_delay_ms,
                cofl_tier,
                cofl_expires_at,
            }
        })
    }
}

fn parse_connection_id(text: &str) -> Option<String> {
    const MARKER: &str = "Your connection id is ";
    let rest = text.split_once(MARKER)?.1;
    rest.split(|character: char| !character.is_ascii_hexdigit())
        .find(|token| token.len() == 32)
        .map(ToOwned::to_owned)
}

fn parse_cofl_ping_ms(text: &str) -> Option<u64> {
    const PREFIX: &str = "The time to receive flips is estimated to be ";
    const SUFFIX: &str = "ms";
    let rest = text.split_once(PREFIX)?.1;
    let raw = rest.split_once(SUFFIX)?.0.trim();
    parse_positive_number(raw).map(|value| value.round() as u64)
}

fn parse_cofl_delay_ms(text: &str) -> Option<u64> {
    if text.contains("You are currently not delayed at all") {
        return Some(0);
    }
    const PREFIX: &str = "You are currently delayed by ";
    const SUFFIX: &str = "s on api";
    let rest = text.split_once(PREFIX)?.1;
    let raw = rest.split_once(SUFFIX)?.0.trim();
    parse_positive_number(raw).map(|value| (value * 1_000.0).round() as u64)
}

fn parse_account_info(text: &str) -> Option<(Option<String>, Option<u64>)> {
    if text.contains("You use the FREE version of the flip finder") {
        return Some((Some("Free".to_string()), None));
    }
    let normalized = text.to_ascii_lowercase();
    if normalized.contains("coflnet prem+ instance")
        || normalized.contains("coflnet premium plus instance")
    {
        return Some((Some("Premium Plus".to_string()), None));
    }
    if normalized.contains("coflnet premium instance") {
        return Some((Some("Premium".to_string()), None));
    }

    let rest = text.split_once("You have ")?.1;
    let (tier, expires_at) = rest.rsplit_once(" until ")?;
    let tier = normalize_account_tier(tier.trim());
    let expires_at = parse_cofl_expiry_epoch(expires_at.trim());
    Some((Some(tier), expires_at))
}

fn normalize_account_tier(tier: &str) -> String {
    match tier.trim() {
        "PREMIUM PLUS" => "Premium Plus".to_string(),
        "PREMIUM" => "Premium".to_string(),
        other => other.to_string(),
    }
}

fn parse_cofl_expiry_epoch(raw: &str) -> Option<u64> {
    let mut parts = raw.split_whitespace();
    let date = parts.next()?;
    let time = parts.next()?;
    let zone = parts.next()?;
    if !zone.eq_ignore_ascii_case("UTC") || parts.next().is_some() {
        return None;
    }

    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i32>().ok()?;
    let month = month_number(date_parts.next()?)?;
    let day = date_parts.next()?.parse::<u32>().ok()?;
    if date_parts.next().is_some() {
        return None;
    }

    let mut time_parts = time.split(':');
    let hour = time_parts.next()?.parse::<u32>().ok()?;
    let minute = time_parts.next()?.parse::<u32>().ok()?;
    if time_parts.next().is_some()
        || !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
    {
        return None;
    }

    let days = days_from_civil(year, month as i32, day as i32)?;
    Some((days * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60) as u64)
}

fn month_number(month: &str) -> Option<u32> {
    match month.to_ascii_lowercase().as_str() {
        "jan" => Some(1),
        "feb" => Some(2),
        "mar" => Some(3),
        "apr" => Some(4),
        "may" => Some(5),
        "jun" => Some(6),
        "jul" => Some(7),
        "aug" => Some(8),
        "sep" => Some(9),
        "oct" => Some(10),
        "nov" => Some(11),
        "dec" => Some(12),
        _ => None,
    }
}

fn days_from_civil(year: i32, month: i32, day: i32) -> Option<i64> {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    let days = era * 146_097 + day_of_era - 719_468;
    (days >= 0).then_some(i64::from(days))
}

fn parse_positive_number(raw: &str) -> Option<f64> {
    let value = raw.replace(',', "").parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}
