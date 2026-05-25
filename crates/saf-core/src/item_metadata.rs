use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemComponent {
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub data: Option<Value>,
    #[serde(default)]
    pub value: Option<Value>,
}

impl ItemComponent {
    pub fn payload(&self) -> Option<&Value> {
        self.data.as_ref().or(self.value.as_ref())
    }

    fn component_type(&self) -> Option<&str> {
        self.r#type
            .as_deref()
            .or(self.name.as_deref())
            .or(self.id.as_deref())
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemStack {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub custom_name: Option<Value>,
    #[serde(default)]
    pub lore: Option<Value>,
    #[serde(default)]
    pub nbt: Option<Value>,
    #[serde(default)]
    pub component_map: BTreeMap<String, ItemComponent>,
    #[serde(default)]
    pub components: Vec<ItemComponent>,
}

impl ItemStack {
    pub fn component_data(&self, component_type: &str) -> Option<Value> {
        for key in component_keys(component_type) {
            if let Some(component) = self.component_map.get(&key) {
                return component.payload().cloned();
            }
        }

        for (key, component) in &self.component_map {
            if normalize_component_type(key) == component_type {
                return component.payload().cloned();
            }
        }

        self.components
            .iter()
            .find(|component| {
                component
                    .component_type()
                    .is_some_and(|kind| normalize_component_type(kind) == component_type)
            })
            .and_then(ItemComponent::payload)
            .cloned()
    }

    pub fn display_name(&self) -> Option<String> {
        let component_name = self
            .component_data("custom_name")
            .or_else(|| self.component_data("item_name"))
            .or_else(|| self.custom_name.clone());
        if let Some(text) = component_name.as_ref().and_then(text_to_plain) {
            return Some(text);
        }

        let legacy_name = self
            .nbt
            .as_ref()
            .and_then(|nbt| nbt.pointer("/value/display/value/Name"))
            .cloned();
        if let Some(text) = legacy_name.as_ref().and_then(text_to_plain) {
            return Some(text);
        }

        self.display_name.clone().or_else(|| self.name.clone())
    }

    pub fn lore(&self) -> Vec<String> {
        let component_lore = self.component_data("lore").or_else(|| self.lore.clone());
        if let Some(Value::Array(lines)) = component_lore {
            return lines.iter().filter_map(text_to_plain).collect();
        }

        let legacy_lore = self
            .nbt
            .as_ref()
            .and_then(|nbt| nbt.pointer("/value/display/value/Lore"))
            .map(simplify_nbt);
        if let Some(Value::Array(lines)) = legacy_lore {
            return lines.iter().filter_map(text_to_plain).collect();
        }

        Vec::new()
    }

    pub fn custom_data(&self) -> Value {
        let legacy = self.nbt.as_ref().map(simplify_nbt).unwrap_or(Value::Null);
        let component = self
            .component_data("custom_data")
            .map(|value| simplify_nbt(&value))
            .unwrap_or(Value::Null);

        let mut merged = object_or_empty(legacy);
        let component_map = object_or_empty(component);
        for (key, value) in component_map {
            merged.insert(key, value);
        }
        if !merged.contains_key("ExtraAttributes") {
            if let Some(extra) = merged.get("ExtraAttributes").cloned() {
                merged.insert("ExtraAttributes".to_string(), extra);
            }
        }
        Value::Object(merged)
    }

    pub fn extra_attributes(&self) -> Option<Value> {
        if looks_like_extra_attributes(&self.custom_data()) {
            return Some(self.custom_data());
        }

        let data = self.custom_data();
        if let Some(extra) = data.get("ExtraAttributes") {
            return Some(extra.clone());
        }

        let mut direct = object_or_empty(data);
        direct.remove("ExtraAttributes");
        if direct.is_empty() {
            None
        } else {
            Some(Value::Object(direct))
        }
    }

    pub fn skyblock_item_id(&self) -> Option<String> {
        let extra = self.extra_attributes()?;
        skyblock_item_id_from_extra(&extra)
    }

    pub fn item_uuid(&self) -> Option<String> {
        let extra = self.extra_attributes()?;
        extra
            .get("uuid")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned)
            .or_else(|| skyblock_item_id_from_extra(&extra))
    }
}

pub fn simplify_nbt(data: &Value) -> Value {
    match data {
        Value::Array(values) => Value::Array(values.iter().map(simplify_nbt).collect()),
        Value::Object(object) => {
            if object.contains_key("type") && object.contains_key("value") {
                let kind = object
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let value = object.get("value").unwrap_or(&Value::Null);
                if kind == "compound" {
                    return simplify_nbt(value);
                }
                if kind == "list" {
                    if let Value::Array(values) = value {
                        return Value::Array(values.iter().map(simplify_nbt).collect());
                    }
                    if let Some(Value::Array(values)) = value.get("value") {
                        return Value::Array(values.iter().map(simplify_nbt).collect());
                    }
                    return Value::Array(Vec::new());
                }
                return simplify_nbt(value);
            }

            Value::Object(
                object
                    .iter()
                    .map(|(key, value)| (key.clone(), simplify_nbt(value)))
                    .collect(),
            )
        }
        value => value.clone(),
    }
}

pub fn text_to_plain(value: &Value) -> Option<String> {
    let simplified = simplify_nbt(value);
    match simplified {
        Value::String(text) => {
            let trimmed = text.trim();
            if ((trimmed.starts_with('{') && trimmed.ends_with('}'))
                || (trimmed.starts_with('[') && trimmed.ends_with(']')))
                && let Ok(parsed) = serde_json::from_str::<Value>(trimmed)
            {
                return text_to_plain(&parsed);
            }
            Some(text)
        }
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        Value::Array(values) => {
            let text = values.iter().filter_map(text_to_plain).collect::<String>();
            (!text.is_empty()).then_some(text)
        }
        Value::Object(object) => {
            let mut text = String::new();
            if let Some(value) = object.get("text").and_then(Value::as_str) {
                text.push_str(value);
            }
            if text.is_empty()
                && let Some(value) = object.get("translate").and_then(Value::as_str)
            {
                text.push_str(value);
            }
            if text.is_empty()
                && let Some(value) = object.get("fallback").and_then(Value::as_str)
            {
                text.push_str(value);
            }
            if let Some(Value::Array(values)) = object.get("with") {
                text.push_str(&values.iter().filter_map(text_to_plain).collect::<String>());
            }
            if let Some(Value::Array(values)) = object.get("extra") {
                text.push_str(&values.iter().filter_map(text_to_plain).collect::<String>());
            }
            (!text.is_empty()).then_some(text)
        }
        Value::Null => None,
    }
}

pub fn looks_like_extra_attributes(value: &Value) -> bool {
    let simplified = simplify_nbt(value);
    let Value::Object(object) = simplified else {
        return false;
    };

    object.get("id").and_then(Value::as_str).is_some()
        || [
            "uuid",
            "runes",
            "drill_part_fuel_tank",
            "drill_part_engine",
            "drill_part_upgrade_module",
        ]
        .iter()
        .any(|key| object.contains_key(*key))
}

pub fn skyblock_item_id_from_extra(extra: &Value) -> Option<String> {
    let simplified = simplify_nbt(extra);
    let object = simplified.as_object()?;
    let mut id = object.get("id").and_then(Value::as_str)?.to_string();
    let first = id.split('_').next().unwrap_or_default();
    if matches!(first, "RUNE" | "UNIQUE")
        && let Some(runes) = object.get("runes").and_then(Value::as_object)
        && let Some(rune) = runes.keys().next()
    {
        id = format!("{rune}_RUNE");
    }
    Some(id)
}

fn component_keys(component_type: &str) -> Vec<String> {
    vec![
        component_type.to_string(),
        format!("minecraft:{component_type}"),
    ]
}

fn normalize_component_type(value: &str) -> &str {
    value.strip_prefix("minecraft:").unwrap_or(value)
}

fn object_or_empty(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(object) => object,
        _ => Map::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_legacy_nbt_items() {
        let item: ItemStack = serde_json::from_value(json!({
            "name": "diamond_sword",
            "nbt": {
                "type": "compound",
                "value": {
                    "display": {
                        "type": "compound",
                        "value": {
                            "Name": { "type": "string", "value": "{\"text\":\"Aspect of the Dragons\"}" },
                            "Lore": {
                                "type": "list",
                                "value": {
                                    "type": "string",
                                    "value": ["{\"text\":\"Damage: +225\"}", "§7A classic weapon"]
                                }
                            }
                        }
                    },
                    "ExtraAttributes": {
                        "type": "compound",
                        "value": {
                            "id": { "type": "string", "value": "ASPECT_OF_THE_DRAGON" },
                            "uuid": { "type": "string", "value": "item-uuid" }
                        }
                    }
                }
            }
        }))
        .unwrap();

        assert_eq!(item.display_name().unwrap(), "Aspect of the Dragons");
        assert_eq!(item.lore(), vec!["Damage: +225", "§7A classic weapon"]);
        assert_eq!(
            item.extra_attributes().unwrap()["id"],
            "ASPECT_OF_THE_DRAGON"
        );
        assert_eq!(item.item_uuid().unwrap(), "item-uuid");
    }

    #[test]
    fn reads_component_backed_items_and_rune_ids() {
        let item: ItemStack = serde_json::from_value(json!({
            "name": "rune",
            "componentMap": {
                "custom_name": { "type": "custom_name", "data": { "text": "Couture Rune I" } },
                "lore": { "type": "lore", "data": [{ "text": "Requires level 1" }] },
                "custom_data": {
                    "type": "custom_data",
                    "data": {
                        "type": "compound",
                        "value": {
                            "ExtraAttributes": {
                                "type": "compound",
                                "value": {
                                    "id": { "type": "string", "value": "RUNE" },
                                    "runes": {
                                        "type": "compound",
                                        "value": {
                                            "COUTURE": { "type": "int", "value": 1 }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }))
        .unwrap();

        assert_eq!(item.display_name().unwrap(), "Couture Rune I");
        assert_eq!(item.lore(), vec!["Requires level 1"]);
        assert_eq!(item.item_uuid().unwrap(), "COUTURE_RUNE");
    }
}
