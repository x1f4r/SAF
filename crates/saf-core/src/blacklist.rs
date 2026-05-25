use crate::time::parse_timestamp_millis;
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod commands;
mod policy;

pub use commands::{
    BlacklistAction, BlacklistApplyResult, BlacklistCommandError, BlacklistField, BlacklistRequest,
    BlacklistScope, BlacklistUpdate, parse_blacklist_request,
};
pub use policy::{
    BlacklistPolicy, BlacklistPolicyHandle, BlockSection, ItemEnchantmentRule, RelistSection,
    TimedValue,
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Enchantment {
    pub id: String,
    pub level: Option<u32>,
}

impl Enchantment {
    pub fn new(id: impl AsRef<str>, level: Option<u32>) -> Self {
        Self {
            id: normalize_enchant_id(id.as_ref()),
            level,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ItemContext {
    pub tag: Option<String>,
    pub item_name: Option<String>,
    pub weird_item_name: Option<String>,
    pub enchantments: Vec<Enchantment>,
    pub finder: Option<String>,
    pub profit: Option<f64>,
    pub skinned: bool,
    pub is_purchased_inventory_relist: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BlockReason {
    Tag(String),
    Name(String),
    Enchantment(String),
    ItemEnchantment(String),
    Finder(String),
    Profit(String),
    Skin,
}

impl std::fmt::Display for BlockReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Tag(value) => write!(f, "{value} is a blocked tag"),
            Self::Name(value) => write!(f, "{value} is a blocked name"),
            Self::Enchantment(value) => write!(f, "{value} is a blocked enchantment"),
            Self::ItemEnchantment(value) => write!(f, "{value} is a blocked item/enchantment"),
            Self::Finder(value) => write!(f, "{value} is a blocked finder"),
            Self::Profit(value) => write!(f, "profit exceeds {value}"),
            Self::Skin => f.write_str("cosmetic items are blocked"),
        }
    }
}

fn enchant_matches(actual: &Enchantment, rule: &Enchantment) -> bool {
    actual.id == rule.id && rule.level.is_none_or(|level| actual.level == Some(level))
}

fn parse_enchant_spec(value: &str) -> Option<Enchantment> {
    let normalized = value.trim();
    if normalized.is_empty() {
        return None;
    }

    let mut parts = normalized.split(':');
    let id = normalize_enchant_id(parts.next()?);
    let level = parts.next().and_then(parse_enchant_level);
    Some(Enchantment { id, level })
}

fn parse_enchant_level(value: &str) -> Option<u32> {
    value
        .trim()
        .parse::<u32>()
        .ok()
        .or_else(|| roman_to_number(value))
}

fn roman_to_number(value: &str) -> Option<u32> {
    match value.trim().to_ascii_uppercase().as_str() {
        "I" => Some(1),
        "II" => Some(2),
        "III" => Some(3),
        "IV" => Some(4),
        "V" => Some(5),
        "VI" => Some(6),
        "VII" => Some(7),
        "VIII" => Some(8),
        "IX" => Some(9),
        "X" => Some(10),
        _ => None,
    }
}

impl Enchantment {
    fn key(&self) -> String {
        match self.level {
            Some(level) => format!("{}:{level}", self.id),
            None => self.id.clone(),
        }
    }
}

pub fn parse_lore_enchantments(lines: &[String]) -> Vec<Enchantment> {
    let mut parsed = Vec::new();
    for line in lines {
        let clean = strip_minecraft_formatting(line);
        for segment in clean.split(',') {
            let segment = segment
                .rsplit_once(':')
                .map(|(_, value)| value)
                .unwrap_or(segment)
                .trim();
            let words = segment.split_whitespace().collect::<Vec<_>>();
            if words.len() < 2 {
                continue;
            }
            for level_index in (1..words.len()).rev() {
                let raw_level = words[level_index]
                    .trim_matches(|value: char| !(value.is_ascii_alphanumeric() || value == '_'));
                let Some(level) = parse_enchant_level(raw_level) else {
                    continue;
                };
                let name = words[..level_index].join(" ");
                if name.chars().any(char::is_alphabetic) {
                    let enchantment = Enchantment::new(name, Some(level));
                    if !parsed.iter().any(|existing: &Enchantment| {
                        existing.id == enchantment.id && existing.level == enchantment.level
                    }) {
                        parsed.push(enchantment);
                    }
                }
                break;
            }
        }
    }
    parsed
}

fn strip_minecraft_formatting(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut skip_next = false;
    for character in value.chars() {
        if skip_next {
            skip_next = false;
            continue;
        }
        if character == '§' {
            skip_next = true;
            continue;
        }
        output.push(character);
    }
    output
}

fn item_tags(item: &ItemContext) -> Vec<String> {
    item.tag.as_deref().map(normalize_tag).into_iter().collect()
}

fn item_names(item: &ItemContext) -> Vec<String> {
    item.item_name
        .as_deref()
        .into_iter()
        .chain(item.weird_item_name.as_deref())
        .map(normalize_name)
        .collect()
}

fn normalize_tag(value: &str) -> String {
    value.trim().to_ascii_uppercase()
}

fn normalize_name(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}

fn normalize_enchant_id(value: &str) -> String {
    value.trim().replace([' ', '-'], "_").to_ascii_uppercase()
}

fn entry_expires_at(value: &Value) -> Option<u64> {
    let object = value.as_object()?;
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

#[cfg(test)]
mod tests;
