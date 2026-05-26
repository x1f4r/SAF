use super::*;
#[cfg(feature = "live-cofl")]
use crate::live_runtime::cofl::CoflFlipSafety;
#[cfg(feature = "live-cofl")]
use crate::live_runtime::cofl::is_expected_blocked_chat_command;
#[cfg(feature = "live-cofl")]
use serde_json::Value;
#[cfg(feature = "live-cofl")]
use std::collections::{BTreeMap, BTreeSet};

#[cfg(feature = "live-cofl")]
fn live_buy_window_slots(
    action_name: &str,
    action_display: &str,
) -> Vec<saf_core::gui::WindowSlot> {
    live_buy_window_slots_with_price(action_name, action_display, "30,000,000")
}

#[cfg(feature = "live-cofl")]
fn live_buy_window_slots_with_price(
    action_name: &str,
    action_display: &str,
    price: &str,
) -> Vec<saf_core::gui::WindowSlot> {
    vec![
        saf_core::gui::WindowSlot {
            slot: 13,
            name: "diamond_sword".to_string(),
            display_name: "Hyperion".to_string(),
            lore: vec![format!("Buy it now: {price} coins")],
            item_uuid: None,
        },
        saf_core::gui::WindowSlot {
            slot: 31,
            name: action_name.to_string(),
            display_name: action_display.to_string(),
            lore: vec!["Click to buy".to_string()],
            item_uuid: None,
        },
    ]
}

#[cfg(feature = "live-cofl")]
fn mark_runtime_cofl_settings_loaded(runtime: &LiveRuntime) {
    for stream in &runtime.cofl_streams {
        stream.client.mark_settings_loaded();
    }
}

#[tokio::test]
async fn unmatched_purchase_chat_does_not_increment_bought_stats() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::ChatMessage {
        text: "You purchased Ancient Necron's Leggings for 31,000,000 coins!".to_string(),
    });
    runtime.minecraft_clients.insert(account.clone(), client);

    runtime.poll_minecraft_once().await.unwrap();
    let stats = runtime.stats.stats(&account).await.unwrap();

    assert_eq!(stats.bought, 0);
    assert_eq!(stats.total_profit, 0.0);
}

#[tokio::test]
async fn cofl_telemetry_updates_ping_and_connections() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime
        .stats
        .record_cofl_telemetry(
            &account,
            &CoflTelemetryUpdate {
                connection_id: Some("0123456789abcdef0123456789abcdef".to_string()),
                cofl_ping_ms: Some(44),
                cofl_delay_ms: Some(2_500),
                cofl_tier: Some("Premium Plus".to_string()),
                cofl_expires_at: Some(1_792_025_640),
            },
        )
        .unwrap();

    let ping = runtime.stats.ping(&account).await.unwrap();
    let stats = runtime.stats.stats(&account).await.unwrap();
    let connections = runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "connections".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert_eq!(
        ping,
        AccountPing {
            cofl_delay_ms: Some(2_500),
            cofl_ping_ms: Some(44),
            hypixel_ping_ms: None,
        }
    );
    assert_eq!(stats.cofl_tier.as_deref(), Some("Premium Plus"));
    assert_eq!(stats.cofl_expires_at, Some(1_792_025_640));
    assert!(matches!(
        connections,
        RuntimeOutcome::ConnectionsSnapshot { connections }
            if connections[0].connection_id.as_deref()
                == Some("0123456789abcdef0123456789abcdef")
    ));
}

#[cfg(feature = "live-cofl")]
#[test]
fn cofl_flip_safety_blocks_unsafe_loaded_settings_and_weak_flips() {
    let account = AccountId::new("Main").unwrap();
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let client = LiveCoflClient::new_with_safety(
        account,
        link.clone(),
        "session".to_string(),
        link,
        CoflFlipSafety::new(20_000_000.0, 20.0),
    );
    let safe_flip = FlipEvent::from_payload(&json!({
        "id": "auction-1",
        "itemName": "Hyperion",
        "startingBid": 30_000_000,
        "target": 70_000_000,
        "finder": "SNIPER_MEDIAN"
    }));

    client.replace_settings_summary(&saf_cofl::CoflSettingsSummary {
        min_profit: Some("2M".to_string()),
        min_volume: None,
        min_profit_percent: None,
        max_flip_items_in_inventory: None,
        using: Some("stellaconfig v230".to_string()),
    });
    assert!(
        client
            .flip_safety_violation(&safe_flip)
            .unwrap()
            .contains("MinProfit")
    );

    client.apply_settings_mutation(&saf_cofl::CoflSettingsMutation {
        min_profit: Some("20000000".to_string()),
        min_profit_percent: None,
    });
    assert_eq!(client.flip_safety_violation(&safe_flip), None);

    client.apply_settings_mutation(&saf_cofl::CoflSettingsMutation {
        min_profit: None,
        min_profit_percent: Some("10".to_string()),
    });
    assert!(
        client
            .flip_safety_violation(&safe_flip)
            .unwrap()
            .contains("MinProfitPercent")
    );
    client.apply_settings_mutation(&saf_cofl::CoflSettingsMutation {
        min_profit: None,
        min_profit_percent: Some("20".to_string()),
    });

    let weak_flip = FlipEvent::from_payload(&json!({
        "id": "auction-2",
        "itemName": "Ancient Necron's Leggings",
        "startingBid": 200_000_000,
        "target": 248_000_000,
        "finder": "EXTERNAL"
    }));
    assert!(
        client
            .flip_safety_violation(&weak_flip)
            .unwrap()
            .contains("profit percent")
    );
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_inventory_request_serializes_live_inventory_snapshot() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    session.add_inventory_provider(account.clone(), Arc::new(FixedInventoryProvider));
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let stream = LiveCoflStream {
        account: account.clone(),
        client: Arc::new(LiveCoflClient::new(
            account.clone(),
            link.clone(),
            "session".to_string(),
            link,
        )),
        tracked_flips: Arc::new(LiveTrackedFlipProvider::default()),
        market_actions: MarketActionMode::DryRun,
        allow_execute_chat: false,
        all_flip_notifier: None,
        bed_click_offset: Duration::from_millis(15),
        bed_spam: false,
        bed_click_delay: Duration::from_millis(125),
        pending_live_buys: Arc::new(Mutex::new(BTreeMap::new())),
        humanizer: Arc::new(saf_core::Humanizer::seeded(
            saf_core::HumanizerConfig::default(),
            0,
        )),
        notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
        pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
    };

    let message = stream
        .inventory_upload_message(&session)
        .await
        .unwrap()
        .unwrap();
    let envelope = serde_json::from_str::<Value>(&message).unwrap();
    let inventory = serde_json::from_str::<Value>(
        envelope["data"]
            .as_str()
            .expect("inventory data is a JSON string"),
    )
    .unwrap();

    assert_eq!(envelope["type"], json!("uploadInventory"));
    assert_eq!(inventory["type"], json!("minecraft:inventory"));
    assert!(inventory["slots"].as_array().unwrap().len() >= 46);
    assert_eq!(
        inventory["slots"][10]["nbt"]["value"]["ExtraAttributes"]["value"]["id"]["value"],
        json!("ASPECT_OF_THE_DRAGON")
    );
    assert_eq!(
        inventory["slots"][10]["nbt"]["value"]["ExtraAttributes"]["value"]["uuid"]["value"],
        json!("priced-item")
    );
}

#[cfg(feature = "live-cofl")]
#[derive(Default)]
struct CountingInventoryProvider {
    requests: Arc<Mutex<Vec<AccountId>>>,
}

#[cfg(feature = "live-cofl")]
#[async_trait::async_trait]
impl InventoryProvider for CountingInventoryProvider {
    async fn snapshot(&self, account: &AccountId) -> Result<InventorySnapshot, PortError> {
        self.requests.lock().unwrap().push(account.clone());
        Ok(InventorySnapshot {
            account: account.clone(),
            items: vec![InventoryItem {
                uuid: Some("flip-uuid".to_string()),
                item_name: "Black Cat".to_string(),
                lore: Vec::new(),
                price: Some(70_800_000.0),
                tag: Some("PET_BLACK_CAT".to_string()),
                slot: Some(22),
                in_hotbar: false,
            }],
        })
    }
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_max_inventory_pause_requests_live_inventory_refresh() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    let inventory = Arc::new(CountingInventoryProvider::default());
    session.add_inventory_provider(account.clone(), inventory.clone());
    let stats = LiveStatsProvider::new(vec![account.clone()]);
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let stream = LiveCoflStream {
        account: account.clone(),
        client: Arc::new(LiveCoflClient::new(
            account.clone(),
            link.clone(),
            "session".to_string(),
            link,
        )),
        tracked_flips: Arc::new(LiveTrackedFlipProvider::default()),
        market_actions: MarketActionMode::DryRun,
        allow_execute_chat: false,
        all_flip_notifier: None,
        bed_click_offset: Duration::from_millis(15),
        bed_spam: false,
        bed_click_delay: Duration::from_millis(125),
        pending_live_buys: Arc::new(Mutex::new(BTreeMap::new())),
        humanizer: Arc::new(saf_core::Humanizer::seeded(
            saf_core::HumanizerConfig::default(),
            0,
        )),
        notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
        pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
    };
    let envelope = saf_cofl::CoflEnvelope {
        kind: "chatMessage".to_string(),
        data: json!(
            "[Coflnet]: Reached max flip items in inventory (1), paused buying until items are sold and listed."
        ),
    };

    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, true, envelope)
            .await
    );
    assert_eq!(
        inventory.requests.lock().unwrap().as_slice(),
        std::slice::from_ref(&account)
    );
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_inventory_request_waits_until_market_ready() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    let inventory = Arc::new(CountingInventoryProvider::default());
    session.add_inventory_provider(account.clone(), inventory.clone());
    let stats = LiveStatsProvider::new(vec![account.clone()]);
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let client = Arc::new(LiveCoflClient::new(
        account.clone(),
        link.clone(),
        "session".to_string(),
        link,
    ));
    let stream = LiveCoflStream {
        account: account.clone(),
        client: client.clone(),
        tracked_flips: Arc::new(LiveTrackedFlipProvider::default()),
        market_actions: MarketActionMode::DryRun,
        allow_execute_chat: false,
        all_flip_notifier: None,
        bed_click_offset: Duration::from_millis(15),
        bed_spam: false,
        bed_click_delay: Duration::from_millis(125),
        pending_live_buys: Arc::new(Mutex::new(BTreeMap::new())),
        humanizer: Arc::new(saf_core::Humanizer::seeded(
            saf_core::HumanizerConfig::default(),
            0,
        )),
        notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
        pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
    };
    let envelope = saf_cofl::CoflEnvelope {
        kind: "getInventory".to_string(),
        data: json!({}),
    };

    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, false, envelope)
            .await
    );
    assert!(inventory.requests.lock().unwrap().is_empty());
    assert!(client.deferred_inventory_upload_pending());

    stream
        .flush_deferred_inventory_upload(&session)
        .await
        .unwrap();

    assert_eq!(
        inventory.requests.lock().unwrap().as_slice(),
        std::slice::from_ref(&account)
    );
    assert!(!client.deferred_inventory_upload_pending());
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_privacy_settings_enable_matching_chat_batch_uploads() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let session = RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    let stats = LiveStatsProvider::new(vec![account.clone()]);
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let client = Arc::new(LiveCoflClient::new(
        account.clone(),
        link.clone(),
        "session".to_string(),
        link,
    ));
    let stream = LiveCoflStream {
        account: account.clone(),
        client: client.clone(),
        tracked_flips: Arc::new(LiveTrackedFlipProvider::default()),
        market_actions: MarketActionMode::DryRun,
        allow_execute_chat: false,
        all_flip_notifier: None,
        bed_click_offset: Duration::from_millis(15),
        bed_spam: false,
        bed_click_delay: Duration::from_millis(125),
        pending_live_buys: Arc::new(Mutex::new(BTreeMap::new())),
        humanizer: Arc::new(saf_core::Humanizer::seeded(
            saf_core::HumanizerConfig::default(),
            0,
        )),
        notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
        pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
    };
    let envelope = saf_cofl::CoflEnvelope {
        kind: "privacySettings".to_string(),
        data: json!({
            "chatRegex": "^Party >"
        }),
    };

    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, true, envelope)
            .await
    );

    assert!(
        client
            .chat_batch_upload_message_if_requested("Guild > Main: hidden")
            .unwrap()
            .is_none()
    );
    let message = client
        .chat_batch_upload_message_if_requested("Party > Main: hello")
        .unwrap()
        .unwrap();
    let envelope = serde_json::from_str::<Value>(&message).unwrap();
    assert_eq!(envelope["type"], json!("chatBatch"));
    assert_eq!(
        serde_json::from_str::<Vec<String>>(
            envelope["data"]
                .as_str()
                .expect("chat batch data is a JSON string")
        )
        .unwrap(),
        vec!["Party > Main: hello".to_string()]
    );
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_auth_links_are_forwarded_to_operator_once() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    let notifier = Arc::new(RecordingNotifier::default());
    session.set_notifier(notifier.clone());
    let stats = LiveStatsProvider::new(vec![account.clone()]);
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let stream = LiveCoflStream {
        account: account.clone(),
        client: Arc::new(LiveCoflClient::new(
            account.clone(),
            link.clone(),
            "session".to_string(),
            link,
        )),
        tracked_flips: Arc::new(LiveTrackedFlipProvider::default()),
        market_actions: MarketActionMode::DryRun,
        allow_execute_chat: false,
        all_flip_notifier: None,
        bed_click_offset: Duration::from_millis(15),
        bed_spam: false,
        bed_click_delay: Duration::from_millis(125),
        pending_live_buys: Arc::new(Mutex::new(BTreeMap::new())),
        humanizer: Arc::new(saf_core::Humanizer::seeded(
            saf_core::HumanizerConfig::default(),
            0,
        )),
        notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
        pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
    };
    let envelope = saf_cofl::CoflEnvelope {
        kind: "chatMessage".to_string(),
        data: json!([{
            "text": "Please click https://sky.coflnet.com/authmod?mcid=Main&conId=abc to login"
        }]),
    };

    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, true, envelope.clone())
            .await
    );
    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, true, envelope)
            .await
    );

    assert!(notifier.notifications.lock().unwrap().is_empty());
    {
        let mut pending = stream.pending_auth_links.lock().unwrap();
        assert_eq!(pending.len(), 1);
        for first_seen in pending.values_mut() {
            *first_seen = Instant::now() - Duration::from_secs(9);
        }
    }
    stream.flush_pending_auth_links_best_effort(&session).await;

    let notifications = notifier.notifications.lock().unwrap();
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].title, "SkyCofl Login Required");
    assert_eq!(notifications[0].account.as_ref(), Some(&account));
    assert!(
        notifications[0]
            .body
            .contains("https://sky.coflnet.com/authmod?mcid=Main&conId=abc")
    );
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_auth_link_is_suppressed_when_settings_json_arrives() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    let notifier = Arc::new(RecordingNotifier::default());
    session.set_notifier(notifier.clone());
    let stats = LiveStatsProvider::new(vec![account.clone()]);
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let stream = LiveCoflStream {
        account: account.clone(),
        client: Arc::new(LiveCoflClient::new(
            account.clone(),
            link.clone(),
            "session".to_string(),
            link,
        )),
        tracked_flips: Arc::new(LiveTrackedFlipProvider::default()),
        market_actions: MarketActionMode::DryRun,
        allow_execute_chat: false,
        all_flip_notifier: None,
        bed_click_offset: Duration::from_millis(15),
        bed_spam: false,
        bed_click_delay: Duration::from_millis(125),
        pending_live_buys: Arc::new(Mutex::new(BTreeMap::new())),
        humanizer: Arc::new(saf_core::Humanizer::seeded(
            saf_core::HumanizerConfig::default(),
            0,
        )),
        notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
        pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
    };

    assert!(
        stream
            .handle_envelope_best_effort(
                &session,
                &stats,
                true,
                saf_cofl::CoflEnvelope {
                    kind: "chatMessage".to_string(),
                    data: json!([{
                        "text": "Please click https://sky.coflnet.com/authmod?mcid=Main&conId=abc to login"
                    }]),
                },
            )
            .await
    );
    assert_eq!(stream.pending_auth_links.lock().unwrap().len(), 1);

    assert!(
        stream
            .handle_envelope_best_effort(
                &session,
                &stats,
                true,
                saf_cofl::CoflEnvelope {
                    kind: "settings".to_string(),
                    data: json!({"MinProfit": "20M"}),
                },
            )
            .await
    );
    stream.flush_pending_auth_links_best_effort(&session).await;

    assert!(stream.pending_auth_links.lock().unwrap().is_empty());
    assert!(notifier.notifications.lock().unwrap().is_empty());
}

#[cfg(feature = "live-cofl")]
#[test]
fn cofl_initial_scoreboard_upload_is_once_per_connection() {
    let account = AccountId::new("Main").unwrap();
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let client = LiveCoflClient::new(account, link.clone(), "session".to_string(), link);
    let connected_at = Instant::now();
    let lines = vec![
        "§bBits: §3120".to_string(),
        "\u{200B}Purse: 1.2M".to_string(),
        "Purse: 1.2M".to_string(),
    ];

    assert!(
        client
            .initial_scoreboard_upload_message_at(
                connected_at,
                connected_at + Duration::from_secs(5),
                &lines,
            )
            .unwrap()
            .is_none()
    );

    let message = client
        .initial_scoreboard_upload_message_at(
            connected_at,
            connected_at + Duration::from_millis(5_500),
            &lines,
        )
        .unwrap()
        .unwrap();
    let envelope = serde_json::from_str::<Value>(&message).unwrap();
    assert_eq!(envelope["type"], json!("uploadScoreboard"));
    assert_eq!(
        serde_json::from_str::<Vec<String>>(
            envelope["data"]
                .as_str()
                .expect("scoreboard data is a JSON string")
        )
        .unwrap(),
        vec!["Bits: 120".to_string(), "Purse: 1.2M".to_string()]
    );

    client.mark_initial_scoreboard_uploaded(connected_at);
    assert!(
        client
            .initial_scoreboard_upload_message_at(
                connected_at,
                connected_at + Duration::from_secs(6),
                &["Purse: 2M".to_string()],
            )
            .unwrap()
            .is_none()
    );
    assert!(
        client
            .initial_scoreboard_upload_message_at(
                connected_at + Duration::from_secs(1),
                connected_at + Duration::from_secs(7),
                &["Purse: 2M".to_string()],
            )
            .unwrap()
            .is_some()
    );
    assert!(
        client
            .initial_scoreboard_upload_message_at(
                connected_at + Duration::from_secs(1),
                connected_at + Duration::from_secs(7),
                &["Your Island".to_string()],
            )
            .unwrap()
            .is_none()
    );
}

#[cfg(feature = "live-cofl")]
#[test]
fn cofl_startup_account_info_retries_until_acknowledged() {
    let account = AccountId::new("Main").unwrap();
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let client = LiveCoflClient::new(account, link.clone(), "session".to_string(), link);
    let connected_at = Instant::now();

    assert!(client.startup_account_info_request_due_at(connected_at, connected_at));

    client.mark_startup_account_info_requested_at(connected_at, connected_at);
    assert!(
        !client.startup_account_info_request_due_at(
            connected_at,
            connected_at + Duration::from_secs(9)
        )
    );
    assert!(
        client.startup_account_info_request_due_at(
            connected_at,
            connected_at + Duration::from_secs(10)
        )
    );

    client.mark_startup_account_info_accepted();
    assert!(
        !client.startup_account_info_request_due_at(
            connected_at,
            connected_at + Duration::from_secs(60)
        )
    );

    client.reset_startup_account_info_state();
    assert!(client.startup_account_info_request_due_at(
        connected_at + Duration::from_secs(1),
        connected_at + Duration::from_secs(1)
    ));
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_envelope_errors_are_account_scoped() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let session = RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    let stats = LiveStatsProvider::new(vec![account.clone()]);
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let stream = LiveCoflStream {
        account: account.clone(),
        client: Arc::new(LiveCoflClient::new(
            account.clone(),
            link.clone(),
            "session".to_string(),
            link,
        )),
        tracked_flips: Arc::new(LiveTrackedFlipProvider::default()),
        market_actions: MarketActionMode::Live,
        allow_execute_chat: false,
        all_flip_notifier: None,
        bed_click_offset: Duration::from_millis(15),
        bed_spam: false,
        bed_click_delay: Duration::from_millis(125),
        pending_live_buys: Arc::new(Mutex::new(BTreeMap::new())),
        humanizer: Arc::new(saf_core::Humanizer::seeded(
            saf_core::HumanizerConfig::default(),
            0,
        )),
        notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
        pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
    };
    stream.client.mark_settings_loaded();
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Hyperion",
            "startingBid": 30_000_000,
            "target": 50_000_000,
            "finder": "SNIPER_MEDIAN"
        }),
    };

    assert!(
        !stream
            .handle_envelope_best_effort(&session, &stats, true, envelope)
            .await
    );
    assert_eq!(stream.tracked_flips.flips.lock().unwrap().len(), 1);
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_flip_dry_run_tracks_without_opening_auction() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    session.add_minecraft_client(account.clone(), minecraft.clone());
    let stats = LiveStatsProvider::new(vec![account.clone()]);
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let stream = LiveCoflStream {
        account: account.clone(),
        client: Arc::new(LiveCoflClient::new(
            account.clone(),
            link.clone(),
            "session".to_string(),
            link,
        )),
        tracked_flips: Arc::new(LiveTrackedFlipProvider::default()),
        market_actions: MarketActionMode::DryRun,
        allow_execute_chat: false,
        all_flip_notifier: None,
        bed_click_offset: Duration::from_millis(15),
        bed_spam: false,
        bed_click_delay: Duration::from_millis(125),
        pending_live_buys: Arc::new(Mutex::new(BTreeMap::new())),
        humanizer: Arc::new(saf_core::Humanizer::seeded(
            saf_core::HumanizerConfig::default(),
            0,
        )),
        notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
        pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
    };
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Hyperion",
            "startingBid": 30_000_000,
            "target": 50_000_000,
            "finder": "SNIPER_MEDIAN"
        }),
    };

    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, true, envelope)
            .await
    );

    assert_eq!(stream.tracked_flips.flips.lock().unwrap().len(), 1);
    assert!(minecraft.actions().is_empty());
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_flip_live_settings_gate_tracks_without_opening_auction() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    session.add_minecraft_client(account.clone(), minecraft.clone());
    let stats = LiveStatsProvider::new(vec![account.clone()]);
    let pending_live_buys = Arc::new(Mutex::new(BTreeMap::new()));
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let stream = LiveCoflStream {
        account: account.clone(),
        client: Arc::new(LiveCoflClient::new(
            account.clone(),
            link.clone(),
            "session".to_string(),
            link,
        )),
        tracked_flips: Arc::new(LiveTrackedFlipProvider::default()),
        market_actions: MarketActionMode::Live,
        allow_execute_chat: false,
        all_flip_notifier: None,
        bed_click_offset: Duration::from_millis(15),
        bed_spam: false,
        bed_click_delay: Duration::from_millis(125),
        pending_live_buys: pending_live_buys.clone(),
        humanizer: Arc::new(saf_core::Humanizer::seeded(
            saf_core::HumanizerConfig::default(),
            0,
        )),
        notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
        pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
    };
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Hyperion",
            "startingBid": 30_000_000,
            "target": 50_000_000,
            "finder": "SNIPER_MEDIAN"
        }),
    };

    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, true, envelope)
            .await
    );

    assert_eq!(stream.tracked_flips.flips.lock().unwrap().len(), 1);
    assert!(minecraft.actions().is_empty());
    assert!(pending_live_buys.lock().unwrap().is_empty());
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_settings_payloads_enable_live_flips_and_load_failure_disables_them() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    session.add_minecraft_client(account.clone(), minecraft.clone());
    let stats = LiveStatsProvider::new(vec![account.clone()]);
    let pending_live_buys = Arc::new(Mutex::new(BTreeMap::new()));
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let stream = LiveCoflStream {
        account: account.clone(),
        client: Arc::new(LiveCoflClient::new(
            account.clone(),
            link.clone(),
            "session".to_string(),
            link,
        )),
        tracked_flips: Arc::new(LiveTrackedFlipProvider::default()),
        market_actions: MarketActionMode::Live,
        allow_execute_chat: false,
        all_flip_notifier: None,
        bed_click_offset: Duration::from_millis(15),
        bed_spam: false,
        bed_click_delay: Duration::from_millis(125),
        pending_live_buys: pending_live_buys.clone(),
        humanizer: Arc::new(saf_core::Humanizer::seeded(
            saf_core::HumanizerConfig::default(),
            0,
        )),
        notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
        pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
    };

    let settings_loaded = saf_cofl::CoflEnvelope {
        kind: "chatMessage".to_string(),
        data: json!([
            {"text": "[Coflnet]: "},
            {"text": "Found and loaded settings for your connection\n MinProfit: 20M   Using: test profile"}
        ]),
    };
    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, true, settings_loaded)
            .await
    );
    assert!(stream.client.settings_loaded());

    let first_flip = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Hyperion",
            "startingBid": 30_000_000,
            "target": 50_000_000,
            "finder": "SNIPER_MEDIAN"
        }),
    };
    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, true, first_flip)
            .await
    );
    assert_eq!(
        minecraft.actions(),
        vec![MinecraftAction::OpenAuction(
            saf_core::AuctionId::new("auction-1").unwrap()
        )]
    );

    let settings_unavailable = saf_cofl::CoflEnvelope {
        kind: "writeToChat".to_string(),
        data: json!("[Coflnet]: Your settings could not be loaded, please relink again :)"),
    };
    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, true, settings_unavailable)
            .await
    );
    assert!(!stream.client.settings_loaded());
    pending_live_buys.lock().unwrap().clear();
    let actions_before_second_flip = minecraft.actions().len();

    let second_flip = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-2",
            "itemName": "Hyperion",
            "startingBid": 30_000_000,
            "target": 50_000_000,
            "finder": "SNIPER_MEDIAN"
        }),
    };
    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, true, second_flip)
            .await
    );

    assert_eq!(minecraft.actions().len(), actions_before_second_flip);
    assert!(pending_live_buys.lock().unwrap().is_empty());

    let settings_json = saf_cofl::CoflEnvelope {
        kind: "settings".to_string(),
        data: json!({
            "MinProfit": "20M",
            "Using": "test profile"
        }),
    };
    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, true, settings_json)
            .await
    );
    assert!(stream.client.settings_loaded());
    assert!(!stream.client.startup_account_info_request_due_at(
        Instant::now(),
        Instant::now() + Duration::from_secs(60)
    ));

    let third_flip = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-3",
            "itemName": "Hyperion",
            "startingBid": 30_000_000,
            "target": 50_000_000,
            "finder": "SNIPER_MEDIAN"
        }),
    };
    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, true, third_flip)
            .await
    );

    let actions = minecraft.actions();
    assert_eq!(actions.len(), actions_before_second_flip + 1);
    assert_eq!(
        actions.last(),
        Some(&MinecraftAction::OpenAuction(
            saf_core::AuctionId::new("auction-3").unwrap()
        ))
    );
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_flip_live_market_gate_tracks_without_opening_auction() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    session.add_minecraft_client(account.clone(), minecraft.clone());
    let stats = LiveStatsProvider::new(vec![account.clone()]);
    let pending_live_buys = Arc::new(Mutex::new(BTreeMap::new()));
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let stream = LiveCoflStream {
        account: account.clone(),
        client: Arc::new(LiveCoflClient::new(
            account.clone(),
            link.clone(),
            "session".to_string(),
            link,
        )),
        tracked_flips: Arc::new(LiveTrackedFlipProvider::default()),
        market_actions: MarketActionMode::Live,
        allow_execute_chat: false,
        all_flip_notifier: None,
        bed_click_offset: Duration::from_millis(15),
        bed_spam: false,
        bed_click_delay: Duration::from_millis(125),
        pending_live_buys: pending_live_buys.clone(),
        humanizer: Arc::new(saf_core::Humanizer::seeded(
            saf_core::HumanizerConfig::default(),
            0,
        )),
        notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
        pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
    };
    stream.client.mark_settings_loaded();
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Hyperion",
            "startingBid": 30_000_000,
            "target": 50_000_000,
            "finder": "SNIPER_MEDIAN"
        }),
    };

    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, false, envelope)
            .await
    );

    assert_eq!(stream.tracked_flips.flips.lock().unwrap().len(), 1);
    assert!(minecraft.actions().is_empty());
    assert!(pending_live_buys.lock().unwrap().is_empty());
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn live_cofl_flip_skip_policy_opens_auction_and_tracks_pending_buy() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        session: "session".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let runtime = LiveRuntime::start(config, options).await.unwrap();
    mark_runtime_cofl_settings_loaded(&runtime);
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .managed_minecraft
        .get(&account)
        .unwrap()
        .replace_minecraft_for_test(minecraft.clone())
        .await;
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Ancient Skeleton Master Chestplate",
            "startingBid": 70,
            "target": 76_000_000,
            "finder": "STONKS"
        }),
    };

    {
        let stream = runtime.cofl_streams.first().unwrap();
        assert!(
            stream
                .handle_envelope_best_effort(&runtime.session, &runtime.stats, true, envelope)
                .await
        );
    }

    assert_eq!(
        minecraft.actions(),
        vec![MinecraftAction::OpenAuction(
            saf_core::AuctionId::new("auction-1").unwrap()
        )]
    );
    let pending = runtime.pending_live_buy(&account).unwrap().unwrap();
    assert_eq!(
        pending.auction_id,
        saf_core::AuctionId::new("auction-1").unwrap()
    );
    assert_eq!(pending.expected_price, 70.0);
    runtime.shutdown().await;
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn live_cofl_flip_clicks_buy_action_after_window_opens() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        session: "session".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    mark_runtime_cofl_settings_loaded(&runtime);
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .managed_minecraft
        .get(&account)
        .unwrap()
        .replace_minecraft_for_test(minecraft.clone())
        .await;
    runtime
        .remember_window_snapshot(
            account.clone(),
            WindowSnapshot {
                title: "BIN Auction View".to_string(),
                slots: vec![saf_core::gui::WindowSlot {
                    slot: 31,
                    name: "gold_nugget".to_string(),
                    display_name: "Buy Item".to_string(),
                    lore: vec!["stale window from before the flip".to_string()],
                    item_uuid: None,
                }],
            },
        )
        .unwrap();
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Hyperion",
            "startingBid": 30_000_000,
            "target": 50_000_000,
            "finder": "SNIPER_MEDIAN"
        }),
    };

    {
        let stream = runtime.cofl_streams.first().unwrap();
        assert!(
            stream
                .handle_envelope_best_effort(&runtime.session, &runtime.stats, true, envelope)
                .await
        );
    }

    assert_eq!(
        minecraft.actions(),
        vec![MinecraftAction::OpenAuction(
            saf_core::AuctionId::new("auction-1").unwrap()
        )]
    );
    assert!(runtime.pending_live_buy(&account).unwrap().is_some());

    runtime.process_pending_live_buys_once().await.unwrap();
    assert_eq!(
        minecraft.actions(),
        vec![MinecraftAction::OpenAuction(
            saf_core::AuctionId::new("auction-1").unwrap()
        )]
    );

    minecraft.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: live_buy_window_slots("gold_nugget", "Buy Item"),
    }));

    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_pending_live_buys_once().await.unwrap();

    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::ClickSlot(31)
        ]
    );
    assert_eq!(
        runtime
            .pending_live_buy(&account)
            .unwrap()
            .unwrap()
            .action_clicks,
        1
    );
    runtime
        .pending_live_buys
        .lock()
        .unwrap()
        .get_mut(&account)
        .unwrap()
        .action_clicked_at = Some(Instant::now() - Duration::from_secs(6));
    runtime.process_pending_live_buys_once().await.unwrap();
    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::CloseWindow
        ]
    );
    assert!(runtime.pending_live_buy(&account).unwrap().is_none());
    runtime.shutdown().await;
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn live_cofl_buy_accepts_price_on_buy_action_lore() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        session: "session".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    mark_runtime_cofl_settings_loaded(&runtime);
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .managed_minecraft
        .get(&account)
        .unwrap()
        .replace_minecraft_for_test(minecraft.clone())
        .await;
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Precise Mosquito Shortbow",
            "startingBid": 65_000_000,
            "target": 89_114_671,
            "finder": "CraftCost"
        }),
    };

    {
        let stream = runtime.cofl_streams.first().unwrap();
        assert!(
            stream
                .handle_envelope_best_effort(&runtime.session, &runtime.stats, true, envelope)
                .await
        );
    }

    minecraft.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 31,
            name: "gold_nugget".to_string(),
            display_name: "Buy Item Right Now".to_string(),
            lore: vec!["65,000,000 coins".to_string(), "Click to buy".to_string()],
            item_uuid: None,
        }],
    }));

    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_pending_live_buys_once().await.unwrap();

    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::ClickSlot(31),
        ]
    );
    runtime.shutdown().await;
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn live_cofl_buy_waits_for_visible_price_before_clicking() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        session: "session".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    mark_runtime_cofl_settings_loaded(&runtime);
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .managed_minecraft
        .get(&account)
        .unwrap()
        .replace_minecraft_for_test(minecraft.clone())
        .await;
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Precise Mosquito Shortbow",
            "startingBid": 65_000_000,
            "target": 89_114_671,
            "finder": "CraftCost"
        }),
    };

    {
        let stream = runtime.cofl_streams.first().unwrap();
        assert!(
            stream
                .handle_envelope_best_effort(&runtime.session, &runtime.stats, true, envelope)
                .await
        );
    }

    minecraft.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 13,
                name: "bow".to_string(),
                display_name: "Precise Mosquito Shortbow".to_string(),
                lore: vec!["Seller: example".to_string()],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 31,
                name: "black_stained_glass_pane".to_string(),
                display_name: "Black Stained Glass Pane".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
        ],
    }));

    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_pending_live_buys_once().await.unwrap();

    assert_eq!(
        minecraft.actions(),
        vec![MinecraftAction::OpenAuction(
            saf_core::AuctionId::new("auction-1").unwrap()
        )]
    );
    assert!(runtime.pending_live_buy(&account).unwrap().is_some());

    minecraft.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: live_buy_window_slots_with_price("gold_nugget", "Buy Item Right Now", "65,000,000"),
    }));

    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_pending_live_buys_once().await.unwrap();

    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::ClickSlot(31),
        ]
    );
    runtime.shutdown().await;
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn live_cofl_buy_blocks_missing_visible_price_after_wait() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        session: "session".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    mark_runtime_cofl_settings_loaded(&runtime);
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .managed_minecraft
        .get(&account)
        .unwrap()
        .replace_minecraft_for_test(minecraft.clone())
        .await;
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Precise Mosquito Shortbow",
            "startingBid": 65_000_000,
            "target": 89_114_671,
            "finder": "CraftCost"
        }),
    };

    {
        let stream = runtime.cofl_streams.first().unwrap();
        assert!(
            stream
                .handle_envelope_best_effort(&runtime.session, &runtime.stats, true, envelope)
                .await
        );
    }

    minecraft.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 31,
            name: "black_stained_glass_pane".to_string(),
            display_name: "Black Stained Glass Pane".to_string(),
            lore: Vec::new(),
            item_uuid: None,
        }],
    }));

    runtime.poll_minecraft_once().await.unwrap();
    if let Some(pending) = runtime.pending_live_buys.lock().unwrap().get_mut(&account) {
        pending.created_at = Instant::now() - Duration::from_secs(2);
    }
    runtime.process_pending_live_buys_once().await.unwrap();

    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::CloseWindow,
        ]
    );
    assert!(runtime.pending_live_buy(&account).unwrap().is_none());
    runtime.shutdown().await;
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn live_cofl_buy_accepts_price_on_gold_block_action_lore() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        session: "session".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    mark_runtime_cofl_settings_loaded(&runtime);
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .managed_minecraft
        .get(&account)
        .unwrap()
        .replace_minecraft_for_test(minecraft.clone())
        .await;
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Submerged Shark Scale Leggings",
            "startingBid": 5_000_000,
            "target": 25_687_181,
            "finder": "SNIPER_MEDIAN"
        }),
    };

    {
        let stream = runtime.cofl_streams.first().unwrap();
        assert!(
            stream
                .handle_envelope_best_effort(&runtime.session, &runtime.stats, true, envelope)
                .await
        );
    }

    minecraft.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 31,
            name: "gold_block".to_string(),
            display_name: "Click to buy".to_string(),
            lore: vec!["5,000,000 coins".to_string()],
            item_uuid: None,
        }],
    }));

    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_pending_live_buys_once().await.unwrap();

    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::ClickSlot(31),
        ]
    );
    runtime.shutdown().await;
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn live_cofl_buy_blocks_action_lore_price_above_expected_bid() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        session: "session".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    mark_runtime_cofl_settings_loaded(&runtime);
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .managed_minecraft
        .get(&account)
        .unwrap()
        .replace_minecraft_for_test(minecraft.clone())
        .await;
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Precise Mosquito Shortbow",
            "startingBid": 65_000_000,
            "target": 89_114_671,
            "finder": "CraftCost"
        }),
    };

    {
        let stream = runtime.cofl_streams.first().unwrap();
        assert!(
            stream
                .handle_envelope_best_effort(&runtime.session, &runtime.stats, true, envelope)
                .await
        );
    }

    minecraft.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 31,
            name: "gold_nugget".to_string(),
            display_name: "Buy Item Right Now".to_string(),
            lore: vec!["80,000,000 coins".to_string(), "Click to buy".to_string()],
            item_uuid: None,
        }],
    }));

    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_pending_live_buys_once().await.unwrap();

    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::CloseWindow,
        ]
    );
    assert!(runtime.pending_live_buy(&account).unwrap().is_none());
    runtime.shutdown().await;
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn live_cofl_buy_blocks_visible_price_above_expected_bid() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        session: "session".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    mark_runtime_cofl_settings_loaded(&runtime);
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .managed_minecraft
        .get(&account)
        .unwrap()
        .replace_minecraft_for_test(minecraft.clone())
        .await;
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Necrotic Fiery Aurora Boots",
            "startingBid": 30_000_000,
            "target": 50_000_000,
            "finder": "SNIPER_MEDIAN"
        }),
    };

    {
        let stream = runtime.cofl_streams.first().unwrap();
        assert!(
            stream
                .handle_envelope_best_effort(&runtime.session, &runtime.stats, true, envelope)
                .await
        );
    }

    minecraft.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: live_buy_window_slots_with_price("gold_nugget", "Buy Item", "90,000,000"),
    }));

    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_pending_live_buys_once().await.unwrap();

    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::CloseWindow,
        ]
    );
    assert!(runtime.pending_live_buy(&account).unwrap().is_none());
    assert!(
        runtime
            .active_windows
            .lock()
            .unwrap()
            .get(&account)
            .is_none()
    );
    runtime.shutdown().await;
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn live_cofl_buy_closes_unrelated_window_before_clicking() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        session: "session".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    mark_runtime_cofl_settings_loaded(&runtime);
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .managed_minecraft
        .get(&account)
        .unwrap()
        .replace_minecraft_for_test(minecraft.clone())
        .await;
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Hyperion",
            "startingBid": 30_000_000,
            "target": 50_000_000,
            "finder": "SNIPER_MEDIAN"
        }),
    };

    {
        let stream = runtime.cofl_streams.first().unwrap();
        assert!(
            stream
                .handle_envelope_best_effort(&runtime.session, &runtime.stats, true, envelope)
                .await
        );
    }

    minecraft.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 81,
            name: "diamond_chestplate".to_string(),
            display_name: "Inventory Item".to_string(),
            lore: vec!["Not an auction buy action.".to_string()],
            item_uuid: Some("inventory-item".to_string()),
        }],
    }));

    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_pending_live_buys_once().await.unwrap();

    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::CloseWindow,
        ]
    );
    assert!(runtime.pending_live_buy(&account).unwrap().is_some());
    assert!(
        runtime
            .active_windows
            .lock()
            .unwrap()
            .get(&account)
            .is_none()
    );

    if let Some(pending) = runtime.pending_live_buys.lock().unwrap().get_mut(&account) {
        pending.last_attempt = Some(Instant::now() - MARKET_STEP_RETRY_INTERVAL);
    }
    minecraft.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: live_buy_window_slots("gold_nugget", "Buy Item Right Now"),
    }));

    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_pending_live_buys_once().await.unwrap();

    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::CloseWindow,
            MinecraftAction::ClickSlot(31),
        ]
    );
    assert!(runtime.pending_live_buy(&account).unwrap().is_some());
    minecraft.push_event(MinecraftEvent::WindowClosed);
    runtime.poll_minecraft_once().await.unwrap();
    assert!(runtime.pending_live_buy(&account).unwrap().is_none());
    runtime.shutdown().await;
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn live_cofl_buy_retries_confirm_purchase_until_window_closes() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        session: "session".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    mark_runtime_cofl_settings_loaded(&runtime);
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .managed_minecraft
        .get(&account)
        .unwrap()
        .replace_minecraft_for_test(minecraft.clone())
        .await;
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Hyperion",
            "startingBid": 30_000_000,
            "target": 50_000_000,
            "finder": "SNIPER_MEDIAN"
        }),
    };

    {
        let stream = runtime.cofl_streams.first().unwrap();
        assert!(
            stream
                .handle_envelope_best_effort(&runtime.session, &runtime.stats, true, envelope)
                .await
        );
    }

    minecraft.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: live_buy_window_slots("gold_nugget", "Buy Item Right Now"),
    }));
    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_pending_live_buys_once().await.unwrap();

    minecraft.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Confirm Purchase".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 11,
            name: "gold_nugget".to_string(),
            display_name: "Confirm Purchase".to_string(),
            lore: vec!["Click to buy".to_string()],
            item_uuid: None,
        }],
    }));
    {
        let mut pending = runtime.pending_live_buys.lock().unwrap();
        let pending = pending.get_mut(&account).unwrap();
        pending.buy_action_retry_delay = Duration::from_millis(55);
        pending.last_click = Some(Instant::now() - Duration::from_millis(60));
    }
    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_pending_live_buys_once().await.unwrap();

    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::ClickSlot(11),
        ]
    );
    assert_eq!(
        runtime
            .pending_live_buy(&account)
            .unwrap()
            .unwrap()
            .action_clicks,
        2
    );
    minecraft.push_event(MinecraftEvent::WindowClosed);
    runtime.poll_minecraft_once().await.unwrap();
    assert!(runtime.pending_live_buy(&account).unwrap().is_none());
    runtime.shutdown().await;
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn live_cofl_timed_bed_waits_for_purchase_time_before_clicking() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        session: "session".to_string(),
        waittime: 15,
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    mark_runtime_cofl_settings_loaded(&runtime);
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .managed_minecraft
        .get(&account)
        .unwrap()
        .replace_minecraft_for_test(minecraft.clone())
        .await;
    let purchase_at = now_ms() + 60_000;
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Hyperion",
            "startingBid": 30_000_000,
            "target": 50_000_000,
            "finder": "SNIPER_MEDIAN",
            "purchaseAt": purchase_at
        }),
    };

    {
        let stream = runtime.cofl_streams.first().unwrap();
        assert!(
            stream
                .handle_envelope_best_effort(&runtime.session, &runtime.stats, true, envelope)
                .await
        );
    }

    minecraft.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: live_buy_window_slots("red_bed", "Buy Item"),
    }));

    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_pending_live_buys_once().await.unwrap();
    assert_eq!(
        minecraft.actions(),
        vec![MinecraftAction::OpenAuction(
            saf_core::AuctionId::new("auction-1").unwrap()
        )]
    );

    {
        let mut pending = runtime.pending_live_buys.lock().unwrap();
        let pending = pending.get_mut(&account).unwrap();
        pending.timed_bed_click_delay = Duration::from_millis(3);
        pending.click_at = Some(Instant::now() - Duration::from_millis(1));
    }
    runtime.process_pending_live_buys_once().await.unwrap();

    assert_eq!(
        runtime
            .pending_live_buy(&account)
            .unwrap()
            .unwrap()
            .timed_bed_clicks,
        1
    );
    for expected_clicks in 2..=5 {
        {
            let mut pending = runtime.pending_live_buys.lock().unwrap();
            let pending = pending.get_mut(&account).unwrap();
            pending.timed_bed_click_delay = Duration::from_millis(3);
            pending.last_click = Some(Instant::now() - Duration::from_millis(4));
        }
        runtime.process_pending_live_buys_once().await.unwrap();
        assert_eq!(
            runtime
                .pending_live_buy(&account)
                .unwrap()
                .unwrap()
                .timed_bed_clicks,
            expected_clicks
        );
    }
    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::ClickSlot(31)
        ]
    );
    assert!(runtime.pending_live_buy(&account).unwrap().is_some());
    runtime
        .pending_live_buys
        .lock()
        .unwrap()
        .get_mut(&account)
        .unwrap()
        .timed_bed_cleanup_at = Some(Instant::now() - Duration::from_millis(1));
    runtime.process_pending_live_buys_once().await.unwrap();
    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::CloseWindow
        ]
    );
    assert!(runtime.pending_live_buy(&account).unwrap().is_none());
    runtime.shutdown().await;
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn live_cofl_bed_spam_clicks_future_beds_immediately_until_timeout() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        session: "session".to_string(),
        waittime: 15,
        bed_spam: true,
        click_delay: 75,
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    mark_runtime_cofl_settings_loaded(&runtime);
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .managed_minecraft
        .get(&account)
        .unwrap()
        .replace_minecraft_for_test(minecraft.clone())
        .await;
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "Hyperion",
            "startingBid": 30_000_000,
            "target": 50_000_000,
            "finder": "SNIPER_MEDIAN",
            "purchaseAt": now_ms() + 60_000
        }),
    };

    {
        let stream = runtime.cofl_streams.first().unwrap();
        assert!(
            stream
                .handle_envelope_best_effort(&runtime.session, &runtime.stats, true, envelope)
                .await
        );
    }

    minecraft.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: live_buy_window_slots("red_bed", "Buy Item"),
    }));

    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_pending_live_buys_once().await.unwrap();
    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::ClickSlot(31)
        ]
    );
    assert!(runtime.pending_live_buy(&account).unwrap().is_some());

    runtime.process_pending_live_buys_once().await.unwrap();
    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::ClickSlot(31)
        ]
    );

    runtime
        .pending_live_buys
        .lock()
        .unwrap()
        .get_mut(&account)
        .unwrap()
        .last_click = Some(Instant::now() - Duration::from_millis(100));
    runtime.process_pending_live_buys_once().await.unwrap();
    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::ClickSlot(31)
        ]
    );

    {
        let mut pending = runtime.pending_live_buys.lock().unwrap();
        let pending = pending.get_mut(&account).unwrap();
        pending.last_click = Some(Instant::now() - Duration::from_millis(100));
        pending.bed_spam_until = Some(Instant::now() - Duration::from_millis(1));
    }
    runtime.process_pending_live_buys_once().await.unwrap();
    assert_eq!(
        minecraft.actions(),
        vec![
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("auction-1").unwrap()),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::CloseWindow
        ]
    );
    assert!(runtime.pending_live_buy(&account).unwrap().is_none());
    runtime.shutdown().await;
}

#[cfg(feature = "live-cofl")]
#[test]
fn all_flip_notification_marks_future_purchase_as_bed() {
    let account = AccountId::new("Main").unwrap();
    let nugget = FlipEvent::from_payload(&json!({
        "id": "auction-1",
        "itemName": "Hyperion",
        "startingBid": 30_000_000,
        "target": 50_000_000,
        "finder": "SNIPER_MEDIAN"
    }));
    assert!(
        all_flip_notification(&account, &nugget)
            .body
            .contains("[NUGGET]")
    );

    let bed = FlipEvent::from_payload(&json!({
        "id": "auction-2",
        "itemName": "Hyperion",
        "startingBid": 30_000_000,
        "target": 50_000_000,
        "finder": "SNIPER_MEDIAN",
        "purchaseAt": now_ms() + 60_000
    }));
    assert!(all_flip_notification(&account, &bed).body.contains("[BED]"));
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_flip_sends_all_flips_notification_best_effort() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let session = RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    let stats = LiveStatsProvider::new(vec![account.clone()]);
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let notifier = Arc::new(RecordingNotifier::default());
    let stream = LiveCoflStream {
        account: account.clone(),
        client: Arc::new(LiveCoflClient::new(
            account.clone(),
            link.clone(),
            "session".to_string(),
            link,
        )),
        tracked_flips: Arc::new(LiveTrackedFlipProvider::default()),
        market_actions: MarketActionMode::DryRun,
        allow_execute_chat: false,
        all_flip_notifier: Some(notifier.clone()),
        bed_click_offset: Duration::from_millis(15),
        bed_spam: false,
        bed_click_delay: Duration::from_millis(125),
        pending_live_buys: Arc::new(Mutex::new(BTreeMap::new())),
        humanizer: Arc::new(saf_core::Humanizer::seeded(
            saf_core::HumanizerConfig::default(),
            0,
        )),
        notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
        pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
    };
    let envelope = saf_cofl::CoflEnvelope {
        kind: "flip".to_string(),
        data: json!({
            "id": "auction-1",
            "itemName": "§6Hyperion",
            "startingBid": 30_000_000,
            "target": 50_000_000,
            "finder": "CraftCost",
            "volume": 24
        }),
    };

    assert!(
        stream
            .handle_envelope_best_effort(&session, &stats, true, envelope)
            .await
    );

    let notifications = notifier.notifications.lock().unwrap();
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].title, "Flip Found");
    assert_eq!(notifications[0].account.as_ref(), Some(&account));
    assert!(notifications[0].body.contains("Craft Cost"));
    assert!(notifications[0].body.contains("`Hyperion`"));
    assert!(notifications[0].body.contains("`30.0M`"));
    assert!(notifications[0].body.contains("`50.0M`"));
    assert!(notifications[0].body.contains("`18.5M` profit"));
    assert!(
        notifications[0]
            .body
            .contains("https://sky.coflnet.com/a/auction-1")
    );
    assert!(notifications[0].body.contains("24"));
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_execute_chat_is_blocked_without_explicit_opt_in() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    session.add_minecraft_client(account.clone(), minecraft.clone());
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let stream = LiveCoflStream {
        account: account.clone(),
        client: Arc::new(LiveCoflClient::new(
            account.clone(),
            link.clone(),
            "session".to_string(),
            link,
        )),
        tracked_flips: Arc::new(LiveTrackedFlipProvider::default()),
        market_actions: MarketActionMode::Live,
        allow_execute_chat: false,
        all_flip_notifier: None,
        bed_click_offset: Duration::from_millis(15),
        bed_spam: false,
        bed_click_delay: Duration::from_millis(125),
        pending_live_buys: Arc::new(Mutex::new(BTreeMap::new())),
        humanizer: Arc::new(saf_core::Humanizer::seeded(
            saf_core::HumanizerConfig::default(),
            0,
        )),
        notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
        pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
    };

    stream
        .handle_execute_instruction(
            &session,
            CoflExecuteInstruction::BlockedChat {
                command: "/lobby".to_string(),
            },
        )
        .await
        .unwrap();

    assert!(minecraft.actions().is_empty());
}

#[cfg(feature = "live-cofl")]
#[test]
fn cofl_tip_execute_blocks_are_expected_noise() {
    assert!(is_expected_blocked_chat_command("/tip Player skywars"));
    assert!(is_expected_blocked_chat_command("  /TIP Player arcade"));
    assert!(!is_expected_blocked_chat_command("/lobby"));
    assert!(!is_expected_blocked_chat_command("/tipall"));
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_execute_chat_opt_in_forwards_to_minecraft_chat() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let minecraft = Arc::new(RecordedMinecraftClient::new(account.clone()));
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    session.add_minecraft_client(account.clone(), minecraft.clone());
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let stream = LiveCoflStream {
        account: account.clone(),
        client: Arc::new(LiveCoflClient::new(
            account.clone(),
            link.clone(),
            "session".to_string(),
            link,
        )),
        tracked_flips: Arc::new(LiveTrackedFlipProvider::default()),
        market_actions: MarketActionMode::Live,
        allow_execute_chat: true,
        all_flip_notifier: None,
        bed_click_offset: Duration::from_millis(15),
        bed_spam: false,
        bed_click_delay: Duration::from_millis(125),
        pending_live_buys: Arc::new(Mutex::new(BTreeMap::new())),
        humanizer: Arc::new(saf_core::Humanizer::seeded(
            saf_core::HumanizerConfig::default(),
            0,
        )),
        notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
        pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
    };

    stream
        .handle_execute_instruction(
            &session,
            CoflExecuteInstruction::BlockedChat {
                command: "/lobby".to_string(),
            },
        )
        .await
        .unwrap();

    assert_eq!(
        minecraft.actions(),
        vec![MinecraftAction::Chat("/lobby".to_string())]
    );
}

#[cfg(feature = "live-cofl")]
#[test]
fn account_cofl_socket_links_can_supply_session() {
    let account = AccountId::new("Main").unwrap();
    let link = normalize_account_cofl_socket_link(
        "wss://sky-us.coflnet.com/modsocket?SId=fake&region=us",
        &account,
        "",
    )
    .unwrap();

    assert!(link.contains("player=Main"));
    assert!(link.contains("SId=fake"));
    assert_eq!(cofl_session_for_link("", &link), "fake");
    assert_eq!(
        cofl_session_for_link(" redacted-global-session ", &link),
        "fake"
    );
    assert_eq!(
        cofl_session_for_link(
            " redacted-global-session ",
            "wss://sky-us.coflnet.com/modsocket"
        ),
        "redacted-global-session"
    );
    assert_eq!(
        account_socket_session_source(
            "wss://sky-us.coflnet.com/modsocket?SId=fake&region=us",
            "redacted-global-session",
        ),
        ""
    );
    assert_eq!(
        account_socket_session_source(
            "wss://sky-us.coflnet.com/modsocket?region=us",
            "redacted-global-session",
        ),
        "redacted-global-session"
    );

    assert!(
        normalize_account_cofl_socket_link(
            "wss://sky-us.coflnet.com/modsocket?region=us",
            &account,
            "",
        )
        .is_none()
    );

    let config = SafConfig {
        session: " redacted-global-session ".to_string(),
        ..SafConfig::default()
    };
    assert_eq!(
        cofl_session_with_env(&config, |_| None),
        "redacted-global-session"
    );
    assert_eq!(
        cofl_session_with_env(&config, |name| {
            (name == "SAF_COFL_SESSION").then(|| " redacted-env-session ".to_string())
        }),
        "redacted-env-session"
    );
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_report_distinguishes_registered_and_connected_sockets() {
    let temp = tempfile::tempdir().unwrap();
    let mut config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        session: "test-session".to_string(),
        ..SafConfig::default()
    };
    config.relist = false;
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.once = true;

    let runtime = LiveRuntime::start(config, options).await.unwrap();
    let report = runtime.report();

    assert_eq!(report.cofl_connections, 1);
    assert_eq!(report.cofl_connected, 0);
}

#[test]
fn env_truthy_accepts_preflight_enabled_values() {
    assert!(env_truthy("1"));
    assert!(env_truthy("true"));
    assert!(env_truthy("TRUE"));
    assert!(env_truthy(" true "));
    assert!(!env_truthy("0"));
    assert!(!env_truthy("yes"));
}

#[cfg(feature = "live-cofl")]
#[test]
fn cofl_reconnect_backoff_doubles_and_resets() {
    let now = Instant::now();
    let mut backoff = CoflReconnectBackoff::new(Duration::from_secs(1), Duration::from_secs(4));

    assert!(backoff.should_attempt(now));
    backoff.record_failure(now);
    assert!(!backoff.should_attempt(now + Duration::from_millis(999)));
    assert!(backoff.should_attempt(now + Duration::from_secs(1)));

    backoff.record_failure(now + Duration::from_secs(1));
    assert!(!backoff.should_attempt(now + Duration::from_secs(2)));
    assert!(backoff.should_attempt(now + Duration::from_secs(3)));

    backoff.record_failure(now + Duration::from_secs(3));
    assert!(!backoff.should_attempt(now + Duration::from_secs(6)));
    assert!(backoff.should_attempt(now + Duration::from_secs(7)));

    backoff.record_success();
    assert!(backoff.should_attempt(now + Duration::from_secs(7)));
    backoff.record_failure(now + Duration::from_secs(7));
    assert!(backoff.should_attempt(now + Duration::from_secs(8)));
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_socket_switch_replaces_link_and_defers_reconnect() {
    let account = AccountId::new("Main").unwrap();
    let default_link =
        "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let client = LiveCoflClient::new(
        account,
        default_link.clone(),
        "session".to_string(),
        default_link,
    );
    let link =
        "wss://sky-us.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();

    client.switch_link(link.clone()).await;

    assert_eq!(client.current_link().await, link);
    assert!(!client.ensure_connected(false).await.unwrap());
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_socket_switch_to_same_link_still_defers_reconnect() {
    let account = AccountId::new("Main").unwrap();
    let link = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake".to_string();
    let client = LiveCoflClient::new(account, link.clone(), "session".to_string(), link);

    client.switch_link(client.current_link().await).await;

    let state = client.state.lock().await;
    assert!(
        state.backoff.next_attempt.is_some(),
        "Cofl connect commands should force a reconnect even when the normalized link is unchanged"
    );
}

#[cfg(feature = "live-cofl")]
#[test]
fn cofl_silent_watchdog_retries_region_then_backs_off_to_default() {
    let now = Instant::now();
    let region = "wss://sky-us.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake";
    let default = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake";
    let mut watchdog = CoflSilentOpenWatchdog::default();

    assert_eq!(
        watchdog.retry_link_after_silent_open(region, default, now, Duration::from_secs(300)),
        region
    );
    assert!(!watchdog.is_region_backed_off(region, now));

    assert_eq!(
        watchdog.retry_link_after_silent_open(region, default, now, Duration::from_secs(300)),
        default
    );
    assert!(watchdog.is_region_backed_off(region, now + Duration::from_secs(299)));
    assert!(!watchdog.is_region_backed_off(region, now + Duration::from_secs(301)));
    assert!(!watchdog.is_region_backed_off(default, now + Duration::from_secs(299)));
}

#[cfg(feature = "live-cofl")]
#[test]
fn cofl_silent_watchdog_clears_count_after_message() {
    let now = Instant::now();
    let region = "wss://sky-us.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake";
    let default = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake";
    let mut watchdog = CoflSilentOpenWatchdog::default();

    assert_eq!(
        watchdog.retry_link_after_silent_open(region, default, now, Duration::from_secs(300)),
        region
    );
    watchdog.clear_count(region);
    assert_eq!(
        watchdog.retry_link_after_silent_open(region, default, now, Duration::from_secs(300)),
        region
    );
    assert!(!watchdog.is_region_backed_off(region, now));
}

#[cfg(feature = "live-cofl")]
#[test]
fn cofl_silent_watchdog_backoff_is_scoped_by_host_port() {
    let now = Instant::now();
    let default = "wss://sky.coflnet.com/modsocket?version=1.5.1-af&player=Main&SId=fake";
    let port_443 = "wss://sky-us.coflnet.com:443/modsocket?version=1.5.1-af&player=Main&SId=fake";
    let port_444 = "wss://sky-us.coflnet.com:444/modsocket?version=1.5.1-af&player=Main&SId=fake";
    let mut watchdog = CoflSilentOpenWatchdog::default();

    assert_eq!(
        cofl_socket_host(port_443).as_deref(),
        Some("sky-us.coflnet.com")
    );
    assert_eq!(
        cofl_socket_host(port_444).as_deref(),
        Some("sky-us.coflnet.com:444")
    );
    assert_eq!(
        cofl_socket_host("wss://[::1]:9000/modsocket").as_deref(),
        Some("[::1]:9000")
    );

    watchdog.retry_link_after_silent_open(port_444, default, now, Duration::from_secs(300));
    watchdog.retry_link_after_silent_open(port_444, default, now, Duration::from_secs(300));

    assert!(watchdog.is_region_backed_off(port_444, now + Duration::from_secs(299)));
    assert!(!watchdog.is_region_backed_off(port_443, now + Duration::from_secs(299)));
}
