use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandDefinition {
    pub name: &'static str,
    pub description: &'static str,
    pub options: Vec<CommandOption>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandOption {
    pub name: &'static str,
    pub description: &'static str,
    pub kind: CommandOptionKind,
    pub required: bool,
    pub choices: Vec<CommandOptionChoice>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandOptionChoice {
    pub name: &'static str,
    pub value: &'static str,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOptionKind {
    String,
    Integer,
    Boolean,
}

impl CommandOption {
    pub fn string(name: &'static str, required: bool) -> Self {
        Self {
            name,
            description: name,
            kind: CommandOptionKind::String,
            required,
            choices: Vec::new(),
        }
    }

    pub fn integer(name: &'static str, required: bool) -> Self {
        Self {
            name,
            description: name,
            kind: CommandOptionKind::Integer,
            required,
            choices: Vec::new(),
        }
    }

    pub fn boolean(name: &'static str, required: bool) -> Self {
        Self {
            name,
            description: name,
            kind: CommandOptionKind::Boolean,
            required,
            choices: Vec::new(),
        }
    }

    pub fn described(mut self, description: &'static str) -> Self {
        self.description = description;
        self
    }

    pub fn with_string_choices(
        mut self,
        choices: impl IntoIterator<Item = (&'static str, &'static str)>,
    ) -> Self {
        self.choices = choices
            .into_iter()
            .map(|(name, value)| CommandOptionChoice { name, value })
            .collect();
        self
    }
}

pub fn command_definitions() -> Vec<CommandDefinition> {
    let target = CommandOption::string("username", false)
        .described("Minecraft IGN. Blank uses the active/default account.");
    vec![
        cmd(
            "dashboard",
            "Open the interactive SAF control panel.",
            vec![],
        ),
        cmd(
            "start_bot",
            "Start the default account or a specific account.",
            vec![target.clone()],
        ),
        cmd(
            "stop_bot",
            "Stop all running accounts, or one specific account.",
            vec![target.clone()],
        ),
        cmd("status", "Open the status dashboard.", vec![]),
        cmd(
            "send_command",
            "Send any terminal-style command.",
            vec![CommandOption::string("command", true), target.clone()],
        ),
        cmd(
            "send_terminal",
            "Alias for /send_command.",
            vec![CommandOption::string("command", true), target.clone()],
        ),
        cmd(
            "cofl",
            "Send a SkyCofl command.",
            vec![CommandOption::string("command", true), target.clone()],
        ),
        cmd(
            "blacklist",
            "List or update live blacklist rules.",
            vec![
                CommandOption::string("action", true)
                    .described("Choose list, add, or remove.")
                    .with_string_choices([("list", "list"), ("add", "add"), ("remove", "remove")]),
                CommandOption::string("scope", false)
                    .described("Which workflow the rule applies to.")
                    .with_string_choices([("buy", "buy"), ("relist", "relist")]),
                CommandOption::string("field", false)
                    .described("Rule type: item tag, display name, enchant, or item+enchant.")
                    .with_string_choices([
                        ("tag", "tag"),
                        ("name", "name"),
                        ("enchant", "enchant"),
                        ("item-enchant", "item-enchant"),
                    ]),
                CommandOption::string("value", false)
                    .described("Rule value, for example LAVA_SHELL_NECKLACE THE_ONE:5."),
                CommandOption::string("duration", false)
                    .described("Optional expiry for add, for example 7d, 12h, or an ISO date."),
            ],
        ),
        cmd(
            "queue",
            "Show the pending action queue.",
            vec![target.clone()],
        ),
        cmd("clear_queue", "Clear queued actions.", vec![target.clone()]),
        cmd(
            "cancel_queue",
            "Cancel a single queued action by its index.",
            vec![target.clone(), CommandOption::integer("index", true)],
        ),
        cmd(
            "clear_data",
            "Clear saved queue/bid data.",
            vec![target.clone()],
        ),
        cmd("logs", "Attach the latest SAF log.", vec![]),
        cmd(
            "messages",
            "Show recent SAF log messages.",
            vec![CommandOption::integer("lines", false)],
        ),
        cmd(
            "stats",
            "Ask SAF to send normal stats.",
            vec![target.clone()],
        ),
        cmd("ping", "Ask SAF to send ping stats.", vec![target.clone()]),
        cmd("profit", "Show local profit.", vec![target.clone()]),
        cmd("global_stats", "Show aggregate local stats.", vec![]),
        cmd("users", "Show configured and running accounts.", vec![]),
        cmd("connections", "Show SkyCofl connection IDs.", vec![]),
        cmd("discord_id", "Show your Discord user ID.", vec![]),
        cmd("help", "Show registered SAF commands.", vec![]),
        cmd(
            "refresh_heads",
            "Refresh cached Discord player-head images.",
            vec![target.clone()],
        ),
        cmd(
            "buy_flip",
            "Queue an external auction purchase.",
            vec![CommandOption::string("auction_id", true), target.clone()],
        ),
        cmd(
            "list_flip",
            "Queue listing for a tracked bought flip.",
            vec![
                CommandOption::string("auction_id", true),
                CommandOption::string("price", false),
                target.clone(),
                CommandOption::string("time", false),
            ],
        ),
        cmd(
            "list_item",
            "Queue a manual item listing.",
            vec![
                CommandOption::string("item_uuid", true),
                CommandOption::string("price", true),
                target.clone(),
                CommandOption::string("time", false),
            ],
        ),
        cmd(
            "delist",
            "Queue a delist action.",
            vec![
                CommandOption::string("auction_id", true),
                CommandOption::string("item_uuid", true),
                target.clone(),
            ],
        ),
        cmd(
            "delist_everything",
            "Scan active auctions before delisting.",
            vec![target.clone()],
        ),
        cmd(
            "delist_all",
            "Alias for /delist_everything.",
            vec![target.clone()],
        ),
        cmd(
            "reconcile",
            "Queue auction reconciliation.",
            vec![target.clone()],
        ),
        cmd(
            "bids",
            "Queue bid-section collection.",
            vec![target.clone()],
        ),
        cmd(
            "claim_sold",
            "Queue sold-auction collection.",
            vec![target.clone()],
        ),
        cmd(
            "cookie",
            "Force-buy and refresh the booster cookie now.",
            vec![target.clone()],
        ),
        cmd(
            "bank",
            "Withdraw or deposit coins.",
            vec![
                CommandOption::string("amount", true),
                target.clone(),
                CommandOption::boolean("withdraw", false),
                CommandOption::boolean("personal", false),
            ],
        ),
        cmd(
            "coins",
            "Queue a bank coin action.",
            vec![
                CommandOption::string("amount", true),
                target.clone(),
                CommandOption::boolean("withdraw", false),
                CommandOption::boolean("personal", false),
            ],
        ),
        cmd(
            "transfer_coins",
            "Move coins between co-op accounts.",
            vec![
                CommandOption::string("from", true),
                CommandOption::string("to", true),
                CommandOption::string("amount", true),
                CommandOption::boolean("stop_source", false),
            ],
        ),
        cmd(
            "timeout",
            "Stop an account after a delay.",
            vec![CommandOption::string("duration", true), target.clone()],
        ),
        cmd(
            "start_in",
            "Start an account after a delay.",
            vec![CommandOption::string("duration", true), target.clone()],
        ),
        cmd(
            "inventory",
            "Show a priced inventory overview.",
            vec![target.clone()],
        ),
        cmd(
            "sell_inventory",
            "Preview inventory items before listing.",
            vec![
                target.clone(),
                CommandOption::boolean("include_hotbar", false),
            ],
        ),
        cmd(
            "test_webhook",
            "Send a SAF test notification.",
            vec![target.clone()],
        ),
        cmd(
            "diagslots",
            "Return cached GUI slot diagnostics.",
            vec![
                CommandOption::string("target", false)
                    .described("Optional GUI target such as current, ah, bank, or profiles."),
                target.clone(),
            ],
        ),
        cmd("get_log", "Alias for /logs.", vec![]),
        cmd(
            "get_messages",
            "Alias for /messages.",
            vec![CommandOption::integer("lines", false)],
        ),
        cmd("get_ping", "Alias for /ping.", vec![target.clone()]),
        cmd("get_profit", "Alias for /profit.", vec![target.clone()]),
        cmd("get_queue", "Alias for /queue.", vec![target.clone()]),
        cmd("get_stats", "Alias for /stats.", vec![target.clone()]),
        cmd("get_users", "Alias for /users.", vec![]),
        cmd("get_connections", "Alias for /connections.", vec![]),
        cmd("get_discord_id", "Show your Discord user ID.", vec![]),
        cmd("get_global_stats", "Alias for /global_stats.", vec![]),
        cmd(
            "delist_item",
            "Alias for /delist.",
            vec![
                CommandOption::string("auction_id", true),
                CommandOption::string("item_uuid", true),
                target.clone(),
            ],
        ),
        cmd(
            "set_time_in",
            "Alias for /start_in.",
            vec![CommandOption::string("duration", true), target.clone()],
        ),
        cmd(
            "set_timeout",
            "Alias for /timeout.",
            vec![CommandOption::string("duration", true), target],
        ),
    ]
}

pub fn validate_command_definitions(commands: &[CommandDefinition]) -> Result<(), String> {
    if commands.len() > 100 {
        return Err("Discord application command limit exceeded".to_string());
    }
    let mut seen = BTreeSet::new();
    for command in commands {
        if !seen.insert(command.name) {
            return Err(format!("duplicate command {}", command.name));
        }
        for option in &command.options {
            if option.choices.len() > 25 {
                return Err(format!(
                    "too many choices for {}.{}",
                    command.name, option.name
                ));
            }
            let mut choices = BTreeSet::new();
            for choice in &option.choices {
                if !choices.insert(choice.name) {
                    return Err(format!(
                        "duplicate choice {} for {}.{}",
                        choice.name, command.name, option.name
                    ));
                }
            }
        }
    }
    Ok(())
}

fn cmd(
    name: &'static str,
    description: &'static str,
    options: Vec<CommandOption>,
) -> CommandDefinition {
    CommandDefinition {
        name,
        description,
        options,
    }
}
