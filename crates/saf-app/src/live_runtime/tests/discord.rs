use super::*;

/// Serialize a reply's embed cards so tests can assert on the rendered title,
/// colour, description, and fields without depending on serenity internals.
#[cfg(feature = "live-discord")]
fn embed_json(reply: &DiscordInteractionReply) -> String {
    serde_json::to_string(&reply.embeds).unwrap()
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_dashboard_returns_button_controls() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string(), "Alt".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    runtime
        .queue
        .add(
            &account,
            json!({ "reason": "status-test" }),
            BotState::Custom("statusProbe".to_string()),
            2,
        )
        .await
        .unwrap();
    runtime
        .stats
        .record_scoreboard(&account, &["Purse: 1.2M".to_string()])
        .unwrap();
    runtime
        .stats
        .record_cofl_telemetry(
            &account,
            &CoflTelemetryUpdate {
                connection_id: Some("conn-main".to_string()),
                cofl_ping_ms: None,
                cofl_delay_ms: None,
                cofl_tier: None,
                cofl_expires_at: None,
            },
        )
        .unwrap();

    let reply = execute_discord_plan(
        &runtime.session,
        DiscordCommandPlan::ControllerAction {
            action: DiscordControllerAction::Dashboard,
        },
        None,
        None,
    )
    .await;
    let components = serde_json::to_string(&reply.components).unwrap();

    assert!(reply.content.contains("Dashboard"));
    assert!(reply.content.contains("Running: Main, Alt"));
    assert!(
        reply
            .content
            .contains("`Main` queue 1 auctions unknown purse 1.20m Cofl conn-main")
    );
    // The card is rendered as a blurple embed mirroring the dashboard prose.
    let embed = embed_json(&reply);
    assert_eq!(reply.embeds.len(), 1);
    assert!(embed.contains("Dashboard"));
    assert!(embed.contains("Running: Main, Alt"));
    assert!(embed.contains(&saf_discord::COLOR_BLURPLE.to_string()));
    assert!(components.contains("saf:dashboard"));
    assert!(components.contains("saf:start"));
    assert!(components.contains("saf:stop:all"));
    assert!(components.contains("saf:queue:Main"));
    assert!(components.contains("saf:inventory:Alt"));
    assert_eq!(reply.components.len(), 4);

    let status = execute_discord_plan(
        &runtime.session,
        DiscordCommandPlan::ControllerAction {
            action: DiscordControllerAction::Status,
        },
        None,
        None,
    )
    .await;
    assert!(status.content.contains("Status"));
    assert!(status.content.contains("`Main` queue 1"));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_help_lists_registered_commands() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    let reply = execute_discord_plan(
        &runtime.session,
        DiscordCommandPlan::ControllerAction {
            action: DiscordControllerAction::Help,
        },
        None,
        None,
    )
    .await;
    let components = serde_json::to_string(&reply.components).unwrap();

    assert!(reply.content.contains("SAF commands"));
    assert!(reply.content.contains("/dashboard"));
    assert!(reply.content.contains("/claim_sold"));
    assert!(reply.content.contains("/test_webhook"));
    assert!(components.contains("saf:dashboard"));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_account_panel_returns_account_controls() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string(), "Alt".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    let reply = execute_discord_plan(
        &runtime.session,
        DiscordCommandPlan::ControllerAction {
            action: DiscordControllerAction::AccountPanel {
                username: "Alt".to_string(),
            },
        },
        None,
        None,
    )
    .await;
    let components = serde_json::to_string(&reply.components).unwrap();

    assert!(reply.content.contains("Account `Alt`"));
    assert!(reply.content.contains("Queue: 0"));
    assert!(reply.content.contains("Auctions: unknown"));
    assert!(reply.content.contains("Cofl: pending"));
    // The account card carries structured fields plus the player-head thumbnail.
    let embed = embed_json(&reply);
    assert_eq!(reply.embeds.len(), 1);
    assert!(embed.contains("Account Alt"));
    assert!(embed.contains("Queue"));
    assert!(embed.contains("Cofl"));
    assert!(embed.contains("crafthead.net/cube/Alt"));
    assert!(components.contains("saf:stats:Alt"));
    assert!(components.contains("saf:queue:Alt"));
    assert!(components.contains("saf:sellInventory:Alt:0"));
    assert!(components.contains("saf:delistAll:Alt"));
    assert!(components.contains("saf:stop:Alt"));
    assert!(components.contains("saf:cofljson:Alt"));
    assert!(components.contains("saf:dashboard"));
    assert!(components.contains("saf:logs"));
    assert!(components.contains("saf:messages"));
    assert!(components.contains("saf:blacklist"));
    assert_eq!(reply.components.len(), 3);
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn discord_gateway_config_enforces_allowed_user_ids() {
    let config = SafConfig {
        discord_id: " 111 ".to_string(),
        allowed_i_ds: vec![
            " 222 ".to_string(),
            "not-a-discord-id".to_string(),
            String::new(),
        ],
        discord_bot: saf_core::config::DiscordBotConfig {
            enabled: true,
            token: " test-token ".to_string(),
            guild_id: " 123456789 ".to_string(),
            allowed_i_ds: vec![" 333 ".to_string(), "000444".to_string()],
            ..Default::default()
        },
        ..SafConfig::default()
    };

    let gateway = DiscordGatewayConfig::from_config(&config).unwrap();

    assert_eq!(gateway.token, "test-token");
    assert_eq!(gateway.guild_id, Some(123456789));
    assert!(gateway.ephemeral);
    assert!(gateway.is_allowed(111));
    assert!(gateway.is_allowed(222));
    assert!(gateway.is_allowed(333));
    assert!(gateway.is_allowed(444));
    assert_eq!(gateway.allowed_ids.len(), 4);
    assert!(!gateway.allowed_ids.contains("not-a-discord-id"));
    assert!(!gateway.allowed_ids.contains("000444"));
    assert!(!gateway.is_allowed(555));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn discord_gateway_config_denies_all_when_acl_is_empty() {
    let config = SafConfig {
        discord_bot: saf_core::config::DiscordBotConfig {
            enabled: true,
            token: "test-token".to_string(),
            ..Default::default()
        },
        ..SafConfig::default()
    };

    let gateway = DiscordGatewayConfig::from_config(&config).unwrap();

    assert!(gateway.ephemeral);
    assert!(gateway.allowed_ids.is_empty());
    assert!(!gateway.is_allowed(444));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn discord_gateway_config_preserves_explicit_public_responses() {
    let config = SafConfig {
        discord_bot: saf_core::config::DiscordBotConfig {
            enabled: true,
            token: "test-token".to_string(),
            ephemeral: false,
            ..Default::default()
        },
        ..SafConfig::default()
    };

    let gateway = DiscordGatewayConfig::from_config(&config).unwrap();

    assert!(!gateway.ephemeral);
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_gateway_exit_schedules_backoff_restart() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    runtime.discord_task = Some(tokio::spawn(async {}));
    runtime.discord_started = true;
    runtime.discord_restart_delay = Duration::from_secs(60);
    tokio::task::yield_now().await;

    runtime.poll_discord_gateway_once().await.unwrap();

    assert!(runtime.discord_task.is_none());
    assert!(!runtime.discord_started);
    assert!(!runtime.report().discord_started);
    assert!(
        runtime
            .discord_restart_at
            .is_some_and(|at| at > Instant::now())
    );
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_gateway_restart_skips_cleanly_when_config_is_disabled() {
    let temp = tempfile::tempdir().unwrap();
    let mut runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    runtime.discord_restart_at = Some(Instant::now() - Duration::from_millis(1));

    runtime.poll_discord_gateway_once().await.unwrap();

    assert!(runtime.discord_task.is_none());
    assert!(!runtime.discord_started);
    assert!(runtime.discord_restart_at.is_none());
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_clear_data_uses_confirmation_buttons() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    let reply = execute_discord_plan(
        &runtime.session,
        DiscordCommandPlan::ControllerAction {
            action: DiscordControllerAction::StatusForConfirmation {
                username: "Main".to_string(),
                action: "clear_data".to_string(),
            },
        },
        None,
        None,
    )
    .await;
    let components = serde_json::to_string(&reply.components).unwrap();

    assert!(reply.content.contains("Confirm clearing saved queue"));
    assert!(components.contains("saf:confirmClearData:Main"));
    assert!(components.contains("saf:cancel"));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_clear_data_confirmation_button_clears_saved_queue() {
    let temp = tempfile::tempdir().unwrap();
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let account = AccountId::new("Main").unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        options,
    )
    .await
    .unwrap();
    runtime
        .queue
        .add(
            &account,
            json!({ "reason": "stale-recovery" }),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
        )
        .await
        .unwrap();

    let prompt = execute_discord_plan(
        &runtime.session,
        DiscordCommandPlan::ControllerAction {
            action: DiscordControllerAction::StatusForConfirmation {
                username: "Main".to_string(),
                action: "clear_data".to_string(),
            },
        },
        None,
        None,
    )
    .await;

    assert!(prompt.content.contains("Confirm clearing saved queue"));
    assert_eq!(runtime.queue.snapshot(&account).await.unwrap().len(), 1);
    assert!(saf_discord::plan_button("saf:cancel").unwrap().is_none());
    assert_eq!(runtime.queue.snapshot(&account).await.unwrap().len(), 1);

    let confirm = saf_discord::plan_button("saf:confirmClearData:Main")
        .unwrap()
        .unwrap();
    let reply = execute_discord_plan(&runtime.session, confirm, None, None).await;

    assert!(reply.content.contains("Cleared saved data for `Main`"));
    assert!(reply.content.contains("Queue removed: 1"));
    assert!(runtime.queue.snapshot(&account).await.unwrap().is_empty());
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_sell_inventory_confirmation_button_queues_after_preview() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let file_store = FileQueueStore::new(temp.path());
    let queue = Arc::new(MarketActionQueueStore::new(
        file_store.clone(),
        MarketActionMode::Live,
    ));
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    session.set_fallback_queue_store(queue.clone());
    session.set_fallback_saved_data_store(Arc::new(file_store));
    session.add_inventory_provider(account.clone(), Arc::new(ListingPreviewInventoryProvider));

    let prompt = execute_discord_plan(
        &session,
        DiscordCommandPlan::ControllerAction {
            action: DiscordControllerAction::SellInventory {
                username: Some("Main".to_string()),
                include_hotbar: false,
            },
        },
        None,
        None,
    )
    .await;

    assert!(
        prompt
            .content
            .contains("Preview: 1 listable inventory item(s).")
    );
    assert!(queue.snapshot(&account).await.unwrap().is_empty());

    let confirm = saf_discord::plan_button("saf:confirmSellInventory:Main:0")
        .unwrap()
        .unwrap();
    let reply = execute_discord_plan(&session, confirm, None, None).await;
    let queued = queue.snapshot(&account).await.unwrap();

    assert!(reply.content.contains("Queued 1 inventory listing(s)"));
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].state, BotState::ListingNoName);
    assert_eq!(queued[0].action["auctionID"], json!("main-priced"));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_delist_all_confirmation_button_queues_after_prompt() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let file_store = FileQueueStore::new(temp.path());
    let queue = Arc::new(MarketActionQueueStore::new(
        file_store.clone(),
        MarketActionMode::Live,
    ));
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    session.set_fallback_queue_store(queue.clone());
    session.set_fallback_saved_data_store(Arc::new(file_store));
    session.add_active_auction_provider(account.clone(), Arc::new(FixedActiveAuctionProvider));

    let prompt = execute_discord_plan(
        &session,
        DiscordCommandPlan::ControllerAction {
            action: DiscordControllerAction::DelistEverything {
                username: Some("Main".to_string()),
            },
        },
        None,
        None,
    )
    .await;

    assert!(prompt.content.contains("Confirm scanning active auctions"));
    assert!(queue.snapshot(&account).await.unwrap().is_empty());

    let confirm = saf_discord::plan_button("saf:confirmDelistAll:Main")
        .unwrap()
        .unwrap();
    let reply = execute_discord_plan(&session, confirm, None, None).await;
    let queued = queue.snapshot(&account).await.unwrap();

    assert!(reply.content.contains("Queued 1 delist action(s)"));
    assert_eq!(queued.len(), 1);
    assert_eq!(queued[0].state, BotState::Delisting);
    assert_eq!(queued[0].action["auctionID"], json!("auction-1"));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_renders_users_without_raw_json() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    let reply = execute_discord_terminal(&runtime.session, "users").await;

    assert!(reply.content.contains("Users"));
    assert!(reply.content.contains("Configured: Main"));
    assert!(!reply.content.contains("\"type\""));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_logs_attach_latest_log() {
    let temp = tempfile::tempdir().unwrap();
    let log_path = temp.path().join("latest.log");
    tokio::fs::write(&log_path, "older\nnewest\n")
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut session = RuntimeSession::new(BotRuntime::from_config(&config, vec![]));
    session.set_log_reader(Arc::new(FileLogReader::new(&log_path)));

    let reply = execute_discord_terminal(&session, "logs").await;

    assert!(reply.content.contains("Logs"));
    assert_eq!(reply.files.len(), 1);
    assert_eq!(reply.files[0].filename, "latest.log");
    let components = serde_json::to_string(&reply.components).unwrap();
    assert!(components.contains("saf:logs"));
    assert!(components.contains("saf:messages"));
    assert!(components.contains("saf:dashboard"));
    assert_eq!(
        String::from_utf8(reply.files[0].data.clone()).unwrap(),
        "older\nnewest\n"
    );
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_logs_use_runtime_state_base_dir() {
    let temp = tempfile::tempdir().unwrap();
    let logs_dir = temp.path().join("logs");
    tokio::fs::create_dir_all(&logs_dir).await.unwrap();
    tokio::fs::write(logs_dir.join("latest.log"), "runtime-base-log\n")
        .await
        .unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    let reply = execute_discord_terminal(&runtime.session, "logs").await;

    assert_eq!(reply.files.len(), 1);
    assert_eq!(
        String::from_utf8(reply.files[0].data.clone()).unwrap(),
        "runtime-base-log\n"
    );
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_inventory_attaches_full_snapshot_json() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    session.add_inventory_provider(account, Arc::new(FixedInventoryProvider));

    let reply = execute_discord_terminal(&session, "inventory").await;

    assert!(reply.content.contains("Inventory `Main`"));
    assert_eq!(reply.files.len(), 1);
    assert_eq!(reply.files[0].filename, "inventory-Main.json");
    let components = serde_json::to_string(&reply.components).unwrap();
    assert!(components.contains("saf:sellInventory:Main:0"));
    assert!(components.contains("saf:sellInventory:Main:1"));
    assert!(components.contains("saf:account:Main"));
    assert!(components.contains("saf:queue:Main"));
    let attachment = String::from_utf8(reply.files[0].data.clone()).unwrap();
    assert!(attachment.contains("\"account\": \"Main\""));
    assert!(attachment.contains("\"itemName\": \"Aspect of the Dragons\""));
    assert!(attachment.contains("\"uuid\": \"priced-item\""));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_planned_inventory_names_missing_provider() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    let reply = execute_discord_terminal(&runtime.session, "inventory").await;

    assert!(reply.content.contains("No live inventory provider"));
    assert!(reply.content.contains("SAF_RUST_MINECRAFT=azalea"));
    assert!(!reply.content.contains("planned; no live provider"));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_dry_run_delist_all_names_cached_window_requirement() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    let reply = execute_discord_terminal(&runtime.session, "delist_everything").await;

    assert!(reply.content.contains("Delist all"));
    assert!(
        reply
            .content
            .contains("cached Manage Auctions window before Rust can queue delist actions")
    );
    assert!(reply.content.contains("Rust did not open `/ah`"));
    assert!(!reply.content.contains("Runtime error"));
    assert!(!reply.content.contains("port unavailable"));
}

#[cfg(feature = "live-discord")]
#[test]
fn live_discord_planned_directive_messages_are_specific() {
    let account = AccountId::new("Main").unwrap();
    let alt = AccountId::new("Alt").unwrap();
    let directives = vec![
        RuntimeDirective::StartAccounts {
            accounts: vec![account.clone()],
        },
        RuntimeDirective::StopAccounts { account: None },
        RuntimeDirective::TransferCoins {
            from: account.clone(),
            to: alt,
            amount: "all".to_string(),
            stop_source: true,
        },
        RuntimeDirective::SendMinecraftChat {
            account: account.clone(),
            message: "/is".to_string(),
        },
        RuntimeDirective::SendCoflCommand {
            account: account.clone(),
            command: "/cofl ping".to_string(),
        },
        RuntimeDirective::ShowStats {
            account: account.clone(),
        },
        RuntimeDirective::ShowProfit {
            account: account.clone(),
        },
        RuntimeDirective::ShowPing {
            account: account.clone(),
        },
        RuntimeDirective::ShowUsers,
        RuntimeDirective::ShowGlobalStats,
        RuntimeDirective::ShowConnections,
        RuntimeDirective::ShowLogs { lines: 8 },
        RuntimeDirective::ShowQueue {
            account: account.clone(),
        },
        RuntimeDirective::ClearQueue {
            account: account.clone(),
        },
        RuntimeDirective::CancelQueueEntry {
            account: account.clone(),
            index: 1,
        },
        RuntimeDirective::ClearAllQueues,
        RuntimeDirective::ClearData {
            account: account.clone(),
        },
        RuntimeDirective::BlacklistCommand {
            account: account.clone(),
            message: "list".to_string(),
        },
        RuntimeDirective::CheckBids {
            account: account.clone(),
        },
        RuntimeDirective::Bank {
            account: account.clone(),
            request: saf_core::BankRequest {
                amount: Some("1m".to_string()),
                withdraw: false,
                personal: false,
            },
        },
        RuntimeDirective::QueueState {
            account: account.clone(),
            action: json!({"reason": "manual"}),
            state: BotState::Custom("reconcileAuctions".to_string()),
            priority: 2,
        },
        RuntimeDirective::ExternalBuy {
            account: account.clone(),
            auction_id: "auction-1".to_string(),
        },
        RuntimeDirective::TrackedListFlip {
            account: account.clone(),
            auction_id: "auction-1".to_string(),
            time_hours: 48.0,
        },
        RuntimeDirective::ShowInventory {
            account: account.clone(),
        },
        RuntimeDirective::SellInventory {
            account: account.clone(),
            include_hotbar: false,
        },
        RuntimeDirective::QueueDelistAll {
            account: account.clone(),
        },
        RuntimeDirective::DiagnoseSlots {
            account: account.clone(),
            target: None,
        },
        RuntimeDirective::TestWebhook {
            account: account.clone(),
        },
        RuntimeDirective::ScheduleAccount {
            account: account.clone(),
            action: ScheduledAccountAction::Start,
            delay_ms: 1,
        },
        RuntimeDirective::UnknownTerminalCommand {
            account,
            command: "wat".to_string(),
            message: String::new(),
        },
    ];

    for directive in directives {
        let message = format_planned_directive(&directive);
        assert!(!message.trim().is_empty(), "{directive:?}");
        assert!(
            !message.contains("Command planned"),
            "{directive:?}: {message}"
        );
        assert!(
            !message.contains("no live provider handled"),
            "{directive:?}: {message}"
        );
    }
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_queue_snapshot_returns_navigation_controls() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    let reply = execute_discord_terminal(&runtime.session, "queue").await;
    let components = serde_json::to_string(&reply.components).unwrap();

    assert!(reply.content.contains("Queue `Main`"));
    assert!(components.contains("saf:queue:Main"));
    assert!(components.contains("saf:reconcile:Main"));
    assert!(components.contains("saf:inventory:Main"));
    assert!(components.contains("saf:clearData:Main"));
    assert!(components.contains("saf:dashboard"));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_clear_queue_all_matches_node_default() {
    let temp = tempfile::tempdir().unwrap();
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string(), "Alt".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        options,
    )
    .await
    .unwrap();
    execute_discord_terminal(&runtime.session, "Main reconcile").await;
    execute_discord_terminal(&runtime.session, "Alt reconcile").await;

    let reply = execute_discord_terminal(&runtime.session, "clear_queue_all").await;
    let components = serde_json::to_string(&reply.components).unwrap();
    let main = execute_discord_terminal(&runtime.session, "Main queue").await;
    let alt = execute_discord_terminal(&runtime.session, "Alt queue").await;

    assert!(reply.content.contains("Cleared 2 queued action(s)."));
    assert!(reply.content.contains("`Main`: 1"));
    assert!(reply.content.contains("`Alt`: 1"));
    assert!(components.contains("saf:dashboard"));
    assert!(main.content.contains("No queued actions."));
    assert!(alt.content.contains("No queued actions."));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_account_outcomes_return_account_controls() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    let queued = execute_discord_terminal(&runtime.session, "reconcile").await;
    let queued_components = serde_json::to_string(&queued.components).unwrap();
    assert!(queued.content.contains("Queued action."));
    assert!(queued_components.contains("saf:stats:Main"));
    assert!(queued_components.contains("saf:bids:Main"));
    assert!(queued_components.contains("saf:sellInventory:Main:0"));
    assert!(queued_components.contains("saf:delistAll:Main"));
    assert!(queued_components.contains("saf:cofljson:Main"));

    let stats = execute_discord_terminal(&runtime.session, "stats").await;
    let stats_components = serde_json::to_string(&stats.components).unwrap();
    assert!(stats.content.contains("Stats `Main`"));
    assert!(stats.content.contains("Auctions: unknown"));
    assert!(stats_components.contains("saf:queue:Main"));
    assert!(stats_components.contains("saf:dashboard"));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_blacklist_list_renders_without_running_account() {
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("config.json5");
    tokio::fs::write(
        &config_path,
        r#"{
            igns: ["Main"],
            defaultIgn: "Main",
            doNotBuy: {
                tags: ["SPEED_RELIC"],
                names: ["Bad `Name`"],
            },
            doNotRelist: {
                itemEnchantments: [
                    { tag: "LAVA_SHELL_NECKLACE", enchantment: "THE_ONE", level: 5 },
                ],
            },
        }"#,
    )
    .await
    .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut session = RuntimeSession::new(BotRuntime::from_config(&config, Vec::new()));
    session.set_fallback_blacklist_store(Arc::new(FileBlacklistStore::new(
        &config_path,
        session.blacklist_handle(),
    )));

    let reply = execute_discord_terminal(&session, "blacklist list").await;

    assert!(reply.content.contains("Live Blacklist Rules"));
    assert!(reply.content.contains("SPEED_RELIC"));
    assert!(reply.content.contains("Bad 'Name'"));
    assert!(reply.content.contains("LAVA_SHELL_NECKLACE + THE_ONE 5"));
    assert!(!reply.content.contains("\"doNotBuy\""));
    let components = serde_json::to_string(&reply.components).unwrap();
    assert!(components.contains("saf:blacklist"));
    assert!(components.contains("saf:dashboard"));
    assert!(components.contains("saf:messages"));
    assert!(components.contains("saf:logs"));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_users_follow_lifecycle_commands() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string(), "Alt".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    execute_discord_terminal(&runtime.session, "stop Alt").await;
    let after_stop = execute_discord_terminal(&runtime.session, "users").await;
    execute_discord_terminal(&runtime.session, "stop").await;
    let after_stop_all = execute_discord_terminal(&runtime.session, "users").await;
    execute_discord_terminal(&runtime.session, "start Alt").await;
    let after_start = execute_discord_terminal(&runtime.session, "users").await;

    assert!(after_stop.content.contains("Running: Main"));
    assert!(after_stop_all.content.contains("Running: none"));
    assert!(after_start.content.contains("Running: Alt"));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_start_controls_start_default_account_only() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string(), "Alt".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    execute_discord_terminal(&runtime.session, "stop").await;
    let button_plan = saf_discord::plan_button("saf:start").unwrap().unwrap();
    execute_discord_plan(&runtime.session, button_plan, None, None).await;
    let after_button = execute_discord_terminal(&runtime.session, "users").await;

    execute_discord_terminal(&runtime.session, "stop").await;
    let slash_plan =
        saf_discord::plan_invocation(&saf_discord::CommandInvocation::new("start_bot")).unwrap();
    execute_discord_plan(&runtime.session, slash_plan, None, None).await;
    let after_slash = execute_discord_terminal(&runtime.session, "users").await;

    for reply in [after_button, after_slash] {
        assert!(reply.content.contains("Running: Main"));
        assert!(!reply.content.contains("Running: Main, Alt"));
    }
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_renders_gui_diagnostics_without_planned_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    runtime
        .remember_window_snapshot(
            AccountId::new("Main").unwrap(),
            WindowSnapshot {
                title: "Manage Auctions".to_string(),
                slots: vec![saf_core::gui::WindowSlot {
                    slot: 10,
                    name: "diamond_sword".to_string(),
                    display_name: "Sharp Sword".to_string(),
                    lore: vec!["Seller: Main".to_string()],
                    item_uuid: Some("item-uuid-1".to_string()),
                }],
            },
        )
        .unwrap();

    let reply = execute_discord_terminal(&runtime.session, "diagslots current").await;

    assert!(reply.content.contains("GUI Slots `Main`"));
    assert!(reply.content.contains("Window: Manage Auctions"));
    assert!(reply.content.contains("Sharp Sword"));
    assert!(!reply.content.contains("planned"));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_reports_unknown_terminal_commands_as_errors() {
    let temp = tempfile::tempdir().unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    let reply = execute_discord_terminal(&runtime.session, "wat even").await;

    assert!(
        reply
            .content
            .contains("Runtime error: Unknown terminal command: wat even")
    );
    assert!(!reply.content.contains("planned"));
}

#[cfg(feature = "live-discord")]
#[test]
fn live_discord_inventory_listing_preview_matches_node_limit() {
    let account = AccountId::new("Main").unwrap();
    let snapshot = InventorySnapshot {
        account,
        items: (0..13)
            .map(|index| InventoryItem {
                uuid: Some(format!("uuid-{index}")),
                item_name: format!("Listable Item {index}"),
                lore: Vec::new(),
                price: Some(1_000_000.0 + f64::from(index)),
                tag: Some("LISTABLE".to_string()),
                slot: Some(index as u8),
                in_hotbar: false,
            })
            .collect(),
    };

    let preview = format_inventory_listing_preview(&snapshot, false);

    assert!(preview.contains("Preview: 13 listable inventory item(s)."));
    assert!(preview.contains("Listable Item 11"));
    assert!(!preview.contains("Listable Item 12 `uuid-12`"));
    assert!(preview.contains("...and 1 more listable item(s)."));
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn live_discord_sell_inventory_confirmation_previews_listable_items() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    session.add_inventory_provider(account, Arc::new(ListingPreviewInventoryProvider));

    let reply = execute_discord_plan(
        &session,
        DiscordCommandPlan::ControllerAction {
            action: DiscordControllerAction::SellInventory {
                username: None,
                include_hotbar: false,
            },
        },
        None,
        None,
    )
    .await;
    let components = serde_json::to_string(&reply.components).unwrap();

    assert!(reply.content.contains("Hotbar items are excluded"));
    assert!(
        reply
            .content
            .contains("Preview: 1 listable inventory item(s).")
    );
    assert!(reply.content.contains("Aspect of the Dragons"));
    assert!(!reply.content.contains("Wither Impact Wand"));
    assert!(components.contains("saf:confirmSellInventory:Main:0"));
    assert!(!components.contains("<ign>"));

    let reply = execute_discord_plan(
        &session,
        DiscordCommandPlan::ControllerAction {
            action: DiscordControllerAction::SellInventory {
                username: None,
                include_hotbar: true,
            },
        },
        None,
        None,
    )
    .await;
    let components = serde_json::to_string(&reply.components).unwrap();

    assert!(reply.content.contains("Hotbar items are included"));
    assert!(
        reply
            .content
            .contains("Preview: 2 listable inventory item(s).")
    );
    assert!(reply.content.contains("Wither Impact Wand"));
    assert!(!reply.content.contains("Cheap Stone"));
    assert!(!reply.content.contains("Missing UUID"));
    assert!(components.contains("saf:confirmSellInventory:Main:1"));
    assert!(!components.contains("<ign>"));
}
