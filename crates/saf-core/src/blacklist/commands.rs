use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlacklistAction {
    List,
    Add,
    Remove,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlacklistScope {
    Buy,
    Relist,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BlacklistField {
    Tag,
    Name,
    Enchant,
    ItemEnchant,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum BlacklistRequest {
    List,
    Update(BlacklistUpdate),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlacklistUpdate {
    pub action: BlacklistAction,
    pub scope: BlacklistScope,
    pub field: BlacklistField,
    pub value: String,
    pub duration: Option<String>,
    pub until: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlacklistApplyResult {
    pub request: BlacklistRequest,
    pub changed: bool,
    pub summary: String,
}

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum BlacklistCommandError {
    #[error("unknown blacklist action {0}")]
    UnknownAction(String),
    #[error("unknown blacklist scope {0}")]
    UnknownScope(String),
    #[error("unknown blacklist field {0}")]
    UnknownField(String),
    #[error("missing blacklist {0}")]
    Missing(&'static str),
}

pub fn parse_blacklist_request(message: &str) -> Result<BlacklistRequest, BlacklistCommandError> {
    let args = message.split_whitespace().collect::<Vec<_>>();
    if args.is_empty()
        || args
            .first()
            .is_some_and(|value| value.eq_ignore_ascii_case("list"))
    {
        return Ok(BlacklistRequest::List);
    }

    let action = parse_blacklist_action(args.first().copied())?;
    let scope = parse_blacklist_scope(args.get(1).copied())?;
    let field = parse_blacklist_field(args.get(2).copied())?;
    let mut value = Vec::new();
    let mut duration = None;
    let mut until = None;
    let mut index = 3;
    while index < args.len() {
        match args[index] {
            "--for" | "--duration" => {
                index += 1;
                duration = Some(
                    args.get(index)
                        .copied()
                        .ok_or(BlacklistCommandError::Missing("duration"))?
                        .to_string(),
                );
            }
            "--until" | "--expires-at" => {
                index += 1;
                until = Some(
                    args.get(index)
                        .copied()
                        .ok_or(BlacklistCommandError::Missing("expiry"))?
                        .to_string(),
                );
            }
            other => value.push(other),
        }
        index += 1;
    }

    let value = value.join(" ");
    if value.trim().is_empty() {
        return Err(BlacklistCommandError::Missing("value"));
    }

    Ok(BlacklistRequest::Update(BlacklistUpdate {
        action,
        scope,
        field,
        value,
        duration,
        until,
    }))
}

fn parse_blacklist_action(value: Option<&str>) -> Result<BlacklistAction, BlacklistCommandError> {
    match value
        .ok_or(BlacklistCommandError::Missing("action"))?
        .to_ascii_lowercase()
        .as_str()
    {
        "add" => Ok(BlacklistAction::Add),
        "remove" => Ok(BlacklistAction::Remove),
        "list" => Ok(BlacklistAction::List),
        other => Err(BlacklistCommandError::UnknownAction(other.to_string())),
    }
}

fn parse_blacklist_scope(value: Option<&str>) -> Result<BlacklistScope, BlacklistCommandError> {
    match value
        .ok_or(BlacklistCommandError::Missing("scope"))?
        .to_ascii_lowercase()
        .as_str()
    {
        "buy" => Ok(BlacklistScope::Buy),
        "relist" => Ok(BlacklistScope::Relist),
        other => Err(BlacklistCommandError::UnknownScope(other.to_string())),
    }
}

fn parse_blacklist_field(value: Option<&str>) -> Result<BlacklistField, BlacklistCommandError> {
    match value
        .ok_or(BlacklistCommandError::Missing("field"))?
        .to_ascii_lowercase()
        .as_str()
    {
        "tag" => Ok(BlacklistField::Tag),
        "name" => Ok(BlacklistField::Name),
        "enchant" | "enchantment" => Ok(BlacklistField::Enchant),
        "item-enchant" | "item_enchant" | "item-enchantment" | "item_enchantment" => {
            Ok(BlacklistField::ItemEnchant)
        }
        other => Err(BlacklistCommandError::UnknownField(other.to_string())),
    }
}
