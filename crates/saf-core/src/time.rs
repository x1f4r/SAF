use crate::protocol_text::no_color_codes;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub fn normal_time(raw: &str) -> Option<Duration> {
    let value = no_color_codes(raw).trim().to_ascii_lowercase();
    if value.is_empty() {
        return None;
    }

    if let Ok(ms) = value.parse::<u64>() {
        return Some(Duration::from_millis(ms));
    }

    let bytes = value.as_bytes();
    let mut total_ms = 0.0;
    let mut saw_component = false;
    let mut seen_units = [false; 5];
    let mut index = 0;

    while index < bytes.len() {
        if !bytes[index].is_ascii_digit() {
            index += 1;
            continue;
        }

        let start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index < bytes.len() && bytes[index] == b'.' {
            let dot = index;
            index += 1;
            let decimal_start = index;
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if decimal_start == index {
                index = dot + 1;
                continue;
            }
        }

        let mut unit_index = index;
        while unit_index < bytes.len() && bytes[unit_index].is_ascii_whitespace() {
            unit_index += 1;
        }
        let Some(unit) = bytes.get(unit_index).map(|unit| *unit as char) else {
            index = start + 1;
            continue;
        };
        let Some((seen_index, multiplier)) = duration_unit(unit) else {
            index = start + 1;
            continue;
        };
        if !seen_units[seen_index] {
            let number = value[start..index].parse::<f64>().ok()?;
            if !number.is_finite() || number < 0.0 {
                return None;
            }
            total_ms += number * multiplier;
            seen_units[seen_index] = true;
            saw_component = true;
        }
        index = unit_index + 1;
    }

    saw_component.then(|| Duration::from_millis(total_ms.round() as u64))
}

fn duration_unit(unit: char) -> Option<(usize, f64)> {
    match unit {
        'y' => Some((0, 31_540_000_000_f64)),
        'd' => Some((1, 86_400_000_f64)),
        'h' => Some((2, 3_600_000_f64)),
        'm' => Some((3, 60_000_f64)),
        's' => Some((4, 1_000_f64)),
        _ => None,
    }
}

pub fn duration_to_hours(duration: Duration) -> f64 {
    duration.as_secs_f64() / 3600.0
}

pub fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

pub fn parse_timestamp_millis(raw: &str) -> Option<u64> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }

    if let Ok(timestamp) = value.parse::<u64>() {
        return Some(timestamp);
    }

    parse_utc_datetime(value)
}

pub fn format_timestamp_millis(timestamp: u64) -> String {
    let seconds = (timestamp / 1_000) as i64;
    let millis = timestamp % 1_000;
    let days = seconds.div_euclid(86_400);
    let seconds_of_day = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = seconds_of_day / 3_600;
    let minute = (seconds_of_day % 3_600) / 60;
    let second = seconds_of_day % 60;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}.{millis:03}Z")
}

fn parse_utc_datetime(value: &str) -> Option<u64> {
    let normalized = value.trim().trim_end_matches('Z').replace('T', " ");
    let (date, time) = normalized
        .split_once(' ')
        .map_or((normalized.as_str(), "00:00:00"), |(date, time)| {
            (date.trim(), time.trim())
        });
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i32>().ok()?;
    let month = date_parts.next()?.parse::<u32>().ok()?;
    let day = date_parts.next()?.parse::<u32>().ok()?;
    if date_parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let mut time_parts = time.split(':');
    let hour = time_parts.next().unwrap_or("0").parse::<u32>().ok()?;
    let minute = time_parts.next().unwrap_or("0").parse::<u32>().ok()?;
    let second_text = time_parts.next().unwrap_or("0");
    if time_parts.next().is_some() || hour > 23 || minute > 59 {
        return None;
    }
    let (second_text, millis_text) = second_text
        .split_once('.')
        .map_or((second_text, ""), |(seconds, millis)| (seconds, millis));
    let second = second_text.parse::<u32>().ok()?;
    if second > 59 {
        return None;
    }
    let millis = if millis_text.is_empty() {
        0
    } else {
        let digits = millis_text
            .chars()
            .take_while(|ch| ch.is_ascii_digit())
            .take(3)
            .collect::<String>();
        if digits.is_empty() {
            0
        } else {
            format!("{digits:0<3}").parse::<u32>().ok()?
        }
    };

    let days = days_from_civil(year, month, day)?;
    let seconds = days
        .checked_mul(86_400)?
        .checked_add((hour as i64) * 3_600)?
        .checked_add((minute as i64) * 60)?
        .checked_add(second as i64)?;
    if seconds < 0 {
        return None;
    }
    Some((seconds as u64) * 1_000 + millis as u64)
}

fn days_from_civil(year: i32, month: u32, day: u32) -> Option<i64> {
    if day > days_in_month(year, month) {
        return None;
    }
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = month as i32;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day as i32 - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    Some((era * 146_097 + day_of_era - 719_468) as i64)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (year as i32, month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_duration_suffixes() {
        assert_eq!(normal_time("15s"), Some(Duration::from_secs(15)));
        assert_eq!(normal_time("1.5h"), Some(Duration::from_secs(5400)));
        assert_eq!(normal_time("1h 30m"), Some(Duration::from_secs(5400)));
        assert_eq!(normal_time("1h30m"), Some(Duration::from_secs(5400)));
        assert_eq!(normal_time("§a1h §b30m"), Some(Duration::from_secs(5400)));
        assert_eq!(duration_to_hours(normal_time("48h").unwrap()), 48.0);
        assert_eq!(normal_time("soon"), None);
    }

    #[test]
    fn parses_node_style_expiry_dates() {
        assert_eq!(
            parse_timestamp_millis("2030-01-01"),
            Some(1_893_456_000_000)
        );
        assert_eq!(
            parse_timestamp_millis("2030-01-01T00:00:00.250Z"),
            Some(1_893_456_000_250)
        );
        assert_eq!(parse_timestamp_millis("2030-02-30"), None);
        assert_eq!(
            format_timestamp_millis(1_893_456_000_250),
            "2030-01-01T00:00:00.250Z"
        );
    }
}
