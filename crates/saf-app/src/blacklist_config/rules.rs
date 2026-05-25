use saf_core::ports::PortError;
use saf_core::time::{format_timestamp_millis, normal_time, parse_timestamp_millis, unix_millis};
use saf_core::{BlacklistField, BlacklistUpdate, Enchantment};
use serde_json::{Map, Value, json};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum RuleValue {
    Scalar(String),
    ItemEnchant(ItemEnchantRule),
}

impl RuleValue {
    pub(super) fn key(&self, field: &BlacklistField) -> String {
        match self {
            Self::Scalar(value) => scalar_key(field, value),
            Self::ItemEnchant(rule) => rule.key(),
        }
    }

    pub(super) fn to_entry(&self, expires_at: Option<u64>) -> Value {
        match self {
            Self::Scalar(value) => {
                if let Some(expires_at) = expires_at {
                    json!({
                        "value": value,
                        "expiresAt": format_timestamp_millis(expires_at),
                    })
                } else {
                    Value::String(value.clone())
                }
            }
            Self::ItemEnchant(rule) => {
                let mut object = rule.to_map();
                if let Some(expires_at) = expires_at {
                    object.insert(
                        "expiresAt".to_string(),
                        Value::String(format_timestamp_millis(expires_at)),
                    );
                }
                Value::Object(object)
            }
        }
    }

    pub(super) fn display(&self) -> String {
        match self {
            Self::Scalar(value) => value.clone(),
            Self::ItemEnchant(rule) => rule.display(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ItemEnchantRule {
    tag: Option<String>,
    name: Option<String>,
    enchantment: Enchantment,
}

impl ItemEnchantRule {
    fn to_map(&self) -> Map<String, Value> {
        let mut object = Map::new();
        if let Some(tag) = &self.tag {
            object.insert("tag".to_string(), Value::String(tag.clone()));
        }
        if let Some(name) = &self.name {
            object.insert("name".to_string(), Value::String(name.clone()));
        }
        object.insert(
            "enchantment".to_string(),
            Value::String(self.enchantment.id.clone()),
        );
        if let Some(level) = self.enchantment.level {
            object.insert("level".to_string(), Value::from(level));
        }
        object
    }

    fn key(&self) -> String {
        let item = self
            .tag
            .as_deref()
            .map(|tag| format!("tag:{tag}"))
            .or_else(|| {
                self.name
                    .as_deref()
                    .map(|name| format!("name:{}", normalize_name(name)))
            })
            .unwrap_or_else(|| "item:*".to_string());
        format!("{item}|{}", enchant_key(&self.enchantment))
    }

    fn display(&self) -> String {
        let item = self
            .tag
            .as_deref()
            .or(self.name.as_deref())
            .unwrap_or("item");
        format!("{item} {}", enchant_key(&self.enchantment))
    }
}

pub(super) fn normalize_update_value(
    field: &BlacklistField,
    value: &str,
) -> Result<RuleValue, PortError> {
    match field {
        BlacklistField::Tag => Ok(RuleValue::Scalar(normalize_tag(value))),
        BlacklistField::Name => Ok(RuleValue::Scalar(value.trim().to_string())),
        BlacklistField::Enchant => parse_enchant_spec(value)
            .map(|enchantment| RuleValue::Scalar(enchant_key(&enchantment)))
            .ok_or_else(|| PortError::Failed("missing blacklist enchantment".to_string())),
        BlacklistField::ItemEnchant => parse_item_enchant_rule(&Value::String(value.to_string()))
            .map(RuleValue::ItemEnchant)
            .ok_or_else(|| PortError::Failed("missing item/enchantment rule".to_string())),
    }
}

pub(super) fn update_expires_at(update: &BlacklistUpdate) -> Result<Option<u64>, PortError> {
    let Some(raw) = update.duration.as_ref().or(update.until.as_ref()) else {
        return Ok(None);
    };
    if let Some(duration) = normal_time(raw) {
        return Ok(Some(unix_millis() + duration.as_millis() as u64));
    }
    parse_timestamp_millis(raw)
        .map(Some)
        .ok_or_else(|| PortError::Failed(format!("invalid blacklist expiry {raw}")))
}

pub(super) fn entry_expired(entry: &Value, now: u64) -> bool {
    entry_expires_at(entry).is_some_and(|expires_at| expires_at <= now)
}

pub(super) fn entry_expires_at(entry: &Value) -> Option<u64> {
    let object = entry.as_object()?;
    object
        .get("expiresAt")
        .or_else(|| object.get("expires"))
        .or_else(|| object.get("until"))
        .and_then(|value| match value {
            Value::Number(number) => number.as_u64(),
            Value::String(value) => parse_timestamp_millis(value),
            _ => None,
        })
}

pub(super) fn entry_key(field: &BlacklistField, entry: &Value) -> Option<String> {
    match field {
        BlacklistField::ItemEnchant => {
            parse_item_enchant_rule(&unwrap_entry(entry)).map(|rule| rule.key())
        }
        _ => value_text(&unwrap_entry(entry)).map(|value| scalar_key(field, &value)),
    }
}

pub(super) fn unwrap_entry(entry: &Value) -> Value {
    let Some(object) = entry.as_object() else {
        return entry.clone();
    };
    if let Some(value) = object.get("value") {
        return value.clone();
    }
    let mut rule = object.clone();
    for key in ["expiresAt", "expires", "until", "createdAt"] {
        rule.remove(key);
    }
    Value::Object(rule)
}

fn scalar_key(field: &BlacklistField, value: &str) -> String {
    match field {
        BlacklistField::Tag => normalize_tag(value),
        BlacklistField::Name => normalize_name(value),
        BlacklistField::Enchant => parse_enchant_spec(value)
            .map(|enchantment| enchant_key(&enchantment))
            .unwrap_or_else(|| normalize_enchant_id(value)),
        BlacklistField::ItemEnchant => value.to_string(),
    }
}

fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn parse_item_enchant_rule(value: &Value) -> Option<ItemEnchantRule> {
    match value {
        Value::Object(object) => {
            let enchantment = object
                .get("enchantment")
                .or_else(|| object.get("enchant"))
                .or_else(|| object.get("id"))
                .and_then(value_text)
                .and_then(|value| parse_enchant_spec(&value))?;
            let level = object
                .get("level")
                .and_then(value_text)
                .and_then(|value| parse_enchant_level(&value))
                .or(enchantment.level);
            Some(ItemEnchantRule {
                tag: object
                    .get("tag")
                    .or_else(|| object.get("itemTag"))
                    .or_else(|| object.get("itemId"))
                    .and_then(value_text)
                    .map(|value| normalize_tag(&value))
                    .filter(|value| !value.is_empty()),
                name: object
                    .get("name")
                    .or_else(|| object.get("itemName"))
                    .or_else(|| object.get("displayName"))
                    .or_else(|| object.get("item"))
                    .and_then(value_text)
                    .filter(|value| !value.trim().is_empty()),
                enchantment: Enchantment {
                    id: enchantment.id,
                    level,
                },
            })
        }
        _ => {
            let text = value_text(value)?;
            let mut parts = text.split_whitespace().collect::<Vec<_>>();
            if parts.len() < 2 {
                return None;
            }
            let item = parts.remove(0);
            let enchantment = parse_enchant_spec(&parts.join(" "))?;
            Some(ItemEnchantRule {
                tag: Some(normalize_tag(item)),
                name: None,
                enchantment,
            })
        }
    }
}

fn parse_enchant_spec(value: &str) -> Option<Enchantment> {
    let value = strip_color_codes(value).trim().to_string();
    if value.is_empty() {
        return None;
    }
    let (name, level) = value
        .rsplit_once([':', '=', ' '])
        .and_then(|(name, level)| parse_enchant_level(level).map(|level| (name, Some(level))))
        .unwrap_or((value.as_str(), None));
    let id = normalize_enchant_id(name);
    (!id.is_empty()).then_some(Enchantment { id, level })
}

fn parse_enchant_level(value: &str) -> Option<u32> {
    value
        .trim()
        .parse::<u32>()
        .ok()
        .or_else(|| roman_to_number(value))
}

fn roman_to_number(value: &str) -> Option<u32> {
    let roman = value.trim().to_ascii_uppercase();
    if roman.is_empty()
        || !roman
            .chars()
            .all(|ch| matches!(ch, 'I' | 'V' | 'X' | 'L' | 'C' | 'D' | 'M'))
    {
        return None;
    }
    let mut total = 0;
    let mut previous = 0;
    for ch in roman.chars().rev() {
        let current = match ch {
            'I' => 1,
            'V' => 5,
            'X' => 10,
            'L' => 50,
            'C' => 100,
            'D' => 500,
            'M' => 1_000,
            _ => return None,
        };
        if current < previous {
            total -= current;
        } else {
            total += current;
            previous = current;
        }
    }
    Some(total)
}

fn enchant_key(enchantment: &Enchantment) -> String {
    enchantment.level.map_or_else(
        || enchantment.id.clone(),
        |level| format!("{}:{level}", enchantment.id),
    )
}

fn normalize_tag(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

fn normalize_name(value: &str) -> String {
    strip_color_codes(value)
        .replace("-us", "")
        .replace(['!', '.'], "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn normalize_enchant_id(value: &str) -> String {
    strip_color_codes(value)
        .trim()
        .trim_start_matches("ultimate ")
        .trim_start_matches("Ultimate ")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string()
}

fn strip_color_codes(value: &str) -> String {
    let mut output = String::new();
    let mut skip_next = false;
    for ch in value.chars() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if ch == '§' {
            skip_next = true;
            continue;
        }
        output.push(ch);
    }
    output
}
