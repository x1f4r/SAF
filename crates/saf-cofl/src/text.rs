use saf_core::item_metadata::text_to_plain;
use serde_json::Value;

pub(crate) fn cofl_message_text(data: &Value) -> Option<String> {
    match data {
        Value::String(text) => Some(text.clone()),
        Value::Array(values) => {
            let text = values
                .iter()
                .filter_map(cofl_message_text)
                .collect::<Vec<_>>()
                .join(" ");
            (!text.trim().is_empty()).then_some(text)
        }
        Value::Object(_) => text_to_plain(data),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Null => None,
    }
}

pub(crate) fn no_color_codes(text: &str) -> String {
    let mut cleaned = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '§' {
            chars.next();
        } else {
            cleaned.push(ch);
        }
    }
    cleaned
}
