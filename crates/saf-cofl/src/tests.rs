use super::*;
use saf_core::AccountId;
use saf_core::ports::{InventoryItem, InventorySnapshot};
use serde_json::json;

#[test]
fn envelope_parses_flip_payloads() {
    let envelope = CoflEnvelope::from_wire(
        json!({
            "type": "flip",
            "data": serde_json::to_string(&json!({
            "auctionID": "auction-1",
            "itemName": "Hyperion",
            "startingBid": "10m",
            "target": "25m"
            })).unwrap()
        })
        .to_string(),
    )
    .unwrap();

    assert!(envelope.parse_flip().unwrap().is_valid());
}

#[test]
fn envelope_extracts_connection_and_ping_telemetry() {
    let envelope = CoflEnvelope::from_wire(
        json!({
            "type": "chatMessage",
            "data": serde_json::to_string("[Coflnet]:  Your connection id is 0123456789abcdef0123456789abcdef, copy that if you encounter an error").unwrap()
        })
        .to_string(),
    )
    .unwrap();

    assert_eq!(
        envelope.telemetry_update().unwrap(),
        CoflTelemetryUpdate {
            connection_id: Some("0123456789abcdef0123456789abcdef".to_string()),
            ..CoflTelemetryUpdate::default()
        }
    );

    assert_eq!(
        CoflTelemetryUpdate::from_message("The time to receive flips is estimated to be 43.7ms")
            .unwrap()
            .cofl_ping_ms,
        Some(44)
    );
    assert_eq!(
        CoflTelemetryUpdate::from_message("You are currently delayed by 2.5s on api")
            .unwrap()
            .cofl_delay_ms,
        Some(2_500)
    );
    assert_eq!(
        CoflTelemetryUpdate::from_message("You are currently not delayed at all")
            .unwrap()
            .cofl_delay_ms,
        Some(0)
    );
    let update =
        CoflTelemetryUpdate::from_message("You have PREMIUM PLUS until 2026-Oct-15 00:54 UTC")
            .unwrap();
    assert_eq!(update.cofl_tier.as_deref(), Some("Premium Plus"));
    assert_eq!(update.cofl_expires_at, Some(1_792_025_640));
    let update = CoflTelemetryUpdate::from_message(
        "Welcome to Coflnet prem+ instance, connected to main instance",
    )
    .unwrap();
    assert_eq!(update.cofl_tier.as_deref(), Some("Premium Plus"));
    assert_eq!(update.cofl_expires_at, None);
    let update =
        CoflTelemetryUpdate::from_message("You use the FREE version of the flip finder").unwrap();
    assert_eq!(update.cofl_tier.as_deref(), Some("Free"));
}

#[test]
fn envelope_extracts_loaded_settings_summary() {
    let envelope = CoflEnvelope::from_wire(
        json!({
            "type": "chatMessage",
            "data": serde_json::to_string(&json!([
                {"text": "[Coflnet]: "},
                {"text": "Found and loaded settings for your connection\n MinProfit: 20M   Using: stellaconfig v230 \n: nothing else to do have a nice day :)"}
            ])).unwrap()
        })
        .to_string(),
    )
    .unwrap();

    let summary = envelope.settings_summary().unwrap();
    assert_eq!(summary.min_profit.as_deref(), Some("20M"));
    assert_eq!(summary.using.as_deref(), Some("stellaconfig v230"));
    assert_eq!(summary.max_flip_items_in_inventory, None);
}

#[test]
fn envelope_extracts_settings_json_summary() {
    let envelope = CoflEnvelope::from_wire(
        json!({
            "type": "settings",
            "data": {
                "MinProfit": 2_000_000,
                "MinProfitPercent": "1",
                "MaxFlipItemsInInventory": 10,
                "Using": "stellaconfig v230"
            }
        })
        .to_string(),
    )
    .unwrap();

    assert!(envelope.settings_json_available());
    let summary = envelope.settings_json_summary().unwrap();
    assert_eq!(summary.min_profit.as_deref(), Some("2000000"));
    assert_eq!(summary.min_profit_percent.as_deref(), Some("1"));
    assert_eq!(summary.max_flip_items_in_inventory.as_deref(), Some("10"));
    assert_eq!(summary.using.as_deref(), Some("stellaconfig v230"));

    let rows = CoflEnvelope::from_wire(
        json!({
            "type": "settings",
            "data": [
                {"key": "minProfit", "name": "MinProfit", "value": 20_000_000},
                {"key": "minProfitPercent", "name": "MinProfitPercent", "value": 20},
                {"key": "maxFlipItemsInInventory", "name": "MaxFlipItemsInInventory", "value": null}
            ]
        })
        .to_string(),
    )
    .unwrap();
    let summary = rows.settings_json_summary().unwrap();
    assert_eq!(summary.min_profit.as_deref(), Some("20000000"));
    assert_eq!(summary.min_profit_percent.as_deref(), Some("20"));
    assert_eq!(summary.max_flip_items_in_inventory, None);

    let empty = CoflEnvelope::from_wire(
        json!({
            "type": "settings",
            "data": null
        })
        .to_string(),
    )
    .unwrap();
    assert!(!empty.settings_json_available());
    assert_eq!(empty.settings_json_summary(), None);
}

#[test]
fn envelope_extracts_settings_mutations() {
    let min_profit = CoflEnvelope::from_wire(
        json!({
            "type": "chatMessage",
            "data": "[Coflnet]: Set MinProfit to 20000000"
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        min_profit.settings_mutation().unwrap(),
        CoflSettingsMutation {
            min_profit: Some("20000000".to_string()),
            min_profit_percent: None,
        }
    );

    let min_profit_percent = CoflEnvelope::from_wire(
        json!({
            "type": "chatMessage",
            "data": "[Coflnet]: Set MinProfitPercent to 20"
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        min_profit_percent.settings_mutation().unwrap(),
        CoflSettingsMutation {
            min_profit: None,
            min_profit_percent: Some("20".to_string()),
        }
    );
}

#[test]
fn envelope_detects_account_info_acceleration_ack() {
    let envelope = CoflEnvelope::from_wire(
        json!({
            "type": "chatMessage",
            "data": "§b[Coflnet]: §7received account info, starting to speed up flips"
        })
        .to_string(),
    )
    .unwrap();

    assert!(envelope.account_info_acceleration_ack());
}

#[test]
fn passive_chat_messages_are_categorized_without_raw_payload_logging() {
    let whitelist = CoflEnvelope::from_wire(
        json!({
            "type": "writeToChat",
            "data": [{"text": "The auction matched your Whitelist"}]
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(whitelist.passive_chat_category(), Some("matched_whitelist"));

    let no_price = CoflEnvelope::from_wire(
        json!({
            "type": "chatMessage",
            "data": "there was no ah price found for Some Item"
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(no_price.passive_chat_category(), Some("no_ah_price"));

    let settings_blocked = CoflEnvelope::from_wire(
        json!({
            "type": "chatMessage",
            "data": "[Coflnet]: Your settings blocked 194 in the last minute"
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        settings_blocked.passive_chat_category(),
        Some("settings_blocked")
    );
    assert!(!settings_blocked.settings_unavailable());

    let setting_error = CoflEnvelope::from_wire(
        json!({
            "type": "chatMessage",
            "data": "[Coflnet]: the setting minPercent doesn't exist, most similar is minProfit"
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(setting_error.passive_chat_category(), Some("command_error"));

    let max_inventory = CoflEnvelope::from_wire(
        json!({
            "type": "chatMessage",
            "data": "[Coflnet]: Reached max flip items in inventory (1), paused buying until items are sold and listed."
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        max_inventory.passive_chat_category(),
        Some("max_flip_items_in_inventory")
    );

    let settings_load_failed = CoflEnvelope::from_wire(
        json!({
            "type": "writeToChat",
            "data": "[Coflnet]: Your settings could not be loaded, please relink again :)"
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        settings_load_failed.passive_chat_category(),
        Some("settings_load_failed")
    );
    assert!(settings_load_failed.settings_unavailable());

    let free_status = CoflEnvelope::from_wire(
        json!({
            "type": "chatMessage",
            "data": "[Coflnet]: You use the FREE version of the flip finder"
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(free_status.passive_chat_category(), Some("free_status"));
    assert!(free_status.settings_unavailable());

    let throttled = CoflEnvelope::from_wire(
        json!({
            "type": "writeToChat",
            "data": "[Coflnet]: You are executing too many commands please wait a bit"
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(throttled.passive_chat_category(), Some("command_throttled"));

    let command_error = CoflEnvelope::from_wire(
        json!({
            "type": "writeToChat",
            "data": "[Coflnet]: An error occured while processing your command."
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(command_error.passive_chat_category(), Some("command_error"));

    let unknown = CoflEnvelope::from_wire(
        json!({
            "type": "chatMessage",
            "data": "Open https://sky.coflnet.com/authmod?conId=secret and use 0123456789abcdef0123456789abcdef"
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(unknown.passive_chat_category(), Some("auth_link"));
    assert_eq!(
        unknown.passive_chat_excerpt(80).unwrap(),
        "Open [redacted] and use [redacted]"
    );

    let active = CoflEnvelope::from_wire(
        json!({
            "type": "flip",
            "data": {"id": "auction-1"}
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(active.passive_chat_category(), None);
}

#[test]
fn envelope_extracts_execute_instructions() {
    let envelope = CoflEnvelope::from_wire(
        json!({
            "type": "execute",
            "data": "/cofl connect ws://sky-us.coflnet.com/modsocket?region=us"
        })
        .to_string(),
    )
    .unwrap();

    let Some(CoflExecuteInstruction::SwitchSocket { link }) =
        envelope.execute_instruction("Main", "test-session")
    else {
        panic!("expected socket switch instruction");
    };
    let url = url::Url::parse(&link).unwrap();
    assert_eq!(url.scheme(), "wss");
    assert_eq!(url.host_str(), Some("sky-us.coflnet.com"));
    assert_eq!(
        url.query_pairs()
            .find(|(key, _)| key == "region")
            .unwrap()
            .1,
        "us"
    );
    assert_eq!(
        url.query_pairs()
            .find(|(key, _)| key == "version")
            .unwrap()
            .1,
        COFL_SOCKET_VERSION
    );
    assert_eq!(
        url.query_pairs()
            .find(|(key, _)| key == "player")
            .unwrap()
            .1,
        "Main"
    );
    assert_eq!(
        url.query_pairs().find(|(key, _)| key == "SId").unwrap().1,
        "test-session"
    );

    assert_eq!(
        CoflExecuteInstruction::from_command("/cofl get json", "Main", "test-session"),
        Some(CoflExecuteInstruction::SendCoflCommand {
            command: "/cofl get json".to_string()
        })
    );
    assert_eq!(
        CoflExecuteInstruction::from_command("/lobby", "Main", "test-session"),
        Some(CoflExecuteInstruction::BlockedChat {
            command: "/lobby".to_string()
        })
    );
}

#[test]
fn scoreboard_uploads_match_cofl_wire_shape() {
    let wire = encode_scoreboard_upload(&["Purse: 1,250,000".to_string(), "Bits: 120".to_string()])
        .unwrap();
    let value = serde_json::from_str::<Value>(&wire).unwrap();

    assert_eq!(value["type"], "uploadScoreboard");
    assert_eq!(
        serde_json::from_str::<Vec<String>>(value["data"].as_str().unwrap()).unwrap(),
        vec!["Purse: 1,250,000".to_string(), "Bits: 120".to_string()]
    );
}

#[test]
fn cofl_socket_links_are_normalized_and_authenticated() {
    let link = build_cofl_socket_link(
        "/cofl connect ws://sky-us.coflnet.com/modsocket?old=1",
        "MainAccount",
        "session-token",
    )
    .unwrap();
    let parsed = Url::parse(&link).unwrap();

    assert_eq!(
        parsed.origin().ascii_serialization(),
        "wss://sky-us.coflnet.com"
    );
    assert_eq!(parsed.scheme(), "wss");
    assert_eq!(parsed.path(), "/modsocket");
    assert_eq!(
        parsed
            .query_pairs()
            .find(|(key, _)| key == "old")
            .unwrap()
            .1,
        "1"
    );
    assert_eq!(
        parsed
            .query_pairs()
            .find(|(key, _)| key == "version")
            .unwrap()
            .1,
        COFL_SOCKET_VERSION
    );
    assert!(build_cofl_socket_link("/cofl connect missing-url", "Main", "sid").is_none());
}

#[test]
fn cofl_socket_links_can_preserve_embedded_session() {
    let link = build_cofl_socket_link(
        "wss://sky-us.coflnet.com/modsocket?SId=fake&region=us",
        "MainAccount",
        "",
    )
    .unwrap();
    let parsed = Url::parse(&link).unwrap();

    assert_eq!(parsed.scheme(), "wss");
    assert_eq!(
        parsed
            .query_pairs()
            .find(|(key, _)| key == "SId")
            .unwrap()
            .1,
        "fake"
    );
    assert_eq!(
        cofl_socket_session_id("/cofl connect wss://sky-us.coflnet.com/modsocket?SId=fake"),
        Some("fake".to_string())
    );
    assert_eq!(
        cofl_socket_session_id("/cofl connect WSS://sky-us.coflnet.com/modsocket?SId=fake"),
        Some("fake".to_string())
    );

    let link = build_cofl_socket_link(
        "wss://sky-us.coflnet.com/modsocket?SId=fake",
        "MainAccount",
        "override-session",
    )
    .unwrap();
    let parsed = Url::parse(&link).unwrap();
    assert_eq!(
        parsed
            .query_pairs()
            .find(|(key, _)| key == "SId")
            .unwrap()
            .1,
        "override-session"
    );
}

#[test]
fn inventory_requests_match_node_upload_wire_shape() {
    let request = CoflEnvelope::from_wire(
        json!({
            "type": "getInventory",
            "data": {}
        })
        .to_string(),
    )
    .unwrap();
    assert!(request.is_inventory_request());

    let encoded = encode_inventory_upload(&json!({
        "account": "Main",
        "items": [
            {
                "uuid": "item-uuid",
                "itemName": "Aspect of the Dragons"
            }
        ]
    }))
    .unwrap();
    let value = serde_json::from_str::<serde_json::Value>(&encoded).unwrap();
    assert_eq!(value["type"], json!("uploadInventory"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(value["data"].as_str().unwrap()).unwrap(),
        json!({
            "account": "Main",
            "items": [
                {
                    "uuid": "item-uuid",
                    "itemName": "Aspect of the Dragons"
                }
            ]
        })
    );
}

#[test]
fn inventory_snapshot_upload_uses_mineflayer_slot_shape() {
    let encoded = encode_inventory_snapshot_upload(&InventorySnapshot {
        account: AccountId::new("Main").unwrap(),
        items: vec![InventoryItem {
            uuid: Some("uuid-1".to_string()),
            item_name: "Aspect of the Dragons".to_string(),
            lore: vec!["Damage: +225".to_string()],
            price: None,
            tag: Some("ASPECT_OF_THE_DRAGON".to_string()),
            slot: Some(10),
            in_hotbar: false,
        }],
    })
    .unwrap();
    let value = serde_json::from_str::<serde_json::Value>(&encoded).unwrap();
    let inventory =
        serde_json::from_str::<serde_json::Value>(value["data"].as_str().unwrap()).unwrap();

    assert_eq!(value["type"], json!("uploadInventory"));
    assert_eq!(inventory["type"], json!("minecraft:inventory"));
    assert!(inventory["slots"].as_array().unwrap().len() >= 46);
    assert_eq!(
        inventory["slots"][10]["nbt"]["value"]["ExtraAttributes"]["value"]["id"]["value"],
        json!("ASPECT_OF_THE_DRAGON")
    );
    assert_eq!(
        inventory["slots"][10]["nbt"]["value"]["ExtraAttributes"]["value"]["uuid"]["value"],
        json!("uuid-1")
    );
}

#[test]
fn cofl_commands_match_node_wire_format() {
    let encoded = encode_cofl_command("/cofl s minProfit 30m").unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&encoded).unwrap(),
        json!({
            "type": "s",
            "data": "\"minProfit 30m\""
        })
    );

    let encoded = encode_cofl_command("/COFL get json").unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&encoded).unwrap(),
        json!({
            "type": "get",
            "data": "\"json\""
        })
    );

    let encoded = encode_cofl_command("/SAF ping").unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&encoded).unwrap(),
        json!({
            "type": "ping",
            "data": "\"\""
        })
    );

    let encoded = encode_cofl_command("ping").unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&encoded).unwrap(),
        json!({
            "type": "ping",
            "data": "\"\""
        })
    );
}

#[test]
fn privacy_settings_parse_chat_regex() {
    let object = CoflEnvelope::from_wire(
        json!({
            "type": "privacySettings",
            "data": {
                "chatRegex": "^(Party|Guild) >"
            }
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        object.privacy_chat_regex().as_deref(),
        Some("^(Party|Guild) >")
    );

    let stringified = CoflEnvelope::from_wire(
        json!({
            "type": "privacySettings",
            "data": serde_json::to_string(&json!({
                "chatRegex": "^You claimed"
            })).unwrap()
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(
        stringified.privacy_chat_regex().as_deref(),
        Some("^You claimed")
    );
}

#[test]
fn chat_batch_matches_node_wire_shape() {
    let encoded = encode_chat_batch(&[
        "Party > Main: hello".to_string(),
        "You claimed 1 auction!".to_string(),
    ])
    .unwrap();
    let value = serde_json::from_str::<serde_json::Value>(&encoded).unwrap();
    assert_eq!(value["type"], json!("chatBatch"));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(value["data"].as_str().unwrap()).unwrap(),
        json!(["Party > Main: hello", "You claimed 1 auction!"])
    );
}

#[test]
fn logged_out_settings_warning_requests_node_recovery_command() {
    let envelope = CoflEnvelope::from_wire(
        json!({
            "type": "chatMessage",
            "data": "§cUntil you do you are using the free version which will make less profit and your settings won't be saved"
        })
        .to_string(),
    )
    .unwrap();

    assert_eq!(
        envelope.logged_out_settings_recovery_command(),
        Some(LOGGED_OUT_SETTINGS_RECOVERY_COMMAND)
    );

    let unrelated = CoflEnvelope::from_wire(
        json!({
            "type": "chatMessage",
            "data": "You are currently not delayed at all"
        })
        .to_string(),
    )
    .unwrap();
    assert_eq!(unrelated.logged_out_settings_recovery_command(), None);
}

#[test]
fn authmod_login_links_are_extracted_from_chat_payloads() {
    let envelope = CoflEnvelope::from_wire(
        json!({
            "type": "chatMessage",
            "data": [
                {
                    "text": "[§1C§6oflnet§f]§7: ",
                    "onClick": null,
                    "hover": null
                },
                {
                    "text": "Please §lclick https://sky.coflnet.com/authmod?mcid=Main&conId=YKA6UlaBK9Ms98VAYq9WuKg%3d to login",
                    "onClick": null,
                    "hover": null
                },
                {
                    "text": "ignored https://sky.coflnet.com/account",
                    "onClick": null,
                    "hover": null
                }
            ]
        })
        .to_string(),
    )
    .unwrap();

    assert_eq!(
        envelope.auth_links(),
        vec!["https://sky.coflnet.com/authmod?mcid=Main&conId=YKA6UlaBK9Ms98VAYq9WuKg%3d"]
    );
}

#[test]
fn authmod_login_links_are_extracted_from_click_payloads() {
    let envelope = CoflEnvelope::from_wire(
        json!({
            "type": "writeToChat",
            "data": {
                "text": "Click here",
                "onClick": "https://sky.coflnet.com/authmod?conId=abc",
                "hover": "Authorize this SkyCofl connection"
            }
        })
        .to_string(),
    )
    .unwrap();

    assert_eq!(
        envelope.auth_links(),
        vec!["https://sky.coflnet.com/authmod?conId=abc"]
    );
}

#[test]
fn authmod_login_links_are_built_from_hover_connection_ids() {
    let envelope = CoflEnvelope::from_wire(
        json!({
            "type": "writeToChat",
            "data": {
                "text": "Connected to SkyCofl",
                "onClick": "https://discord.gg/wvKXfTgCfb",
                "hover": "Attempting to load your settings on sky-mod-commands conId: 48e8160ac4ad781555f7687d23d6ccf9"
            }
        })
        .to_string(),
    )
    .unwrap();

    assert_eq!(
        envelope.auth_links(),
        vec!["https://sky.coflnet.com/authmod?conId=48e8160ac4ad781555f7687d23d6ccf9"]
    );
}
