use crate::item_metadata::text_to_plain;
use crate::numbers::parse_compact_number;
use regex::Regex;
use serde_json::Value;

pub fn no_color_codes(text: &str) -> String {
    let mut cleaned = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '§' {
            chars.next();
            continue;
        }
        cleaned.push(ch);
    }
    cleaned
}

pub fn clean_scoreboard_line(text: &str) -> String {
    no_color_codes(text)
        .chars()
        .filter(|ch| !matches!(ch, '\u{200B}' | '\u{200C}' | '\u{200D}' | '\u{FEFF}'))
        .collect::<String>()
        .trim()
        .to_string()
}

pub fn clean_scoreboard_lines(lines: &[String]) -> Vec<String> {
    let mut cleaned = Vec::new();
    for line in lines {
        let line = clean_scoreboard_line(line);
        if line.is_empty() || cleaned.iter().any(|known| known == &line) {
            continue;
        }
        cleaned.push(line);
    }
    cleaned
}

pub fn message_text(value: &Value) -> String {
    text_to_plain(value).unwrap_or_else(|| value.to_string())
}

pub fn is_game_message_type(kind: Option<&str>) -> bool {
    matches!(kind, None | Some("chat") | Some("system"))
}

pub fn parse_purse_from_scoreboard_lines(lines: &[Value]) -> Option<u64> {
    let pattern = Regex::new(r"(?i)\b(?:Purse|Piggy):\s*([\d,.]+)\s*([kmbt])?").ok()?;
    for line in lines {
        let text = text_to_plain(line).map(|text| no_color_codes(&text))?;
        let Some(captures) = pattern.captures(&text) else {
            continue;
        };
        let number = captures.get(1)?.as_str();
        let suffix = captures.get(2).map(|suffix| suffix.as_str()).unwrap_or("");
        let compact = format!("{number}{suffix}");
        if let Some(value) = parse_compact_number(&compact) {
            return Some(value.round() as u64);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn message_helpers_support_modern_system_messages() {
        assert_eq!(
            message_text(&json!({ "text": "Putting coins in escrow..." })),
            "Putting coins in escrow..."
        );
        assert!(is_game_message_type(Some("chat")));
        assert!(is_game_message_type(Some("system")));
        assert!(!is_game_message_type(Some("game_info")));
    }

    #[test]
    fn purse_parser_handles_components_and_suffixes() {
        assert_eq!(
            parse_purse_from_scoreboard_lines(&[json!({ "text": "§6Purse: §e123,456,789" })]),
            Some(123_456_789)
        );
        assert_eq!(
            parse_purse_from_scoreboard_lines(&[json!({ "text": "§6Purse: §e3.2B" })]),
            Some(3_200_000_000)
        );
        assert_eq!(
            parse_purse_from_scoreboard_lines(&[json!({
                "type": "compound",
                "value": { "text": { "type": "string", "value": "Piggy: 9,876" } }
            })]),
            Some(9_876)
        );
        assert_eq!(
            parse_purse_from_scoreboard_lines(&[json!("Bits: 200")]),
            None
        );
    }

    #[test]
    fn scoreboard_cleaning_matches_node_upload_shape() {
        let lines = clean_scoreboard_lines(&[
            "§6Purse: §e123,456".to_string(),
            "\u{200B}Your Island\u{200C}".to_string(),
            "Your Island".to_string(),
            "   ".to_string(),
            "Bits: 100".to_string(),
        ]);
        assert_eq!(
            lines,
            vec![
                "Purse: 123,456".to_string(),
                "Your Island".to_string(),
                "Bits: 100".to_string()
            ]
        );
    }
}
