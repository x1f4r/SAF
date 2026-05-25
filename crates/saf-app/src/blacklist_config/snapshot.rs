use super::rules::{RuleValue, entry_expired, entry_expires_at, unwrap_entry};
use super::{field_name, section_name};
use saf_core::config::{BlockListConfig, ItemEnchantmentConfig, RelistBlockConfig};
use saf_core::time::{format_timestamp_millis, unix_millis};
use saf_core::{BlacklistAction, BlacklistUpdate, SafConfig};
use serde_json::{Map, Value, json};

pub(super) fn blacklist_snapshot(config: &SafConfig) -> Value {
    json!({
        "doNotBuy": snapshot_buy_section(&config.do_not_buy),
        "doNotRelist": snapshot_relist_section(&config.do_not_relist),
    })
}

pub(super) fn snapshot_item_enchantment(entry: &ItemEnchantmentConfig) -> Value {
    let mut object = Map::new();
    if let Some(tag) = &entry.tag {
        object.insert("tag".to_string(), Value::String(tag.clone()));
    }
    if let Some(name) = &entry.name {
        object.insert("name".to_string(), Value::String(name.clone()));
    }
    object.insert(
        "enchantment".to_string(),
        Value::String(entry.enchantment.clone()),
    );
    if let Some(level) = entry.level {
        object.insert("level".to_string(), Value::from(level));
    }
    if let Some(expires_at) = entry.expires_at {
        object.insert(
            "expiresAt".to_string(),
            Value::String(format_timestamp_millis(expires_at)),
        );
    }
    Value::Object(object)
}

pub(super) fn update_summary(
    update: &BlacklistUpdate,
    value: &RuleValue,
    expires_at: Option<u64>,
    changed: bool,
) -> String {
    let status = if changed {
        match update.action {
            BlacklistAction::Add => "Added",
            BlacklistAction::Remove => "Removed",
            BlacklistAction::List => "Listed",
        }
    } else {
        "Already up to date"
    };
    let expiry = expires_at
        .map(|expires_at| format!(" until {}", format_timestamp_millis(expires_at)))
        .unwrap_or_default();
    format!(
        "{status}: {}.{} {}{expiry}",
        section_name(&update.scope),
        field_name(&update.field),
        value.display()
    )
}

fn snapshot_buy_section(section: &BlockListConfig) -> Value {
    let now = unix_millis();
    json!({
        "tags": snapshot_config_values(&section.tags, now),
        "names": snapshot_config_values(&section.names, now),
        "enchantments": snapshot_config_values(&section.enchantments, now),
        "itemEnchantments": snapshot_item_enchantments(&section.item_enchantments, now),
    })
}

fn snapshot_relist_section(section: &RelistBlockConfig) -> Value {
    let now = unix_millis();
    json!({
        "tags": snapshot_config_values(&section.tags, now),
        "names": snapshot_config_values(&section.names, now),
        "enchantments": snapshot_config_values(&section.enchantments, now),
        "itemEnchantments": snapshot_item_enchantments(&section.item_enchantments, now),
    })
}

fn snapshot_config_values(values: &[Value], now: u64) -> Value {
    Value::Array(
        values
            .iter()
            .filter(|&entry| !entry_expired(entry, now))
            .cloned()
            .map(snapshot_entry)
            .collect(),
    )
}

fn snapshot_item_enchantments(values: &[ItemEnchantmentConfig], now: u64) -> Value {
    Value::Array(
        values
            .iter()
            .filter(|entry| entry.expires_at.is_none_or(|expires_at| expires_at > now))
            .map(snapshot_item_enchantment)
            .collect(),
    )
}

fn snapshot_entry(entry: Value) -> Value {
    let expires_at = entry_expires_at(&entry);
    let value = unwrap_entry(&entry);
    match expires_at {
        Some(expires_at) => match value {
            Value::Object(mut object) => {
                object.insert(
                    "expiresAt".to_string(),
                    Value::String(format_timestamp_millis(expires_at)),
                );
                Value::Object(object)
            }
            value => json!({
                "value": value,
                "expiresAt": format_timestamp_millis(expires_at),
            }),
        },
        None => value,
    }
}
