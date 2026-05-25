use super::*;
use saf_core::LocalCommand;
use saf_core::ports::Notification;
use std::collections::BTreeSet;

fn assert_terminal_plan(plan: DiscordCommandPlan, expected: &str) {
    assert_eq!(
        plan,
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: expected.to_string(),
                created_at: None,
            }
        }
    );
}

#[test]
fn replacement_command_surface_matches_node_controller() {
    let commands = command_definitions();
    validate_command_definitions(&commands).unwrap();
    let names = commands
        .iter()
        .map(|command| command.name)
        .collect::<BTreeSet<_>>();

    for name in [
        "dashboard",
        "start_bot",
        "stop_bot",
        "status",
        "send_command",
        "send_terminal",
        "cofl",
        "blacklist",
        "queue",
        "clear_queue",
        "clear_data",
        "logs",
        "messages",
        "stats",
        "ping",
        "profit",
        "global_stats",
        "users",
        "connections",
        "discord_id",
        "help",
        "refresh_heads",
        "buy_flip",
        "list_flip",
        "list_item",
        "delist",
        "delist_everything",
        "delist_all",
        "reconcile",
        "bids",
        "claim_sold",
        "bank",
        "coins",
        "transfer_coins",
        "timeout",
        "start_in",
        "inventory",
        "sell_inventory",
        "test_webhook",
        "diagslots",
        "get_log",
        "get_messages",
        "get_ping",
        "get_profit",
        "get_queue",
        "get_stats",
        "get_users",
        "get_connections",
        "get_discord_id",
        "get_global_stats",
        "delist_item",
        "set_time_in",
        "set_timeout",
    ] {
        assert!(names.contains(name), "{name} command is registered");
    }

    let blacklist = commands
        .iter()
        .find(|command| command.name == "blacklist")
        .unwrap();
    assert!(
        blacklist
            .options
            .iter()
            .any(|option| option.name == "duration")
    );
    let action = blacklist
        .options
        .iter()
        .find(|option| option.name == "action")
        .unwrap();
    assert_eq!(action.description, "Choose list, add, or remove.");
    assert_eq!(
        action
            .choices
            .iter()
            .map(|choice| (choice.name, choice.value))
            .collect::<Vec<_>>(),
        vec![("list", "list"), ("add", "add"), ("remove", "remove")]
    );
    let field = blacklist
        .options
        .iter()
        .find(|option| option.name == "field")
        .unwrap();
    assert!(
        field
            .choices
            .iter()
            .any(|choice| choice.name == "item-enchant" && choice.value == "item-enchant")
    );
}

#[cfg(feature = "serenity-controller")]
#[test]
fn serenity_controller_serializes_registered_command_catalog() {
    let definitions = command_definitions();
    let payloads = serenity_controller::to_serenity_commands(&definitions)
        .into_iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();

    assert_eq!(payloads.len(), definitions.len());
    for definition in definitions {
        let payload = payloads
            .iter()
            .find(|payload| payload["name"] == definition.name)
            .unwrap_or_else(|| panic!("{} command is missing", definition.name));
        assert_eq!(payload["description"], definition.description);
        let options = payload["options"].as_array().unwrap();
        assert_eq!(
            options.len(),
            definition.options.len(),
            "{} option count changed",
            definition.name
        );
        for option in definition.options {
            let payload_option = options
                .iter()
                .find(|payload_option| payload_option["name"] == option.name)
                .unwrap_or_else(|| panic!("{}.{} option is missing", definition.name, option.name));
            assert_eq!(payload_option["description"], option.description);
            assert_eq!(payload_option["required"], option.required);
            let choices = payload_option["choices"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for choice in option.choices {
                assert!(
                    choices.iter().any(|payload_choice| {
                        payload_choice["name"] == choice.name
                            && payload_choice["value"] == choice.value
                    }),
                    "{}.{} choice {} is missing",
                    definition.name,
                    option.name,
                    choice.name
                );
            }
        }
    }
}

#[test]
fn discord_command_routes_match_node_control_surface() {
    for (invocation, expected) in [
        (CommandInvocation::new("start_bot"), "start"),
        (
            CommandInvocation::new("start_bot").with_string("username", "Main"),
            "start Main",
        ),
        (CommandInvocation::new("stop_bot"), "stop"),
        (
            CommandInvocation::new("stop_bot").with_string("username", "Main"),
            "stop Main",
        ),
        (CommandInvocation::new("clear_queue"), "clear_queue_all"),
        (
            CommandInvocation::new("clear_queue").with_string("username", "Main"),
            "Main clear_queue",
        ),
        (CommandInvocation::new("ping"), "ping"),
        (
            CommandInvocation::new("ping").with_string("username", "Main"),
            "Main ping",
        ),
        (
            CommandInvocation::new("stats").with_string("username", "Main"),
            "Main stats",
        ),
        (
            CommandInvocation::new("queue").with_string("username", "Main"),
            "Main queue",
        ),
        (
            CommandInvocation::new("inventory").with_string("username", "Main"),
            "Main inventory",
        ),
        (
            CommandInvocation::new("cofl")
                .with_string("username", "Main")
                .with_string("command", "get json"),
            "Main /cofl get json",
        ),
    ] {
        assert_terminal_plan(plan_invocation(&invocation).unwrap(), expected);
    }

    for (custom_id, expected) in [
        ("saf:start", "start"),
        ("saf:stop:all", "stop"),
        ("saf:stop:Main", "stop Main"),
        ("saf:ping", "ping"),
        ("saf:ping:Main", "Main ping"),
        ("saf:stats:Main", "Main stats"),
        ("saf:queue:Main", "Main queue"),
        ("saf:inventory:Main", "Main inventory"),
        ("saf:reconcile:Main", "Main reconcile"),
        ("saf:bids:Main", "Main bids"),
        ("saf:cofljson:Main", "Main /cofl get json"),
        ("saf:confirmClearData:Main", "Main clear_data"),
        ("saf:confirmDelistAll:Main", "Main delist_everything"),
        (
            "saf:confirmSellInventory:Main:1",
            "Main sell_inventory include_hotbar",
        ),
    ] {
        assert_terminal_plan(plan_button(custom_id).unwrap().unwrap(), expected);
    }
}

#[test]
fn slash_invocations_plan_runtime_commands() {
    let plan = plan_invocation(
        &CommandInvocation::new("bank")
            .with_string("username", "Main")
            .with_string("amount", "50m")
            .with_boolean("withdraw", true)
            .with_boolean("personal", true),
    )
    .unwrap();
    assert_eq!(
        plan,
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "Main bank 50m withdraw personal".to_string(),
                created_at: None,
            }
        }
    );

    let plan =
        plan_invocation(&CommandInvocation::new("clear_data").with_string("username", "Main"))
            .unwrap();
    assert_eq!(
        plan,
        DiscordCommandPlan::ControllerAction {
            action: DiscordControllerAction::StatusForConfirmation {
                username: "Main".to_string(),
                action: "clear_data".to_string(),
            }
        }
    );

    assert_eq!(
        plan_invocation(&CommandInvocation::new("help")).unwrap(),
        DiscordCommandPlan::ControllerAction {
            action: DiscordControllerAction::Help,
        }
    );

    assert_eq!(
        plan_invocation(&CommandInvocation::new("clear_queue")).unwrap(),
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "clear_queue_all".to_string(),
                created_at: None,
            }
        }
    );

    assert_eq!(
        plan_invocation(&CommandInvocation::new("clear_queue").with_string("username", "Main"))
            .unwrap(),
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "Main clear_queue".to_string(),
                created_at: None,
            }
        }
    );

    let plan = plan_invocation(
        &CommandInvocation::new("transfer_coins")
            .with_string("from", "Main")
            .with_string("to", "Alt")
            .with_string("amount", "all"),
    )
    .unwrap();
    assert_eq!(
        plan,
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Transfer {
                from: "Main".to_string(),
                to: "Alt".to_string(),
                amount: "all".to_string(),
                stop_source: true,
                created_at: None,
            }
        }
    );

    let plan = plan_invocation(
        &CommandInvocation::new("blacklist")
            .with_string("action", "add")
            .with_string("scope", "buy")
            .with_string("field", "tag")
            .with_string("value", "SPEED_RELIC")
            .with_string("duration", "7d"),
    )
    .unwrap();
    assert_eq!(
        plan,
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "blacklist add buy tag SPEED_RELIC --duration 7d".to_string(),
                created_at: None,
            }
        }
    );

    let plan = plan_invocation(
        &CommandInvocation::new("timeout")
            .with_string("duration", "30m")
            .with_string("username", "Main"),
    )
    .unwrap();
    assert_eq!(
        plan,
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "timeout 30m Main".to_string(),
                created_at: None,
            }
        }
    );

    let plan =
        plan_invocation(&CommandInvocation::new("profit").with_string("username", "Main")).unwrap();
    assert_eq!(
        plan,
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "Main profit".to_string(),
                created_at: None,
            }
        }
    );

    let plan =
        plan_invocation(&CommandInvocation::new("get_profit").with_string("username", "Main"))
            .unwrap();
    assert_eq!(
        plan,
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "Main profit".to_string(),
                created_at: None,
            }
        }
    );

    assert_eq!(
        plan_invocation(&CommandInvocation::new("reconcile").with_string("username", "Main"))
            .unwrap(),
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "Main reconcile".to_string(),
                created_at: None,
            }
        }
    );

    assert_eq!(
        plan_invocation(&CommandInvocation::new("claim_sold").with_string("username", "Main"))
            .unwrap(),
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "Main claim_sold".to_string(),
                created_at: None,
            }
        }
    );

    assert_eq!(
        plan_invocation(
            &CommandInvocation::new("diagslots")
                .with_string("username", "Main")
                .with_string("target", "current"),
        )
        .unwrap(),
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "Main diagslots current".to_string(),
                created_at: None,
            }
        }
    );

    assert_eq!(
        plan_invocation(&CommandInvocation::new("test_webhook").with_string("username", "Main"))
            .unwrap(),
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "Main test_webhook".to_string(),
                created_at: None,
            }
        }
    );

    assert_eq!(
        plan_invocation(&CommandInvocation::new("users")).unwrap(),
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "users".to_string(),
                created_at: None,
            }
        }
    );

    assert_eq!(
        plan_invocation(&CommandInvocation::new("connections")).unwrap(),
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "connections".to_string(),
                created_at: None,
            }
        }
    );

    assert_eq!(
        plan_invocation(&CommandInvocation::new("messages").with_integer("lines", 12)).unwrap(),
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "messages 12".to_string(),
                created_at: None,
            }
        }
    );

    assert_eq!(
        plan_invocation(&CommandInvocation::new("inventory").with_string("username", "Main"))
            .unwrap(),
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "Main inventory".to_string(),
                created_at: None,
            }
        }
    );
}

#[test]
fn list_flip_without_manual_price_uses_tracked_runtime_path() {
    let plan = plan_invocation(
        &CommandInvocation::new("list_flip")
            .with_string("username", "Main")
            .with_string("auction_id", "auction-1")
            .with_string("time", "12h"),
    )
    .unwrap();

    assert_eq!(
        plan,
        DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "Main list_flip auction-1 --time 12h".to_string(),
                created_at: None,
            }
        }
    );
}

#[test]
fn dashboard_buttons_plan_runtime_commands_and_confirmations() {
    assert_eq!(
        plan_button("saf:queue:Main").unwrap(),
        Some(DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "Main queue".to_string(),
                created_at: None,
            }
        })
    );

    assert_eq!(
        plan_button("saf:account:Main").unwrap(),
        Some(DiscordCommandPlan::ControllerAction {
            action: DiscordControllerAction::AccountPanel {
                username: "Main".to_string(),
            }
        })
    );

    assert_eq!(
        plan_button("saf:delistAll:Main").unwrap(),
        Some(DiscordCommandPlan::ControllerAction {
            action: DiscordControllerAction::DelistEverything {
                username: Some("Main".to_string()),
            }
        })
    );

    assert_eq!(
        plan_button("saf:confirmDelistAll:Main").unwrap(),
        Some(DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "Main delist_everything".to_string(),
                created_at: None,
            }
        })
    );

    assert_eq!(
        plan_button("saf:confirmSellInventory:Main:1").unwrap(),
        Some(DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "Main sell_inventory include_hotbar".to_string(),
                created_at: None,
            }
        })
    );

    assert_eq!(
        plan_button("saf:cofljson:Main").unwrap(),
        Some(DiscordCommandPlan::LocalCommand {
            command: LocalCommand::Terminal {
                line: "Main /cofl get json".to_string(),
                created_at: None,
            }
        })
    );

    assert_eq!(plan_button("saf:cancel").unwrap(), None);
}

#[test]
fn slash_invocations_report_missing_required_options() {
    assert_eq!(
        plan_invocation(&CommandInvocation::new("buy_flip")).unwrap_err(),
        DiscordCommandPlanError::MissingOption("auction_id")
    );
}

#[test]
fn dashboard_rows_stay_within_discord_limit() {
    let summary = DashboardSummary {
        accounts: ["A", "B", "C", "D"]
            .into_iter()
            .map(|name| AccountPanel {
                account: AccountId::new(name).unwrap(),
                state: "idle".to_string(),
                queue_size: 0,
                purse: None,
                current_auctions: None,
                max_auctions: None,
            })
            .collect(),
        external_backend_enabled: false,
        paused: false,
    };

    assert!(summary.button_rows_needed() <= 5);
}

#[test]
fn notification_payload_uses_discord_embed_limits_and_account_footer() {
    let notification = Notification {
        title: "T".repeat(300),
        body: "B".repeat(5000),
        account: Some(AccountId::new("Main").unwrap()),
    };
    let payload = notification_payload(
        &notification,
        &DiscordWebhookIdentity {
            username: Some("SAF".to_string()),
            avatar_url: Some("https://example.invalid/icon.png".to_string()),
        },
    );

    assert_eq!(payload.username.as_deref(), Some("SAF"));
    assert_eq!(payload.embeds.len(), 1);
    assert_eq!(payload.embeds[0].title.chars().count(), 256);
    assert_eq!(payload.embeds[0].description.chars().count(), 4096);
    assert_eq!(
        payload.embeds[0].footer,
        Some(DiscordEmbedFooter {
            text: "Main".to_string()
        })
    );
}
