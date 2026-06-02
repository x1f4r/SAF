use super::helpers::{controller, local_terminal, targeted_line};
use super::{
    CommandInvocation, CommandOptionValue, DiscordCommandPlan, DiscordCommandPlanError,
    DiscordControllerAction,
};
use saf_core::LocalCommand;

pub fn plan_invocation(
    invocation: &CommandInvocation,
) -> Result<DiscordCommandPlan, DiscordCommandPlanError> {
    let username = invocation.string("username").map(ToString::to_string);
    match invocation.name.trim().to_ascii_lowercase().as_str() {
        "dashboard" => Ok(controller(DiscordControllerAction::Dashboard)),
        "status" => Ok(controller(DiscordControllerAction::Status)),
        "help" => Ok(controller(DiscordControllerAction::Help)),
        "logs" | "get_log" => Ok(local_terminal("logs".to_string())),
        "messages" | "get_messages" => {
            let lines = invocation.integer("lines").map(|lines| lines.to_string());
            Ok(local_terminal(targeted_line(
                None,
                "messages",
                lines.as_deref(),
            )))
        }
        "profit" | "get_profit" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "profit",
            std::iter::empty::<&str>(),
        ))),
        "global_stats" | "get_global_stats" => Ok(local_terminal("global_stats".to_string())),
        "users" | "get_users" => Ok(local_terminal("users".to_string())),
        "connections" | "get_connections" => Ok(local_terminal("connections".to_string())),
        "discord_id" | "get_discord_id" => Ok(controller(DiscordControllerAction::DiscordId)),
        "refresh_heads" => Ok(controller(DiscordControllerAction::RefreshHeads {
            username,
        })),
        "inventory" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "inventory",
            std::iter::empty::<&str>(),
        ))),
        "sell_inventory" => Ok(controller(DiscordControllerAction::SellInventory {
            username,
            include_hotbar: invocation.boolean("include_hotbar"),
        })),
        "delist_everything" | "delist_all" => {
            Ok(controller(DiscordControllerAction::DelistEverything {
                username,
            }))
        }
        "timeout" | "set_timeout" => Ok(local_terminal(targeted_line(
            None,
            "timeout",
            [
                Some(invocation.required_string("duration")?.as_str()),
                username.as_deref(),
            ]
            .into_iter()
            .flatten(),
        ))),
        "start_in" | "set_time_in" => Ok(local_terminal(targeted_line(
            None,
            "start_in",
            [
                Some(invocation.required_string("duration")?.as_str()),
                username.as_deref(),
            ]
            .into_iter()
            .flatten(),
        ))),
        "start_bot" => Ok(local_terminal(targeted_line(
            None,
            "start",
            [username.as_deref()].into_iter().flatten(),
        ))),
        "stop_bot" => Ok(local_terminal(targeted_line(
            None,
            "stop",
            [username.as_deref()].into_iter().flatten(),
        ))),
        "send_command" | "send_terminal" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            invocation.required_string("command")?,
            std::iter::empty::<&str>(),
        ))),
        "cofl" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "/cofl",
            [invocation.required_string("command")?.as_str()],
        ))),
        "blacklist" => {
            let mut args = [
                invocation.string("action"),
                invocation.string("scope"),
                invocation.string("field"),
                invocation.string("value"),
            ]
            .into_iter()
            .flatten()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
            if let Some(duration) = invocation.string("duration") {
                args.push("--duration".to_string());
                args.push(duration.to_string());
            }
            Ok(local_terminal(targeted_line(
                username.as_deref(),
                "blacklist",
                args.iter().map(String::as_str),
            )))
        }
        "queue" | "get_queue" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "queue",
            std::iter::empty::<&str>(),
        ))),
        "clear_queue" => Ok(local_terminal(match username.as_deref() {
            Some(username) => {
                targeted_line(Some(username), "clear_queue", std::iter::empty::<&str>())
            }
            None => "clear_queue_all".to_string(),
        })),
        "clear_data" => Ok(controller(DiscordControllerAction::StatusForConfirmation {
            username: username.unwrap_or_default(),
            action: "clear_data".to_string(),
        })),
        "stats" | "get_stats" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "stats",
            std::iter::empty::<&str>(),
        ))),
        "ping" | "get_ping" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "ping",
            std::iter::empty::<&str>(),
        ))),
        "buy_flip" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "buy_flip",
            [invocation.required_string("auction_id")?.as_str()],
        ))),
        "list_flip" => {
            let auction_id = invocation.required_string("auction_id")?;
            if let Some(price) = invocation.string("price") {
                Ok(local_terminal(targeted_line(
                    username.as_deref(),
                    "list_flip",
                    [
                        Some(auction_id.as_str()),
                        Some(price),
                        invocation.string("time"),
                    ]
                    .into_iter()
                    .flatten(),
                )))
            } else {
                Ok(local_terminal(targeted_line(
                    username.as_deref(),
                    "list_flip",
                    [
                        Some(auction_id.as_str()),
                        invocation.string("time").map(|_| "--time"),
                        invocation.string("time"),
                    ]
                    .into_iter()
                    .flatten(),
                )))
            }
        }
        "list_item" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "list_item",
            [
                Some(invocation.required_string("item_uuid")?.as_str()),
                Some(invocation.required_string("price")?.as_str()),
                invocation.string("time"),
            ]
            .into_iter()
            .flatten(),
        ))),
        "delist" | "delist_item" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "delist",
            [
                Some(invocation.required_string("auction_id")?.as_str()),
                Some(invocation.required_string("item_uuid")?.as_str()),
            ]
            .into_iter()
            .flatten(),
        ))),
        "bids" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "bids",
            std::iter::empty::<&str>(),
        ))),
        "claim_sold" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "claim_sold",
            std::iter::empty::<&str>(),
        ))),
        "reconcile" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "reconcile",
            std::iter::empty::<&str>(),
        ))),
        "cookie" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "cookie",
            std::iter::empty::<&str>(),
        ))),
        "test_webhook" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "test_webhook",
            std::iter::empty::<&str>(),
        ))),
        "diagslots" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            "diagslots",
            invocation.string("target"),
        ))),
        "bank" | "coins" => Ok(local_terminal(targeted_line(
            username.as_deref(),
            invocation.name.as_str(),
            [
                Some(invocation.required_string("amount")?.as_str()),
                invocation.boolean("withdraw").then_some("withdraw"),
                invocation.boolean("personal").then_some("personal"),
            ]
            .into_iter()
            .flatten(),
        ))),
        "transfer_coins" => Ok(DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Transfer {
                from: invocation.required_string("from")?,
                to: invocation.required_string("to")?,
                amount: invocation
                    .string("amount")
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "all".to_string()),
                stop_source: !matches!(
                    invocation.options.get("stop_source"),
                    Some(CommandOptionValue::Boolean(false))
                ),
                created_at: None,
            },
        }),
        other => Err(DiscordCommandPlanError::UnknownCommand(other.to_string())),
    }
}
