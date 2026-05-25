use crate::errors::CoflParseError;
use crate::execute::CoflExecuteInstruction;
use crate::telemetry::CoflTelemetryUpdate;
use crate::text::{cofl_message_text, no_color_codes};
use saf_core::FlipEvent;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const LOGGED_OUT_SETTINGS_RECOVERY_COMMAND: &str = "/cofl s maxItemsInInventory 1";
const LOGGED_OUT_SETTINGS_WARNING: &str = "Until you do you are using the free version which will make less profit and your settings won't be saved";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoflSettingsSummary {
    pub min_profit: Option<String>,
    pub min_volume: Option<String>,
    pub min_profit_percent: Option<String>,
    pub max_flip_items_in_inventory: Option<String>,
    pub using: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CoflSettingsMutation {
    pub min_profit: Option<String>,
    pub min_profit_percent: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoflEnvelope {
    #[serde(rename = "type", alias = "kind")]
    pub kind: String,
    #[serde(default)]
    pub data: Value,
}

impl CoflEnvelope {
    pub fn from_wire(raw: impl AsRef<[u8]>) -> Result<Self, CoflParseError> {
        let value: Value = serde_json::from_slice(raw.as_ref())?;
        let mut envelope: Self = serde_json::from_value(value)?;
        if let Value::String(data) = &envelope.data
            && let Ok(parsed) = serde_json::from_str::<Value>(data)
        {
            envelope.data = parsed;
        }
        Ok(envelope)
    }

    pub fn parse_flip(&self) -> Option<FlipEvent> {
        matches!(self.kind.as_str(), "flip" | "newFlip" | "auction")
            .then(|| FlipEvent::from_payload(&self.data))
    }

    pub fn telemetry_update(&self) -> Option<CoflTelemetryUpdate> {
        let text = cofl_message_text(&self.data)?;
        CoflTelemetryUpdate::from_message(&text)
    }

    pub fn settings_summary(&self) -> Option<CoflSettingsSummary> {
        let text = no_color_codes(&cofl_message_text(&self.data)?);
        CoflSettingsSummary::from_message(&text)
    }

    pub fn settings_json_summary(&self) -> Option<CoflSettingsSummary> {
        (self.kind == "settings")
            .then(|| CoflSettingsSummary::from_json(&self.data))
            .flatten()
    }

    pub fn settings_json_available(&self) -> bool {
        self.kind == "settings" && !self.data.is_null()
    }

    pub fn settings_mutation(&self) -> Option<CoflSettingsMutation> {
        matches!(self.kind.as_str(), "writeToChat" | "chatMessage").then_some(())?;
        let text = no_color_codes(&cofl_message_text(&self.data)?);
        let mutation = CoflSettingsMutation {
            min_profit: value_after_case_insensitive(&text, "set minprofit to"),
            min_profit_percent: value_after_case_insensitive(&text, "set minprofitpercent to"),
        };
        mutation.has_any_value().then_some(mutation)
    }

    pub fn settings_unavailable(&self) -> bool {
        matches!(
            self.passive_chat_category(),
            Some("settings_load_failed" | "logged_out_settings_warning" | "free_status")
        )
    }

    pub fn account_info_acceleration_ack(&self) -> bool {
        cofl_message_text(&self.data)
            .map(|text| no_color_codes(&text))
            .is_some_and(|text| text.contains("received account info, starting to speed up flips"))
    }

    pub fn execute_instruction(
        &self,
        ign: &str,
        session_id: &str,
    ) -> Option<CoflExecuteInstruction> {
        (self.kind == "execute")
            .then(|| cofl_message_text(&self.data))
            .flatten()
            .and_then(|command| CoflExecuteInstruction::from_command(&command, ign, session_id))
    }

    pub fn is_inventory_request(&self) -> bool {
        self.kind == "getInventory"
    }

    pub fn privacy_chat_regex(&self) -> Option<String> {
        (self.kind == "privacySettings")
            .then(|| privacy_chat_regex_from_data(&self.data))
            .flatten()
    }

    pub fn logged_out_settings_recovery_command(&self) -> Option<&'static str> {
        cofl_message_text(&self.data)
            .map(|text| no_color_codes(&text))
            .filter(|text| text.contains(LOGGED_OUT_SETTINGS_WARNING))
            .map(|_| LOGGED_OUT_SETTINGS_RECOVERY_COMMAND)
    }

    pub fn passive_chat_category(&self) -> Option<&'static str> {
        matches!(self.kind.as_str(), "writeToChat" | "chatMessage").then_some(())?;
        let Some(text) = cofl_message_text(&self.data).map(|text| no_color_codes(&text)) else {
            return Some("non_text");
        };
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Some("empty");
        }
        let normalized = trimmed.to_ascii_lowercase();
        if normalized.contains("matched your whitelist") {
            return Some("matched_whitelist");
        }
        if normalized.contains("there was no ah price found for") {
            return Some("no_ah_price");
        }
        if normalized.contains("reached max flip items in inventory") {
            return Some("max_flip_items_in_inventory");
        }
        if normalized.contains("your settings blocked ") {
            return Some("settings_blocked");
        }
        if normalized.contains("your settings could not be loaded") {
            return Some("settings_load_failed");
        }
        if normalized.contains("you are executing too many commands") {
            return Some("command_throttled");
        }
        if normalized.contains("an error occured while processing your command")
            || normalized.contains("an error occurred while processing your command")
            || (normalized.contains("the setting ") && normalized.contains(" doesn't exist"))
        {
            return Some("command_error");
        }
        if normalized.contains("received account info, starting to speed up flips") {
            return Some("account_info_ack");
        }
        if normalized.contains("found and loaded settings for your connection") {
            return Some("settings_loaded");
        }
        if normalized.contains("the time to receive flips is estimated to be ") {
            return Some("ping_estimate");
        }
        if normalized.contains("you are currently delayed by ")
            || normalized.contains("you are currently not delayed at all")
        {
            return Some("delay_status");
        }
        if normalized.contains("you have ") && normalized.contains(" until ") {
            return Some("premium_status");
        }
        if normalized.contains("you use the free version of the flip finder") {
            return Some("free_status");
        }
        if normalized.contains(&LOGGED_OUT_SETTINGS_WARNING.to_ascii_lowercase()) {
            return Some("logged_out_settings_warning");
        }
        if !self.auth_links().is_empty() {
            return Some("auth_link");
        }
        Some("unknown_text")
    }

    pub fn passive_chat_excerpt(&self, max_chars: usize) -> Option<String> {
        matches!(self.kind.as_str(), "writeToChat" | "chatMessage").then_some(())?;
        let text = cofl_message_text(&self.data)?;
        Some(sanitized_message_excerpt(&no_color_codes(&text), max_chars))
    }

    pub fn auth_links(&self) -> Vec<String> {
        crate::auth_link::cofl_auth_links(&self.data)
    }
}

fn sanitized_message_excerpt(text: &str, max_chars: usize) -> String {
    let mut excerpt = String::new();
    for token in text.split_whitespace() {
        let token = if should_redact_message_token(token) {
            "[redacted]"
        } else {
            token
        };
        if !excerpt.is_empty() {
            excerpt.push(' ');
        }
        excerpt.push_str(token);
        if excerpt.chars().count() >= max_chars {
            return truncate_chars(&excerpt, max_chars);
        }
    }
    truncate_chars(&excerpt, max_chars)
}

fn should_redact_message_token(token: &str) -> bool {
    let cleaned = token.trim_matches(|character: char| {
        matches!(
            character,
            ',' | '.' | ';' | ':' | ')' | '(' | '[' | ']' | '"' | '\''
        )
    });
    if cleaned.starts_with("http://")
        || cleaned.starts_with("https://")
        || cleaned.contains("SId=")
        || cleaned.contains("session=")
        || cleaned.contains("conId=")
    {
        return true;
    }
    cleaned.len() >= 24
        && cleaned
            .chars()
            .all(|character| character.is_ascii_hexdigit())
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

impl CoflSettingsSummary {
    fn from_message(message: &str) -> Option<Self> {
        message
            .contains("Found and loaded settings for your connection")
            .then(|| Self {
                min_profit: value_after_marker(message, "MinProfit:"),
                min_volume: value_after_marker(message, "MinVolume:"),
                min_profit_percent: value_after_marker(message, "MinProfitPercent:"),
                max_flip_items_in_inventory: None,
                using: phrase_after_marker(message, "Using:"),
            })
    }

    fn from_json(data: &Value) -> Option<Self> {
        let summary = match data {
            Value::Object(object) => Self {
                min_profit: setting_value(object, &["MinProfit", "minProfit"]),
                min_volume: setting_value(object, &["MinVolume", "minVolume"]),
                min_profit_percent: setting_value(
                    object,
                    &["MinProfitPercent", "minProfitPercent"],
                ),
                max_flip_items_in_inventory: setting_value(
                    object,
                    &[
                        "MaxFlipItemsInInventory",
                        "maxFlipItemsInInventory",
                        "MaxItemsInInventory",
                        "maxItemsInInventory",
                    ],
                ),
                using: setting_value(object, &["Using", "using", "Profile", "profile"]),
            },
            Value::Array(rows) => Self {
                min_profit: setting_row_value(rows, &["MinProfit", "minProfit"]),
                min_volume: setting_row_value(rows, &["MinVolume", "minVolume"]),
                min_profit_percent: setting_row_value(
                    rows,
                    &["MinProfitPercent", "minProfitPercent"],
                ),
                max_flip_items_in_inventory: setting_row_value(
                    rows,
                    &[
                        "MaxFlipItemsInInventory",
                        "maxFlipItemsInInventory",
                        "MaxItemsInInventory",
                        "maxItemsInInventory",
                    ],
                ),
                using: setting_row_value(rows, &["Using", "using", "Profile", "profile"]),
            },
            _ => return None,
        };
        summary.has_any_value().then_some(summary)
    }

    fn has_any_value(&self) -> bool {
        self.min_profit.is_some()
            || self.min_volume.is_some()
            || self.min_profit_percent.is_some()
            || self.max_flip_items_in_inventory.is_some()
            || self.using.is_some()
    }
}

impl CoflSettingsMutation {
    fn has_any_value(&self) -> bool {
        self.min_profit.is_some() || self.min_profit_percent.is_some()
    }
}

fn setting_value(object: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| object.get(*key))
        .and_then(scalar_setting_value)
}

fn setting_row_value(rows: &[Value], keys: &[&str]) -> Option<String> {
    rows.iter()
        .filter_map(Value::as_object)
        .find(|row| {
            keys.iter().any(|key| {
                row.get("key")
                    .or_else(|| row.get("name"))
                    .and_then(Value::as_str)
                    .is_some_and(|value| value.eq_ignore_ascii_case(key))
            })
        })
        .and_then(|row| row.get("value"))
        .and_then(scalar_setting_value)
}

fn scalar_setting_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        }
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_after_marker(message: &str, marker: &str) -> Option<String> {
    message
        .split_once(marker)?
        .1
        .split_whitespace()
        .next()
        .map(|value| value.trim_matches(|ch: char| ch == ',' || ch == ';'))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn value_after_case_insensitive(message: &str, marker: &str) -> Option<String> {
    let normalized = message.to_ascii_lowercase();
    let marker = marker.to_ascii_lowercase();
    let start = normalized.find(&marker)? + marker.len();
    message[start..]
        .split_whitespace()
        .next()
        .map(|value| value.trim_matches(|ch: char| ch == ',' || ch == ';' || ch == '.'))
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn phrase_after_marker(message: &str, marker: &str) -> Option<String> {
    let rest = message.split_once(marker)?.1.trim();
    let end = ["\n:", "\\n:", " :", "\n", " nothing else"]
        .into_iter()
        .filter_map(|delimiter| rest.find(delimiter))
        .min()
        .unwrap_or(rest.len());
    let phrase = rest[..end].trim();
    (!phrase.is_empty()).then(|| phrase.to_string())
}

fn privacy_chat_regex_from_data(data: &Value) -> Option<String> {
    match data {
        Value::Object(object) => object
            .get("chatRegex")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        Value::String(raw) => serde_json::from_str::<Value>(raw)
            .ok()
            .and_then(|parsed| privacy_chat_regex_from_data(&parsed)),
        _ => None,
    }
}
