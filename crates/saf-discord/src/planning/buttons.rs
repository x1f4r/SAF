use super::helpers::{controller, local_terminal, targeted_line};
use super::{DiscordCommandPlan, DiscordCommandPlanError, DiscordControllerAction};

pub fn plan_button(custom_id: &str) -> Result<Option<DiscordCommandPlan>, DiscordCommandPlanError> {
    let parts = custom_id.split(':').collect::<Vec<_>>();
    if parts.first().copied() != Some("saf") {
        return Err(DiscordCommandPlanError::UnknownCommand(
            custom_id.to_string(),
        ));
    }

    match parts.as_slice() {
        ["saf", "cancel"] => Ok(None),
        ["saf", "dashboard"] => Ok(Some(controller(DiscordControllerAction::Dashboard))),
        ["saf", "users"] => Ok(Some(local_terminal("users".to_string()))),
        ["saf", "blacklist"] => Ok(Some(local_terminal("blacklist list".to_string()))),
        ["saf", "messages"] => Ok(Some(local_terminal("messages".to_string()))),
        ["saf", "logs"] => Ok(Some(local_terminal("logs".to_string()))),
        ["saf", "start"] => Ok(Some(local_terminal("start".to_string()))),
        ["saf", "stop", "all"] => Ok(Some(local_terminal("stop".to_string()))),
        ["saf", "ping"] => Ok(Some(local_terminal("ping".to_string()))),
        ["saf", "globalStats"] => Ok(Some(local_terminal("global_stats".to_string()))),
        ["saf", "connections"] => Ok(Some(local_terminal("connections".to_string()))),
        ["saf", "account", username] => {
            Ok(Some(controller(DiscordControllerAction::AccountPanel {
                username: (*username).to_string(),
            })))
        }
        ["saf", "queue", username] => Ok(Some(local_terminal(targeted_line(
            Some(username),
            "queue",
            std::iter::empty::<&str>(),
        )))),
        ["saf", "inventory", username] => Ok(Some(local_terminal(targeted_line(
            Some(username),
            "inventory",
            std::iter::empty::<&str>(),
        )))),
        ["saf", "reconcile", username] => Ok(Some(local_terminal(targeted_line(
            Some(username),
            "reconcile",
            std::iter::empty::<&str>(),
        )))),
        ["saf", "stop", username] => Ok(Some(local_terminal(targeted_line(
            None,
            "stop",
            [*username],
        )))),
        ["saf", "profit", username] => Ok(Some(local_terminal(targeted_line(
            Some(username),
            "profit",
            std::iter::empty::<&str>(),
        )))),
        ["saf", "stats", username] => Ok(Some(local_terminal(targeted_line(
            Some(username),
            "stats",
            std::iter::empty::<&str>(),
        )))),
        ["saf", "ping", username] => Ok(Some(local_terminal(targeted_line(
            Some(username),
            "ping",
            std::iter::empty::<&str>(),
        )))),
        ["saf", "bids", username] => Ok(Some(local_terminal(targeted_line(
            Some(username),
            "bids",
            std::iter::empty::<&str>(),
        )))),
        ["saf", "cofljson", username] => Ok(Some(local_terminal(targeted_line(
            Some(username),
            "/cofl",
            ["get", "json"],
        )))),
        ["saf", "clearData", username] => Ok(Some(controller(
            DiscordControllerAction::StatusForConfirmation {
                username: (*username).to_string(),
                action: "clear_data".to_string(),
            },
        ))),
        ["saf", "confirmClearData", username] => Ok(Some(local_terminal(targeted_line(
            Some(username),
            "clear_data",
            std::iter::empty::<&str>(),
        )))),
        ["saf", "confirmDelistAll", username] => Ok(Some(local_terminal(targeted_line(
            Some(username),
            "delist_everything",
            std::iter::empty::<&str>(),
        )))),
        ["saf", "delistAll", username] => Ok(Some(controller(
            DiscordControllerAction::DelistEverything {
                username: Some((*username).to_string()),
            },
        ))),
        ["saf", "sellInventory", username, include_hotbar] => {
            Ok(Some(controller(DiscordControllerAction::SellInventory {
                username: Some((*username).to_string()),
                include_hotbar: *include_hotbar == "1",
            })))
        }
        ["saf", "confirmSellInventory", username, include_hotbar] => {
            let include_hotbar = (*include_hotbar == "1").then_some("include_hotbar");
            Ok(Some(local_terminal(targeted_line(
                Some(username),
                "sell_inventory",
                include_hotbar,
            ))))
        }
        _ => Err(DiscordCommandPlanError::UnknownCommand(
            custom_id.to_string(),
        )),
    }
}
