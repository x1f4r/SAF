use saf_core::LocalCommand;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommandOptionValue {
    String(String),
    Integer(i64),
    Boolean(bool),
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandInvocation {
    pub name: String,
    #[serde(default)]
    pub options: BTreeMap<String, CommandOptionValue>,
}

impl CommandInvocation {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            options: BTreeMap::new(),
        }
    }

    pub fn with_string(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.options
            .insert(name.into(), CommandOptionValue::String(value.into()));
        self
    }

    pub fn with_integer(mut self, name: impl Into<String>, value: i64) -> Self {
        self.options
            .insert(name.into(), CommandOptionValue::Integer(value));
        self
    }

    pub fn with_boolean(mut self, name: impl Into<String>, value: bool) -> Self {
        self.options
            .insert(name.into(), CommandOptionValue::Boolean(value));
        self
    }

    pub(in crate::planning) fn string(&self, name: &str) -> Option<&str> {
        match self.options.get(name) {
            Some(CommandOptionValue::String(value)) => {
                Some(value.trim()).filter(|value| !value.is_empty())
            }
            _ => None,
        }
    }

    pub(in crate::planning) fn integer(&self, name: &str) -> Option<i64> {
        match self.options.get(name) {
            Some(CommandOptionValue::Integer(value)) => Some(*value),
            _ => None,
        }
    }

    pub(in crate::planning) fn boolean(&self, name: &str) -> bool {
        matches!(
            self.options.get(name),
            Some(CommandOptionValue::Boolean(true))
        )
    }

    pub(in crate::planning) fn required_string(
        &self,
        name: &'static str,
    ) -> Result<String, DiscordCommandPlanError> {
        self.string(name)
            .map(ToString::to_string)
            .ok_or(DiscordCommandPlanError::MissingOption(name))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DiscordCommandPlan {
    LocalCommand { command: LocalCommand },
    ControllerAction { action: DiscordControllerAction },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DiscordControllerAction {
    Dashboard,
    Status,
    Help,
    AccountPanel {
        username: String,
    },
    StatusForConfirmation {
        username: String,
        action: String,
    },
    DiscordId,
    RefreshHeads {
        username: Option<String>,
    },
    SellInventory {
        username: Option<String>,
        include_hotbar: bool,
    },
    DelistEverything {
        username: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum DiscordCommandPlanError {
    #[error("missing Discord command option {0}")]
    MissingOption(&'static str),
    #[error("unknown Discord command {0}")]
    UnknownCommand(String),
}
