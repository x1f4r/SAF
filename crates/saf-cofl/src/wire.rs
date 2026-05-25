use crate::errors::CoflCommandError;
use saf_core::ports::{InventoryItem, InventorySnapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CoflCommandMessage {
    #[serde(rename = "type")]
    pub kind: String,
    pub data: String,
}

impl CoflCommandMessage {
    pub fn from_terminal_command(command: &str) -> Result<Self, CoflCommandError> {
        let mut parts = command.split_whitespace().collect::<Vec<_>>();
        if parts
            .first()
            .is_some_and(|part| is_cofl_command_prefix(part))
        {
            parts.remove(0);
        }

        let kind = parts
            .first()
            .copied()
            .ok_or(CoflCommandError::MissingCommand)?;
        let data = parts.iter().skip(1).copied().collect::<Vec<_>>().join(" ");

        Ok(Self {
            kind: kind.trim_start_matches('/').to_string(),
            data: serde_json::to_string(&data)?,
        })
    }

    pub fn to_wire_json(&self) -> Result<String, CoflCommandError> {
        Ok(serde_json::to_string(self)?)
    }
}

fn is_cofl_command_prefix(part: &str) -> bool {
    matches!(
        part.trim_start_matches('/').to_ascii_lowercase().as_str(),
        "cofl" | "saf" | "icymacro"
    )
}

pub fn encode_cofl_command(command: &str) -> Result<String, CoflCommandError> {
    CoflCommandMessage::from_terminal_command(command)?.to_wire_json()
}

pub fn encode_scoreboard_upload(lines: &[String]) -> Result<String, CoflCommandError> {
    Ok(serde_json::to_string(&json!({
        "type": "uploadScoreboard",
        "data": serde_json::to_string(lines)?
    }))?)
}

pub fn encode_inventory_upload<T: Serialize>(inventory: &T) -> Result<String, CoflCommandError> {
    Ok(serde_json::to_string(&json!({
        "type": "uploadInventory",
        "data": serde_json::to_string(inventory)?
    }))?)
}

pub fn encode_inventory_snapshot_upload(
    snapshot: &InventorySnapshot,
) -> Result<String, CoflCommandError> {
    let slot_count = snapshot
        .items
        .iter()
        .filter_map(|item| item.slot.map(usize::from))
        .max()
        .map(|slot| slot + 1)
        .unwrap_or(46)
        .max(46);
    let mut slots = vec![Value::Null; slot_count];
    for item in &snapshot.items {
        let Some(slot) = item.slot.map(usize::from) else {
            continue;
        };
        if slot >= slots.len() {
            continue;
        }
        slots[slot] = inventory_item_slot(item);
    }

    encode_inventory_upload(&json!({
        "_events": {},
        "_eventsCount": 0,
        "id": 0,
        "type": "minecraft:inventory",
        "title": "Inventory",
        "slots": slots
    }))
}

pub fn encode_chat_batch(messages: &[String]) -> Result<String, CoflCommandError> {
    Ok(serde_json::to_string(&json!({
        "type": "chatBatch",
        "data": serde_json::to_string(messages)?
    }))?)
}

fn inventory_item_slot(item: &InventoryItem) -> Value {
    let slot = item.slot.unwrap_or_default();
    let skyblock_id = item
        .tag
        .as_deref()
        .filter(|tag| !tag.trim().is_empty())
        .unwrap_or(item.item_name.as_str());
    let mut extra_attributes = json!({
        "id": {
            "type": "string",
            "value": skyblock_id
        }
    });
    if let Some(uuid) = item.uuid.as_deref().filter(|uuid| !uuid.trim().is_empty()) {
        extra_attributes["uuid"] = json!({
            "type": "string",
            "value": uuid
        });
    }

    json!({
        "slot": slot,
        "name": item_name_to_minecraft_name(&item.item_name),
        "displayName": item.item_name,
        "count": 1,
        "lore": item.lore,
        "nbt": {
            "type": "compound",
            "value": {
                "display": {
                    "type": "compound",
                    "value": {
                        "Name": {
                            "type": "string",
                            "value": serde_json::to_string(&json!({ "text": item.item_name })).unwrap_or_else(|_| item.item_name.clone())
                        },
                        "Lore": {
                            "type": "list",
                            "value": {
                                "type": "string",
                                "value": item.lore.iter()
                                    .map(|line| serde_json::to_string(&json!({ "text": line })).unwrap_or_else(|_| line.clone()))
                                    .collect::<Vec<_>>()
                            }
                        }
                    }
                },
                "ExtraAttributes": {
                    "type": "compound",
                    "value": extra_attributes
                }
            }
        }
    })
}

fn item_name_to_minecraft_name(item_name: &str) -> String {
    let normalized = item_name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    normalized
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}
