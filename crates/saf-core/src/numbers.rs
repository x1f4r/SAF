use serde_json::Value;

pub fn normal_number(value: impl Into<NumberInput>) -> f64 {
    parse_number_input(value.into()).unwrap_or(f64::NAN)
}

pub fn parse_number_input(input: NumberInput) -> Option<f64> {
    match input {
        NumberInput::Number(value) => value.is_finite().then_some(value),
        NumberInput::String(value) => parse_compact_number(&value),
        NumberInput::Json(value) => match value {
            Value::Number(number) => number.as_f64(),
            Value::String(value) => parse_compact_number(&value),
            _ => None,
        },
    }
}

pub fn parse_compact_number(raw: &str) -> Option<f64> {
    let compact = raw.trim().replace([',', '_'], "").to_ascii_lowercase();
    if compact.is_empty() {
        return None;
    }

    let (number, multiplier) = match compact.chars().last()? {
        'k' => (&compact[..compact.len() - 1], 1_000.0),
        'm' => (&compact[..compact.len() - 1], 1_000_000.0),
        'b' => (&compact[..compact.len() - 1], 1_000_000_000.0),
        't' => (&compact[..compact.len() - 1], 1_000_000_000_000.0),
        _ => (compact.as_str(), 1.0),
    };

    let value = number.trim().parse::<f64>().ok()?;
    value.is_finite().then_some(value * multiplier)
}

pub fn add_commas_to_number(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string();
    }

    let sign = if value < 0.0 { "-" } else { "" };
    let whole = value.abs().trunc() as u128;
    let mut digits = whole.to_string();
    let mut grouped = String::new();

    while digits.len() > 3 {
        let rest = digits.split_off(digits.len() - 3);
        if grouped.is_empty() {
            grouped = rest;
        } else {
            grouped = format!("{rest},{grouped}");
        }
    }

    if grouped.is_empty() {
        format!("{sign}{digits}")
    } else {
        format!("{sign}{digits},{grouped}")
    }
}

pub enum NumberInput {
    Number(f64),
    String(String),
    Json(Value),
}

impl From<f64> for NumberInput {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<i64> for NumberInput {
    fn from(value: i64) -> Self {
        Self::Number(value as f64)
    }
}

impl From<&str> for NumberInput {
    fn from(value: &str) -> Self {
        Self::String(value.to_string())
    }
}

impl From<String> for NumberInput {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<Value> for NumberInput {
    fn from(value: Value) -> Self {
        Self::Json(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_existing_suffix_semantics() {
        assert_eq!(parse_compact_number("25m"), Some(25_000_000.0));
        assert_eq!(parse_compact_number("1.5b"), Some(1_500_000_000.0));
        assert_eq!(parse_compact_number("1,234,567"), Some(1_234_567.0));
        assert_eq!(parse_compact_number(""), None);
    }
}
