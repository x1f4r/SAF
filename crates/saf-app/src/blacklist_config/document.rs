use super::snapshot::snapshot_item_enchantment;
use saf_core::SafConfig;
use saf_core::config::ItemEnchantmentConfig;
use saf_core::ports::PortError;
use serde_json::{Map, Value, json};

pub(super) fn parse_config_value(raw: &str) -> Result<Value, PortError> {
    if raw.trim().is_empty() {
        return Ok(json!({}));
    }
    json5::from_str(raw).map_err(|error| PortError::Failed(format!("parse config.json5: {error}")))
}

pub(super) fn config_array_or_effective(
    root: &Value,
    config: &SafConfig,
    section: &str,
    field: &str,
) -> Vec<Value> {
    if config_field_present(root, section, field) {
        return config_array(root, section, field);
    }
    effective_config_array(config, section, field)
}

pub(super) fn set_config_array(
    root: &mut Value,
    section: &str,
    field: &str,
    entries: Vec<Value>,
) -> Result<(), PortError> {
    let Some(object) = root.as_object_mut() else {
        return Err(PortError::Failed(
            "config root is not an object".to_string(),
        ));
    };
    let section_value = object
        .entry(section.to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    let Some(section_object) = section_value.as_object_mut() else {
        return Err(PortError::Failed(format!("{section} is not an object")));
    };
    section_object.insert(field.to_string(), Value::Array(entries));
    Ok(())
}

fn config_array(root: &Value, section: &str, field: &str) -> Vec<Value> {
    root.get(section)
        .and_then(Value::as_object)
        .and_then(|section| section.get(field))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn config_field_present(root: &Value, section: &str, field: &str) -> bool {
    root.get(section)
        .and_then(Value::as_object)
        .is_some_and(|section| section.contains_key(field))
}

fn effective_config_array(config: &SafConfig, section: &str, field: &str) -> Vec<Value> {
    match (section, field) {
        ("doNotBuy", "tags") => config.do_not_buy.tags.clone(),
        ("doNotBuy", "names") => config.do_not_buy.names.clone(),
        ("doNotBuy", "enchantments") => config.do_not_buy.enchantments.clone(),
        ("doNotBuy", "itemEnchantments") => {
            item_enchantment_entries(&config.do_not_buy.item_enchantments)
        }
        ("doNotRelist", "tags") => config.do_not_relist.tags.clone(),
        ("doNotRelist", "names") => config.do_not_relist.names.clone(),
        ("doNotRelist", "enchantments") => config.do_not_relist.enchantments.clone(),
        ("doNotRelist", "itemEnchantments") => {
            item_enchantment_entries(&config.do_not_relist.item_enchantments)
        }
        _ => Vec::new(),
    }
}

fn item_enchantment_entries(values: &[ItemEnchantmentConfig]) -> Vec<Value> {
    values.iter().map(snapshot_item_enchantment).collect()
}
