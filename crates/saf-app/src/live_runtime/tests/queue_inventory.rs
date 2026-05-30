use super::*;
#[tokio::test]
async fn dry_run_queue_executor_records_without_mutating_saved_queue() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"auctionID": "auction-1"}),
            BotState::Buying,
            5,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.once = true;

    let report = run_live(config, options).await.unwrap();

    assert_eq!(report.processed_queue_steps, 1);
    assert_eq!(report.dry_run_market_actions, 1);
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );
}

#[tokio::test]
async fn dry_run_queue_executor_records_window_click_without_mutating_saved_queue() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"auctionID": "auction-1"}),
            BotState::Buying,
            5,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "BIN Auction View".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 33,
                name: "gold_nugget".to_string(),
                display_name: "Buy Item Right Now".to_string(),
                lore: vec!["Click to buy".to_string()],
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    let records = runtime.dry_run_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].action["instruction"]["type"], json!("clickSlot"));
    assert_eq!(records[0].action["instruction"]["slot"], json!(33));
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );
}

#[tokio::test]
async fn live_queue_blocks_implausibly_low_listing_prices() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "ab458ac7-f7ff-4056-a710-fa2d8089703c",
                "inv": "ab458ac7-f7ff-4056-a710-fa2d8089703c",
                "price": 101,
                "oldPrice": 90_500_000,
                "weirdItemName": "Ancient Skeleton Master Chestplate"
            }),
            BotState::Listing,
            4,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;

    runtime.process_market_queue_once().await.unwrap();

    assert!(client.actions().is_empty());
    assert_eq!(runtime.report().completed_queue_entries, 1);
    assert!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .is_empty()
    );
}

#[tokio::test]
async fn stopped_accounts_do_not_advance_market_queue() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"auctionID": "auction-1"}),
            BotState::Buying,
            5,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string(), "Alt".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.once = true;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();

    runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "stop Main".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    runtime.process_market_queue_once().await.unwrap();

    assert!(runtime.dry_run_records().is_empty());

    runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "start Main".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(runtime.dry_run_records().len(), 1);
}

#[tokio::test]
async fn stopped_accounts_clear_cached_market_windows() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string(), "Alt".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.once = true;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "BIN Auction View".to_string(),
            slots: Vec::new(),
        },
    );

    runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "stop Main".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    runtime.poll_once().await.unwrap();

    assert!(
        !runtime
            .active_windows
            .lock()
            .unwrap()
            .contains_key(&account)
    );
}

#[tokio::test]
async fn stopped_accounts_reset_island_readiness_before_restart() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string(), "Alt".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.once = true;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    {
        let island = runtime.island_states.get_mut(&account).unwrap();
        island.require_ready_for_market = true;
        island.ready = true;
        island.startup_reconcile_queued = true;
        island.startup_profile_scan_complete = true;
        island.startup_ready_notified = true;
    }

    runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "stop Main".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    runtime.poll_once().await.unwrap();

    let island = runtime.island_states.get(&account).unwrap();
    assert!(!island.ready);
    assert!(!island.startup_reconcile_queued);
    assert!(!island.startup_profile_scan_complete);
    assert!(!island.startup_ready_notified);

    runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "start Main".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    assert!(!runtime.island_states.get(&account).unwrap().ready);
}

#[tokio::test]
async fn window_close_events_clear_cached_market_windows() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::WindowClosed);
    runtime.minecraft_clients.insert(account.clone(), client);
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "BIN Auction View".to_string(),
            slots: Vec::new(),
        },
    );
    runtime.pending_market_steps.insert(
        account.clone(),
        PendingMarketStep {
            entry: QueueEntry {
                state: BotState::Buying,
                priority: 5,
                action: json!({"auctionID": "auction-1"}),
            },
            instruction: MarketInstruction::ClickSlot { slot: 33 },
            last_attempt: Instant::now(),
            opens_sold_claim_action: false,
        },
    );

    runtime.poll_minecraft_once().await.unwrap();

    assert_eq!(runtime.processed_minecraft_events, 1);
    assert!(
        !runtime
            .active_windows
            .lock()
            .unwrap()
            .contains_key(&account)
    );
    assert!(!runtime.pending_market_steps.contains_key(&account));
}

#[tokio::test]
async fn same_title_window_refresh_keeps_market_step_debounce() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Auction House".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 15,
            name: "gold_ingot".to_string(),
            display_name: "Manage Auctions".to_string(),
            lore: Vec::new(),
            item_uuid: None,
        }],
    }));
    runtime.minecraft_clients.insert(account.clone(), client);
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Auction House".to_string(),
            slots: Vec::new(),
        },
    );
    runtime.pending_market_steps.insert(
        account.clone(),
        PendingMarketStep {
            entry: QueueEntry {
                state: BotState::Custom("reconcileAuctions".to_string()),
                priority: 2,
                action: json!({"reason": "startup"}),
            },
            instruction: MarketInstruction::ClickSlot { slot: 15 },
            last_attempt: Instant::now(),
            opens_sold_claim_action: false,
        },
    );

    runtime.poll_minecraft_once().await.unwrap();

    assert!(runtime.pending_market_steps.contains_key(&account));
}

#[tokio::test]
async fn new_window_title_releases_market_step_debounce() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: Vec::new(),
    }));
    runtime.minecraft_clients.insert(account.clone(), client);
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Auction House".to_string(),
            slots: Vec::new(),
        },
    );
    runtime.pending_market_steps.insert(
        account.clone(),
        PendingMarketStep {
            entry: QueueEntry {
                state: BotState::Custom("reconcileAuctions".to_string()),
                priority: 2,
                action: json!({"reason": "startup"}),
            },
            instruction: MarketInstruction::ClickSlot { slot: 15 },
            last_attempt: Instant::now(),
            opens_sold_claim_action: false,
        },
    );

    runtime.poll_minecraft_once().await.unwrap();

    assert!(!runtime.pending_market_steps.contains_key(&account));
}

#[tokio::test]
async fn live_market_clicks_wait_for_fresh_window_to_settle() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"reason": "startup"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: false,
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime
        .remember_window_snapshot(
            account.clone(),
            WindowSnapshot {
                title: "Auction House".to_string(),
                slots: vec![saf_core::gui::WindowSlot {
                    slot: 15,
                    name: "gold_ingot".to_string(),
                    display_name: "Manage Auctions".to_string(),
                    lore: Vec::new(),
                    item_uuid: None,
                }],
            },
        )
        .unwrap();

    runtime.process_market_queue_once().await.unwrap();

    assert!(client.actions().is_empty());
    runtime.active_window_observed_at.lock().unwrap().insert(
        account.clone(),
        Instant::now() - MARKET_WINDOW_SETTLE_DELAY - Duration::from_millis(1),
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(15)]);
}

#[tokio::test]
async fn stale_transition_window_reopens_auction_house_instead_of_reclicking() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"reason": "startup"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: false,
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime
        .remember_window_snapshot(
            account.clone(),
            WindowSnapshot {
                title: "Auction House".to_string(),
                slots: vec![saf_core::gui::WindowSlot {
                    slot: 15,
                    name: "gold_ingot".to_string(),
                    display_name: "Manage Auctions".to_string(),
                    lore: Vec::new(),
                    item_uuid: None,
                }],
            },
        )
        .unwrap();
    runtime.active_window_observed_at.lock().unwrap().insert(
        account.clone(),
        Instant::now() - MARKET_WINDOW_SETTLE_DELAY - Duration::from_millis(1),
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(15)]);
    let stale_attempt = Instant::now() - MARKET_STEP_RETRY_INTERVAL - Duration::from_millis(1);
    runtime
        .pending_market_steps
        .get_mut(&account)
        .unwrap()
        .last_attempt = stale_attempt;
    runtime
        .active_window_observed_at
        .lock()
        .unwrap()
        .insert(account.clone(), stale_attempt - Duration::from_millis(1));

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(15)]);
    assert!(
        !runtime
            .active_windows
            .lock()
            .unwrap()
            .contains_key(&account)
    );
    assert!(!runtime.pending_market_steps.contains_key(&account));

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::ClickSlot(15),
            MinecraftAction::Chat("/ah".to_string())
        ]
    );
}

#[tokio::test]
async fn unchanged_window_snapshot_keeps_original_observed_time() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let runtime = LiveRuntime::start(config, options).await.unwrap();
    let window = WindowSnapshot {
        title: "Auction House".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 15,
            name: "gold_ingot".to_string(),
            display_name: "Manage Auctions".to_string(),
            lore: Vec::new(),
            item_uuid: None,
        }],
    };

    runtime
        .remember_window_snapshot(account.clone(), window.clone())
        .unwrap();
    let original_observed_at = *runtime
        .active_window_observed_at
        .lock()
        .unwrap()
        .get(&account)
        .unwrap();
    runtime.active_window_observed_at.lock().unwrap().insert(
        account.clone(),
        original_observed_at - MARKET_WINDOW_SETTLE_DELAY - Duration::from_millis(1),
    );

    runtime
        .remember_window_snapshot(account.clone(), window.clone())
        .unwrap();

    let unchanged_observed_at = *runtime
        .active_window_observed_at
        .lock()
        .unwrap()
        .get(&account)
        .unwrap();
    assert!(unchanged_observed_at < original_observed_at);

    let same_title_refresh = WindowSnapshot {
        slots: vec![saf_core::gui::WindowSlot {
            slot: 15,
            name: "gold_ingot".to_string(),
            display_name: "Manage Auctions".to_string(),
            lore: vec!["Updated content without a title transition.".to_string()],
            item_uuid: None,
        }],
        ..window.clone()
    };
    runtime
        .remember_window_snapshot(account.clone(), same_title_refresh)
        .unwrap();

    let same_title_observed_at = *runtime
        .active_window_observed_at
        .lock()
        .unwrap()
        .get(&account)
        .unwrap();
    assert!(same_title_observed_at >= original_observed_at);

    let changed_window = WindowSnapshot {
        title: "Manage Auctions".to_string(),
        ..window
    };
    runtime
        .remember_window_snapshot(account.clone(), changed_window)
        .unwrap();

    let changed_observed_at = *runtime
        .active_window_observed_at
        .lock()
        .unwrap()
        .get(&account)
        .unwrap();
    assert!(changed_observed_at >= same_title_observed_at);
}

#[tokio::test]
async fn stopped_accounts_persist_deferred_queue_followups() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string(), "Alt".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.deferred_queue_entries.push(DeferredQueueEntry {
        account: account.clone(),
        action: json!({
            "auctionID": "expired-item-uuid",
            "inv": "expired-item-uuid",
            "price": 9700,
            "time": 48
        }),
        state: BotState::ListingNoName,
        priority: 4,
        ready_at: Instant::now() - Duration::from_millis(1),
    });

    runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "stop Main".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    runtime.poll_once().await.unwrap();

    assert!(runtime.deferred_queue_entries.is_empty());
    assert_eq!(runtime.report().processed_queue_steps, 0);
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::ListingNoName);
    assert_eq!(snapshot.queue[0].action["inv"], json!("expired-item-uuid"));
}

#[cfg(not(feature = "live-cofl"))]
#[tokio::test]
async fn supervisor_stop_disconnects_and_start_reconnects_minecraft() {
    let account = AccountId::new("Main").unwrap();
    let managed = Arc::new(
        ManagedMinecraftClient::connect(account.clone(), MarketActionMode::DryRun, None)
            .await
            .unwrap(),
    );
    let supervisor = LiveAccountSupervisor::new(
        vec![account.clone()],
        vec![account.clone()],
        BTreeMap::from([(account.clone(), managed.clone())]),
        Arc::new(std::sync::atomic::AtomicBool::new(false)),
    );

    assert!(managed.is_connected().await);
    supervisor.stop(Some(account.clone())).await.unwrap();
    assert!(!managed.is_connected().await);
    supervisor.start(&account).await.unwrap();
    assert!(managed.is_connected().await);
}

#[cfg(not(feature = "live-cofl"))]
#[tokio::test]
async fn stop_all_latches_halt_and_start_clears_it() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let main = AccountId::new("Main").unwrap();
    let alt = AccountId::new("Alt").unwrap();
    let managed_main = Arc::new(
        ManagedMinecraftClient::connect(main.clone(), MarketActionMode::DryRun, None)
            .await
            .unwrap(),
    );
    let managed_alt = Arc::new(
        ManagedMinecraftClient::connect(alt.clone(), MarketActionMode::DryRun, None)
            .await
            .unwrap(),
    );
    let halted = Arc::new(AtomicBool::new(false));
    let supervisor = LiveAccountSupervisor::new(
        vec![main.clone(), alt.clone()],
        vec![main.clone(), alt.clone()],
        BTreeMap::from([
            (main.clone(), managed_main.clone()),
            (alt.clone(), managed_alt.clone()),
        ]),
        halted.clone(),
    );

    // A single-account stop must NOT latch the global halt (others keep flipping).
    supervisor.stop(Some(main.clone())).await.unwrap();
    assert!(
        !halted.load(Ordering::SeqCst),
        "a single-account stop must not halt the whole runtime"
    );

    // Stop All is the panic stop: it latches the halt so nothing can restart.
    supervisor.stop(None).await.unwrap();
    assert!(
        halted.load(Ordering::SeqCst),
        "Stop All must latch the runtime halt"
    );

    // An explicit operator start re-arms the runtime.
    supervisor.start(&alt).await.unwrap();
    assert!(
        !halted.load(Ordering::SeqCst),
        "an explicit start must clear the halt"
    );
}

#[tokio::test]
async fn managed_minecraft_reconnects_after_disconnect_event() {
    let account = AccountId::new("Main").unwrap();
    let recorded = Arc::new(RecordedMinecraftClient::new(account.clone()));
    recorded.push_event(MinecraftEvent::Disconnected {
        reason: "socket closed".to_string(),
    });
    let managed = ManagedMinecraftClient {
        account: account.clone(),
        mode: MarketActionMode::DryRun,
        inner: Arc::new(AsyncMutex::new(Some(LiveMinecraftClientBundle {
            minecraft: recorded,
            inventory_provider: None,
        }))),
        reconnect_at: Arc::new(AsyncMutex::new(None)),
        reconnect_delay: Duration::ZERO,
        reconnect_failures: Arc::new(AsyncMutex::new(0)),
        price_lookup: None,
        stopped: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    assert!(matches!(
        managed.next_event().await.unwrap(),
        Some(MinecraftEvent::Disconnected { reason }) if reason == "socket closed"
    ));
    assert!(!managed.is_connected().await);
    assert_eq!(managed.next_event().await.unwrap(), None);
    assert!(managed.is_connected().await);
}

#[tokio::test]
async fn managed_minecraft_reconnect_backoff_grows_after_fast_disconnects() {
    let account = AccountId::new("Main").unwrap();
    let managed = ManagedMinecraftClient::stopped(account, MarketActionMode::DryRun, None);

    managed.mark_runtime_disconnected().await;
    let first = managed.reconnect_at.lock().await.unwrap();
    managed.mark_runtime_disconnected().await;
    let second = managed.reconnect_at.lock().await.unwrap();

    assert!(second.duration_since(first) >= Duration::from_secs(4));
}

#[tokio::test]
async fn managed_minecraft_background_connect_respects_reconnect_backoff() {
    let account = AccountId::new("Main").unwrap();
    // Model a *running* account that was just kicked (transient disconnect): it
    // is not intentionally stopped, so its reconnect is governed by the backoff,
    // not the inert-stop guard.
    let managed = ManagedMinecraftClient {
        account,
        mode: MarketActionMode::DryRun,
        inner: Arc::new(AsyncMutex::new(None)),
        reconnect_at: Arc::new(AsyncMutex::new(None)),
        reconnect_delay: Duration::from_secs(5),
        reconnect_failures: Arc::new(AsyncMutex::new(0)),
        price_lookup: None,
        stopped: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    managed.mark_runtime_disconnected().await;

    let error = managed.connect_if_needed().await.unwrap_err();
    assert!(matches!(error, PortError::Unavailable(message) if message.contains("scheduled in")));
    assert!(!managed.is_connected().await);
}

#[tokio::test]
async fn shutdown_disconnects_managed_minecraft_clients() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let config = SafConfig {
        igns: vec![account.to_string()],
        default_ign: account.to_string(),
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let runtime = LiveRuntime::start(config, options).await.unwrap();
    let managed = runtime.managed_minecraft.get(&account).unwrap().clone();

    assert!(managed.is_connected().await);
    runtime.shutdown_live_clients().await;
    assert!(!managed.is_connected().await);
}

#[tokio::test]
async fn once_runtime_shutdown_disconnects_managed_minecraft_clients() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let config = SafConfig {
        igns: vec![account.to_string()],
        default_ign: account.to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.once = true;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let managed = runtime.managed_minecraft.get(&account).unwrap().clone();

    runtime.poll_once().await.unwrap();
    assert!(managed.is_connected().await);
    runtime.shutdown_runtime().await;
    assert!(!managed.is_connected().await);
}
#[tokio::test]
async fn managed_inventory_provider_enriches_missing_prices() {
    let account = AccountId::new("Main").unwrap();
    let lookup = Arc::new(FixedInventoryPriceLookup::default());
    let managed = ManagedMinecraftClient::from_bundle(
        account.clone(),
        MarketActionMode::DryRun,
        LiveMinecraftClientBundle {
            minecraft: Arc::new(RecordedMinecraftClient::new(account.clone())),
            inventory_provider: Some(Arc::new(FixedInventoryProvider)),
        },
        Some(lookup.clone()),
    );

    let snapshot = managed.snapshot(&account).await.unwrap();

    assert_eq!(snapshot.items[0].price, Some(5_000_000.0));
    assert_eq!(
        lookup.requests.lock().unwrap().as_slice(),
        &["ASPECT_OF_THE_DRAGON".to_string()]
    );
}

#[tokio::test]
async fn managed_inventory_provider_keeps_snapshot_when_price_lookup_fails() {
    let account = AccountId::new("Main").unwrap();
    let managed = ManagedMinecraftClient::from_bundle(
        account.clone(),
        MarketActionMode::DryRun,
        LiveMinecraftClientBundle {
            minecraft: Arc::new(RecordedMinecraftClient::new(account.clone())),
            inventory_provider: Some(Arc::new(FixedInventoryProvider)),
        },
        Some(Arc::new(FailingInventoryPriceLookup)),
    );

    let snapshot = managed.snapshot(&account).await.unwrap();

    assert_eq!(snapshot.items.len(), 1);
    assert_eq!(snapshot.items[0].item_name, "Aspect of the Dragons");
    assert_eq!(snapshot.items[0].price, None);
}

#[tokio::test]
async fn purchase_chat_queues_auto_relist_with_inventory_uuid() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let managed = Arc::new(ManagedMinecraftClient::from_bundle(
        account.clone(),
        MarketActionMode::DryRun,
        LiveMinecraftClientBundle {
            minecraft: Arc::new(RecordedMinecraftClient::new(account.clone())),
            inventory_provider: Some(Arc::new(PurchasedInventoryProvider)),
        },
        None,
    ));
    runtime.managed_minecraft.insert(account.clone(), managed);
    runtime
        .tracked_flips
        .record_flip(
            &account,
            &FlipEvent::from_payload(&json!({
                "id": "auction-1",
                "itemName": "Ancient Necron's Leggings",
                "startingBid": 31_000_000,
                "target": 56_300_000,
                "tag": "NECRON_LEGGINGS",
                "finder": "USER"
            })),
        )
        .unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::ChatMessage {
        text: "You purchased Ancient Necron's Leggings for 31,000,000 coins!".to_string(),
    });
    runtime.minecraft_clients.insert(account.clone(), client);

    runtime.poll_minecraft_once().await.unwrap();

    let records = runtime.dry_run_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state, BotState::Listing);
    assert_eq!(records[0].priority, 1);
    assert_eq!(records[0].action["auctionID"], json!("auction-1"));
    assert_eq!(records[0].action["inventory"], json!("inventory-uuid"));
    assert_eq!(records[0].action["inv"], json!("inventory-uuid"));
    assert_eq!(records[0].action["tag"], json!("NECRON_LEGGINGS"));
    assert_eq!(records[0].action["pricePaid"], json!(31_000_000));
}

#[tokio::test]
async fn purchase_chat_queue_write_errors_retry_without_dropping_pending() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        relist: true,
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let bad_state_base = temp.path().join("state-root-is-file");
    std::fs::write(&bad_state_base, b"not a directory").unwrap();
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), &bad_state_base);
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let managed = Arc::new(ManagedMinecraftClient::from_bundle(
        account.clone(),
        MarketActionMode::Live,
        LiveMinecraftClientBundle {
            minecraft: Arc::new(RecordedMinecraftClient::new(account.clone())),
            inventory_provider: Some(Arc::new(PurchasedInventoryProvider)),
        },
        None,
    ));
    runtime.managed_minecraft.insert(account.clone(), managed);
    runtime
        .tracked_flips
        .record_flip(
            &account,
            &FlipEvent::from_payload(&json!({
                "id": "auction-1",
                "itemName": "Ancient Necron's Leggings",
                "startingBid": 31_000_000,
                "target": 56_300_000,
                "tag": "NECRON_LEGGINGS",
                "finder": "USER"
            })),
        )
        .unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::ChatMessage {
        text: "You purchased Ancient Necron's Leggings for 31,000,000 coins!".to_string(),
    });
    runtime.minecraft_clients.insert(account.clone(), client);

    runtime.poll_minecraft_once().await.unwrap();

    assert_eq!(runtime.pending_purchase_relists.len(), 1);
    assert_eq!(
        runtime.pending_purchase_relists[0].purchase.auction_id,
        "auction-1"
    );

    std::fs::remove_file(&bad_state_base).unwrap();
    std::fs::create_dir_all(&bad_state_base).unwrap();
    runtime.pending_purchase_relists[0].ready_at = Instant::now();
    runtime.drain_pending_purchase_relists().await.unwrap();

    assert!(runtime.pending_purchase_relists.is_empty());
    let snapshot = crate::state_cli::snapshot(&bad_state_base, "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::Listing);
    assert_eq!(
        snapshot.queue[0].action["inventory"],
        json!("inventory-uuid")
    );
}

#[tokio::test]
async fn purchase_relist_retries_until_inventory_uuid_is_visible() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let provider = Arc::new(MutableInventoryProvider::default());
    let managed = Arc::new(ManagedMinecraftClient::from_bundle(
        account.clone(),
        MarketActionMode::DryRun,
        LiveMinecraftClientBundle {
            minecraft: Arc::new(RecordedMinecraftClient::new(account.clone())),
            inventory_provider: Some(provider.clone()),
        },
        None,
    ));
    runtime.managed_minecraft.insert(account.clone(), managed);
    runtime
        .tracked_flips
        .record_flip(
            &account,
            &FlipEvent::from_payload(&json!({
                "id": "auction-1",
                "itemName": "Ancient Necron's Leggings",
                "startingBid": 31_000_000,
                "target": 56_300_000,
                "tag": "NECRON_LEGGINGS",
                "finder": "USER"
            })),
        )
        .unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::ChatMessage {
        text: "You purchased Ancient Necron's Leggings for 31,000,000 coins!".to_string(),
    });
    runtime.minecraft_clients.insert(account.clone(), client);

    runtime.poll_minecraft_once().await.unwrap();
    assert!(runtime.dry_run_records().is_empty());
    assert_eq!(runtime.pending_purchase_relists.len(), 1);

    *provider.items.lock().unwrap() = vec![InventoryItem {
        uuid: Some("inventory-uuid".to_string()),
        item_name: "Ancient Necron's Leggings".to_string(),
        lore: Vec::new(),
        price: Some(56_300_000.0),
        tag: Some("NECRON_LEGGINGS".to_string()),
        slot: Some(12),
        in_hotbar: false,
    }];
    runtime.pending_purchase_relists[0].ready_at = Instant::now();
    runtime.drain_pending_purchase_relists().await.unwrap();

    let records = runtime.dry_run_records();
    assert!(runtime.pending_purchase_relists.is_empty());
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state, BotState::Listing);
    assert_eq!(records[0].action["inventory"], json!("inventory-uuid"));
}

#[tokio::test]
async fn purchase_relist_prefers_exact_inventory_name_over_broad_tag() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let provider = Arc::new(MutableInventoryProvider::default());
    let managed = Arc::new(ManagedMinecraftClient::from_bundle(
        account.clone(),
        MarketActionMode::DryRun,
        LiveMinecraftClientBundle {
            minecraft: Arc::new(RecordedMinecraftClient::new(account.clone())),
            inventory_provider: Some(provider.clone()),
        },
        None,
    ));
    runtime.managed_minecraft.insert(account.clone(), managed);
    *provider.items.lock().unwrap() = vec![
        InventoryItem {
            uuid: Some("wrong-uuid".to_string()),
            item_name: "Clean Necron's Leggings".to_string(),
            lore: Vec::new(),
            price: Some(40_000_000.0),
            tag: Some("POWER_WITHER_LEGGINGS".to_string()),
            slot: Some(11),
            in_hotbar: false,
        },
        InventoryItem {
            uuid: Some("target-uuid".to_string()),
            item_name: "Ancient Necron's Leggings ✪✪✪✪✪➌".to_string(),
            lore: Vec::new(),
            price: Some(128_327_984.0),
            tag: Some("POWER_WITHER_LEGGINGS".to_string()),
            slot: Some(12),
            in_hotbar: false,
        },
    ];
    runtime
        .pending_purchase_relists
        .push(PendingPurchaseRelist {
            account: account.clone(),
            purchase: PurchaseStatsUpdate {
                auction_id: "auction-1".to_string(),
                item_name: "Ancient Necron's Leggings ✪✪✪✪✪➌".to_string(),
                weird_item_name: "Ancient Necron's Leggings 5 star 3".to_string(),
                tag: Some("POWER_WITHER_LEGGINGS".to_string()),
                price: 99_000_000,
                target_price: 128_327_984.0,
                profit: 24_836_504.56,
                finder: "SNIPER_MEDIAN".to_string(),
                volume: None,
                profit_percentage: Some(25.0),
                buy_kind: "NUGGET".to_string(),
                buy_speed_ms: Some(286),
            },
            attempts: PURCHASE_RELIST_MAX_ATTEMPTS,
            claim_attempts: 1,
            ready_at: Instant::now(),
        });

    runtime.drain_pending_purchase_relists().await.unwrap();

    let records = runtime.dry_run_records();
    assert!(runtime.pending_purchase_relists.is_empty());
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state, BotState::Listing);
    assert_eq!(records[0].action["inventory"], json!("target-uuid"));
}

#[tokio::test]
async fn purchase_relist_ignores_equipped_matching_item_until_claimed() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        relist: true,
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let provider = Arc::new(MutableInventoryProvider::default());
    let managed = Arc::new(ManagedMinecraftClient::from_bundle(
        account.clone(),
        MarketActionMode::DryRun,
        LiveMinecraftClientBundle {
            minecraft: Arc::new(RecordedMinecraftClient::new(account.clone())),
            inventory_provider: Some(provider.clone()),
        },
        None,
    ));
    runtime.managed_minecraft.insert(account.clone(), managed);
    *provider.items.lock().unwrap() = vec![InventoryItem {
        uuid: Some("equipped-uuid".to_string()),
        item_name: "Ancient Necron's Leggings \u{272a}\u{272a}\u{272a}\u{272a}\u{272a}\u{279c}"
            .to_string(),
        lore: Vec::new(),
        price: Some(128_327_984.0),
        tag: Some("POWER_WITHER_LEGGINGS".to_string()),
        slot: Some(7),
        in_hotbar: false,
    }];
    runtime
        .pending_purchase_relists
        .push(PendingPurchaseRelist {
            account: account.clone(),
            purchase: PurchaseStatsUpdate {
                auction_id: "auction-1".to_string(),
                item_name:
                    "Ancient Necron's Leggings \u{272a}\u{272a}\u{272a}\u{272a}\u{272a}\u{279c}"
                        .to_string(),
                weird_item_name: "Ancient Necron's Leggings 5 star 3".to_string(),
                tag: Some("POWER_WITHER_LEGGINGS".to_string()),
                price: 99_000_000,
                target_price: 128_327_984.0,
                profit: 24_836_504.56,
                finder: "SNIPER_MEDIAN".to_string(),
                volume: None,
                profit_percentage: Some(25.0),
                buy_kind: "NUGGET".to_string(),
                buy_speed_ms: Some(286),
            },
            attempts: PURCHASE_RELIST_MAX_ATTEMPTS,
            claim_attempts: 0,
            ready_at: Instant::now(),
        });

    runtime.drain_pending_purchase_relists().await.unwrap();

    let records = runtime.dry_run_records();
    assert!(runtime.pending_purchase_relists.is_empty());
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].state,
        BotState::Custom("claimPurchased".to_string())
    );
    assert_eq!(records[0].action["auctionID"], json!("auction-1"));
    assert!(records[0].action.get("inventory").is_none());
    assert!(records[0].action.get("inv").is_none());
}

#[tokio::test]
async fn purchase_relist_refuses_tag_only_inventory_match() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        relist: true,
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let provider = Arc::new(MutableInventoryProvider::default());
    let managed = Arc::new(ManagedMinecraftClient::from_bundle(
        account.clone(),
        MarketActionMode::DryRun,
        LiveMinecraftClientBundle {
            minecraft: Arc::new(RecordedMinecraftClient::new(account.clone())),
            inventory_provider: Some(provider.clone()),
        },
        None,
    ));
    runtime.managed_minecraft.insert(account.clone(), managed);
    *provider.items.lock().unwrap() = vec![InventoryItem {
        uuid: None,
        item_name: "Ancient Necron's Leggings ✪✪✪✪✪➌".to_string(),
        lore: Vec::new(),
        price: Some(128_327_984.0),
        tag: Some("POWER_WITHER_LEGGINGS".to_string()),
        slot: Some(12),
        in_hotbar: false,
    }];
    runtime
        .pending_purchase_relists
        .push(PendingPurchaseRelist {
            account: account.clone(),
            purchase: PurchaseStatsUpdate {
                auction_id: "auction-1".to_string(),
                item_name: "Ancient Necron's Leggings ✪✪✪✪✪➌".to_string(),
                weird_item_name: "Ancient Necron's Leggings 5 star 3".to_string(),
                tag: Some("POWER_WITHER_LEGGINGS".to_string()),
                price: 99_000_000,
                target_price: 128_327_984.0,
                profit: 24_836_504.56,
                finder: "SNIPER_MEDIAN".to_string(),
                volume: None,
                profit_percentage: Some(25.0),
                buy_kind: "NUGGET".to_string(),
                buy_speed_ms: Some(286),
            },
            attempts: PURCHASE_RELIST_MAX_ATTEMPTS,
            claim_attempts: 0,
            ready_at: Instant::now(),
        });

    runtime.drain_pending_purchase_relists().await.unwrap();

    let records = runtime.dry_run_records();
    assert!(runtime.pending_purchase_relists.is_empty());
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].state,
        BotState::Custom("claimPurchased".to_string())
    );
    assert!(records[0].action.get("inventory").is_none());
    assert!(records[0].action.get("inv").is_none());
}

#[tokio::test]
async fn purchase_relist_refuses_tag_shaped_inventory_uuid() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        relist: true,
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let provider = Arc::new(MutableInventoryProvider::default());
    let managed = Arc::new(ManagedMinecraftClient::from_bundle(
        account.clone(),
        MarketActionMode::DryRun,
        LiveMinecraftClientBundle {
            minecraft: Arc::new(RecordedMinecraftClient::new(account.clone())),
            inventory_provider: Some(provider.clone()),
        },
        None,
    ));
    runtime.managed_minecraft.insert(account.clone(), managed);
    *provider.items.lock().unwrap() = vec![InventoryItem {
        uuid: Some("POWER_WITHER_LEGGINGS".to_string()),
        item_name: "Ancient Necron's Leggings ✪✪✪✪✪➌".to_string(),
        lore: Vec::new(),
        price: Some(128_327_984.0),
        tag: Some("POWER_WITHER_LEGGINGS".to_string()),
        slot: Some(12),
        in_hotbar: false,
    }];
    runtime
        .pending_purchase_relists
        .push(PendingPurchaseRelist {
            account: account.clone(),
            purchase: PurchaseStatsUpdate {
                auction_id: "auction-1".to_string(),
                item_name: "Ancient Necron's Leggings ✪✪✪✪✪➌".to_string(),
                weird_item_name: "Ancient Necron's Leggings 5 star 3".to_string(),
                tag: Some("POWER_WITHER_LEGGINGS".to_string()),
                price: 99_000_000,
                target_price: 128_327_984.0,
                profit: 24_836_504.56,
                finder: "SNIPER_MEDIAN".to_string(),
                volume: None,
                profit_percentage: Some(25.0),
                buy_kind: "NUGGET".to_string(),
                buy_speed_ms: Some(286),
            },
            attempts: PURCHASE_RELIST_MAX_ATTEMPTS,
            claim_attempts: 0,
            ready_at: Instant::now(),
        });

    runtime.drain_pending_purchase_relists().await.unwrap();

    let records = runtime.dry_run_records();
    assert!(runtime.pending_purchase_relists.is_empty());
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].state,
        BotState::Custom("claimPurchased".to_string())
    );
    assert_eq!(records[0].action["auctionID"], json!("auction-1"));
    assert!(records[0].action.get("inventory").is_none());
    assert!(records[0].action.get("inv").is_none());
}

#[tokio::test]
async fn purchase_relist_refuses_unique_tag_only_inventory_match() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        relist: true,
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let provider = Arc::new(MutableInventoryProvider::default());
    let managed = Arc::new(ManagedMinecraftClient::from_bundle(
        account.clone(),
        MarketActionMode::DryRun,
        LiveMinecraftClientBundle {
            minecraft: Arc::new(RecordedMinecraftClient::new(account.clone())),
            inventory_provider: Some(provider.clone()),
        },
        None,
    ));
    runtime.managed_minecraft.insert(account.clone(), managed);
    *provider.items.lock().unwrap() = vec![InventoryItem {
        uuid: Some("wrong-uuid".to_string()),
        item_name: "Clean Necron's Leggings".to_string(),
        lore: Vec::new(),
        price: Some(40_000_000.0),
        tag: Some("POWER_WITHER_LEGGINGS".to_string()),
        slot: Some(12),
        in_hotbar: false,
    }];
    runtime
        .pending_purchase_relists
        .push(PendingPurchaseRelist {
            account: account.clone(),
            purchase: PurchaseStatsUpdate {
                auction_id: "auction-1".to_string(),
                item_name: "Ancient Necron's Leggings ✪✪✪✪✪➌".to_string(),
                weird_item_name: "Ancient Necron's Leggings 5 star 3".to_string(),
                tag: Some("POWER_WITHER_LEGGINGS".to_string()),
                price: 99_000_000,
                target_price: 128_327_984.0,
                profit: 24_836_504.56,
                finder: "SNIPER_MEDIAN".to_string(),
                volume: None,
                profit_percentage: Some(25.0),
                buy_kind: "NUGGET".to_string(),
                buy_speed_ms: Some(286),
            },
            attempts: PURCHASE_RELIST_MAX_ATTEMPTS,
            claim_attempts: 0,
            ready_at: Instant::now(),
        });

    runtime.drain_pending_purchase_relists().await.unwrap();

    let records = runtime.dry_run_records();
    assert!(runtime.pending_purchase_relists.is_empty());
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].state,
        BotState::Custom("claimPurchased".to_string())
    );
    assert_eq!(records[0].action["auctionID"], json!("auction-1"));
    assert!(records[0].action.get("inventory").is_none());
    assert!(records[0].action.get("inv").is_none());
}

#[tokio::test]
async fn purchase_relist_inventory_errors_queue_explicit_claim_without_tag_fallback() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        relist: true,
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let managed = Arc::new(ManagedMinecraftClient::from_bundle(
        account.clone(),
        MarketActionMode::DryRun,
        LiveMinecraftClientBundle {
            minecraft: Arc::new(RecordedMinecraftClient::new(account.clone())),
            inventory_provider: Some(Arc::new(FailingInventoryProvider)),
        },
        None,
    ));
    runtime.managed_minecraft.insert(account.clone(), managed);
    let purchase = PurchaseStatsUpdate {
        auction_id: "auction-1".to_string(),
        item_name: "Ancient Necron's Leggings".to_string(),
        weird_item_name: "Ancient Necron's Leggings".to_string(),
        tag: Some("NECRON_LEGGINGS".to_string()),
        price: 31_000_000,
        target_price: 56_300_000.0,
        profit: 25_300_000.0,
        finder: "USER".to_string(),
        volume: None,
        profit_percentage: None,
        buy_kind: "NUGGET".to_string(),
        buy_speed_ms: None,
    };
    runtime
        .pending_purchase_relists
        .push(PendingPurchaseRelist {
            account: account.clone(),
            purchase,
            attempts: 0,
            claim_attempts: 0,
            ready_at: Instant::now(),
        });

    runtime.drain_pending_purchase_relists().await.unwrap();

    assert_eq!(runtime.pending_purchase_relists.len(), 1);
    assert_eq!(runtime.pending_purchase_relists[0].attempts, 1);
    assert!(runtime.dry_run_records().is_empty());

    runtime.pending_purchase_relists[0].attempts = PURCHASE_RELIST_MAX_ATTEMPTS;
    runtime.pending_purchase_relists[0].ready_at = Instant::now();
    runtime.drain_pending_purchase_relists().await.unwrap();

    let records = runtime.dry_run_records();
    assert!(runtime.pending_purchase_relists.is_empty());
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].state,
        BotState::Custom("claimPurchased".to_string())
    );
    assert_eq!(records[0].priority, 1);
    assert_eq!(records[0].action["auctionID"], json!("auction-1"));
    assert_eq!(records[0].action["claimAttempts"], json!(1));
    assert!(records[0].action.get("inventory").is_none());
    assert!(records[0].action.get("inv").is_none());
}

#[tokio::test]
async fn purchase_relist_queue_write_errors_retry_without_dropping_pending() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        relist: true,
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let bad_state_base = temp.path().join("state-root-is-file");
    std::fs::write(&bad_state_base, b"not a directory").unwrap();
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), &bad_state_base);
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let purchase = PurchaseStatsUpdate {
        auction_id: "auction-1".to_string(),
        item_name: "Ancient Necron's Leggings".to_string(),
        weird_item_name: "Ancient Necron's Leggings".to_string(),
        tag: Some("NECRON_LEGGINGS".to_string()),
        price: 31_000_000,
        target_price: 56_300_000.0,
        profit: 25_300_000.0,
        finder: "USER".to_string(),
        volume: None,
        profit_percentage: None,
        buy_kind: "NUGGET".to_string(),
        buy_speed_ms: None,
    };
    runtime
        .pending_purchase_relists
        .push(PendingPurchaseRelist {
            account: account.clone(),
            purchase,
            attempts: PURCHASE_RELIST_MAX_ATTEMPTS,
            claim_attempts: 0,
            ready_at: Instant::now(),
        });

    runtime.drain_pending_purchase_relists().await.unwrap();

    assert_eq!(runtime.pending_purchase_relists.len(), 1);
    assert_eq!(
        runtime.pending_purchase_relists[0].attempts,
        PURCHASE_RELIST_MAX_ATTEMPTS.saturating_add(1)
    );
    assert!(runtime.pending_purchase_relists[0].ready_at > Instant::now());
}

#[tokio::test]
async fn claim_purchased_open_retries_then_clears_stale_entry() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "auction-1",
                "itemName": "Cornucopia Crystal",
                "claimAttempts": 2
            }),
            BotState::Custom("claimPurchased".to_string()),
            1,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        relist: true,
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;

    runtime.process_market_queue_once().await.unwrap();
    for _ in 0..CLAIM_PURCHASED_OPEN_MAX_ATTEMPTS {
        if let Some(pending) = runtime.pending_market_steps.get_mut(&account) {
            pending.last_attempt =
                Instant::now() - MARKET_STEP_RETRY_INTERVAL - std::time::Duration::from_millis(1);
        }
        runtime.process_market_queue_once().await.unwrap();
    }

    let actions = client.actions();
    assert_eq!(actions.len(), CLAIM_PURCHASED_OPEN_MAX_ATTEMPTS as usize);
    assert!(actions.iter().all(|action| {
        matches!(action, MinecraftAction::OpenAuction(auction_id) if auction_id.as_str() == "auction-1")
    }));
    assert_eq!(runtime.report().completed_queue_entries, 1);
    assert!(!runtime.pending_market_steps.contains_key(&account));
    assert!(!runtime.pending_open_auction_retries.contains_key(&account));
    assert!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .is_empty()
    );
}

#[tokio::test]
async fn live_claim_purchased_entry_claims_then_retries_inventory_relist() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "auction-1",
                "itemName": "Ancient Necron's Leggings",
                "weirdItemName": "Ancient Necron's Leggings",
                "tag": "NECRON_LEGGINGS",
                "price": 54_600_000,
                "targetPrice": 56_300_000,
                "pricePaid": 31_000_000,
                "profit": 25_300_000,
                "finder": "USER",
                "claimAttempts": 1
            }),
            BotState::Custom("claimPurchased".to_string()),
            1,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        relist: true,
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let provider = Arc::new(MutableInventoryProvider::default());
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    let managed = runtime.managed_minecraft.get(&account).unwrap().clone();
    *managed.inner.lock().await = Some(LiveMinecraftClientBundle {
        minecraft: client.clone(),
        inventory_provider: Some(provider.clone()),
    });
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "BIN Auction View".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_block".to_string(),
                display_name: "Claim Item".to_string(),
                lore: vec!["Click to claim this auction".to_string()],
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![MinecraftAction::ClickSlot(31), MinecraftAction::CloseWindow]
    );
    assert_eq!(runtime.pending_purchase_relists.len(), 1);
    assert_eq!(runtime.pending_purchase_relists[0].claim_attempts, 1);
    assert!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .is_empty()
    );

    *provider.items.lock().unwrap() = vec![InventoryItem {
        uuid: Some("inventory-uuid".to_string()),
        item_name: "Ancient Necron's Leggings".to_string(),
        lore: Vec::new(),
        price: Some(56_300_000.0),
        tag: Some("NECRON_LEGGINGS".to_string()),
        slot: Some(12),
        in_hotbar: false,
    }];
    runtime.pending_purchase_relists[0].ready_at = Instant::now();
    runtime.drain_pending_purchase_relists().await.unwrap();

    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::Listing);
    assert_eq!(
        snapshot.queue[0].action["inventory"],
        json!("inventory-uuid")
    );
    assert_eq!(snapshot.queue[0].action["pricePaid"], json!(31_000_000));
}

#[tokio::test]
async fn market_queue_snapshot_errors_skip_account_without_stopping_runtime() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let saved_dir = temp.path().join("SavedData");
    std::fs::create_dir_all(&saved_dir).unwrap();
    std::fs::write(saved_dir.join("Main.json"), b"{not valid json").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(runtime.report().processed_queue_steps, 0);
    assert_eq!(runtime.report().completed_queue_entries, 0);
    assert!(!runtime.pending_market_steps.contains_key(&account));
}

#[tokio::test]
async fn live_queue_executor_removes_completed_queue_entry() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({}),
            BotState::Custom("reconcileAuctions".to_string()),
            5,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    options.once = true;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: Vec::new(),
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(runtime.report().completed_queue_entries, 1);
    assert!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .is_empty()
    );
}

#[tokio::test]
async fn live_queue_executor_executes_final_action_before_removal() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"auctionID": "auction-1"}),
            BotState::Buying,
            5,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    options.once = true;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "BIN Auction View".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 33,
                name: "gold_nugget".to_string(),
                display_name: "Buy Item".to_string(),
                lore: vec!["Click to buy".to_string()],
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();
    let report = runtime.report();

    assert_eq!(report.processed_queue_steps, 1);
    assert_eq!(report.completed_queue_entries, 1);
    assert!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .is_empty()
    );
}

#[tokio::test]
async fn live_market_action_errors_leave_entry_queued_for_retry() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"auctionID": "auction-1"}),
            BotState::Buying,
            5,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    options.once = true;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "BIN Auction View".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 33,
                name: "gold_nugget".to_string(),
                display_name: "Buy Item".to_string(),
                lore: vec!["Click to buy".to_string()],
                item_uuid: None,
            }],
        },
    );

    let attempts = install_failing_minecraft_client(&mut runtime, &account).await;

    runtime.process_market_queue_once().await.unwrap();
    runtime.process_market_queue_once().await.unwrap();
    let report = runtime.report();

    assert_eq!(*attempts.lock().unwrap(), 1);
    assert_eq!(report.processed_queue_steps, 0);
    assert_eq!(report.completed_queue_entries, 0);
    assert!(runtime.pending_market_steps.contains_key(&account));
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );
}

#[tokio::test]
async fn live_listing_confirm_waits_for_success_signal_before_removing_queue_entry() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "item-uuid",
                "inv": "item-uuid",
                "price": 12_345_678,
                "time": 48
            }),
            BotState::ListingNoName,
            4,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Confirm BIN Auction".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_nugget".to_string(),
                display_name: "Confirm Auction".to_string(),
                lore: vec![
                    "Price: 12,345,678 coins".to_string(),
                    "Click to create BIN auction".to_string(),
                ],
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(runtime.report().completed_queue_entries, 0);
    assert_eq!(runtime.pending_listing_confirmations.len(), 1);
    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(31)]);
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );

    client.push_event(MinecraftEvent::ChatMessage {
        text: "BIN Auction started for Test Item!".to_string(),
    });
    runtime.poll_minecraft_once().await.unwrap();

    assert_eq!(runtime.report().completed_queue_entries, 1);
    assert!(runtime.pending_listing_confirmations.is_empty());
    assert!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .is_empty()
    );
    assert_eq!(
        runtime
            .stats
            .stats(&account)
            .await
            .unwrap()
            .auction_slots_used,
        Some(1)
    );
}

#[tokio::test]
async fn live_listing_confirm_accepts_compact_price_lore() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "item-uuid",
                "inv": "item-uuid",
                "price": 25_100_000,
                "time": 48
            }),
            BotState::ListingNoName,
            4,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Confirm BIN Auction".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_nugget".to_string(),
                display_name: "Confirm Auction".to_string(),
                lore: vec![
                    "Price: 25.1M coins".to_string(),
                    "Click to create BIN auction".to_string(),
                ],
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(31)]);
    assert_eq!(runtime.pending_listing_confirmations.len(), 1);
    assert!(runtime.pending_listing_price_mismatch_retries.is_empty());
}

#[tokio::test]
async fn live_listing_confirm_accepts_one_percent_creation_fee_lore() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "item-uuid",
                "inv": "item-uuid",
                "price": 5_600_000,
                "time": 48
            }),
            BotState::ListingNoName,
            4,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Confirm BIN Auction".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 11,
                name: "lime_wool".to_string(),
                display_name: "Confirm Auction".to_string(),
                lore: vec![
                    "Cost: 57,200 coins".to_string(),
                    "Click to create BIN auction".to_string(),
                ],
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(11)]);
    assert_eq!(runtime.pending_listing_confirmations.len(), 1);
    assert!(runtime.pending_listing_price_mismatch_retries.is_empty());
}

#[tokio::test]
async fn live_listing_confirm_accepts_high_value_creation_fee_lore() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "item-uuid",
                "inv": "item-uuid",
                "price": 106_500_000,
                "time": 48
            }),
            BotState::ListingNoName,
            4,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Confirm BIN Auction".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 11,
                name: "lime_wool".to_string(),
                display_name: "Confirm Auction".to_string(),
                lore: vec![
                    "Cost: 2,663,700 coins".to_string(),
                    "Click to create BIN auction".to_string(),
                ],
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(11)]);
    assert_eq!(runtime.pending_listing_confirmations.len(), 1);
    assert!(runtime.pending_listing_price_mismatch_retries.is_empty());
}

#[tokio::test]
async fn listing_confirm_price_mismatch_retries_then_reconciles() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "item-uuid",
                "inv": "item-uuid",
                "price": 25_100_000,
                "time": 48
            }),
            BotState::ListingNoName,
            4,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    let wrong_price_window = WindowSnapshot {
        title: "Confirm BIN Auction".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 31,
            name: "gold_nugget".to_string(),
            display_name: "Confirm Auction".to_string(),
            lore: vec![
                "Price: 805,160 coins".to_string(),
                "Click to create BIN auction".to_string(),
            ],
            item_uuid: None,
        }],
    };
    runtime
        .active_windows
        .lock()
        .unwrap()
        .insert(account.clone(), wrong_price_window.clone());

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::CloseWindow]);
    assert_eq!(runtime.report().completed_queue_entries, 0);
    assert_eq!(
        runtime
            .pending_listing_price_mismatch_retries
            .get(&account)
            .unwrap()
            .attempts,
        1
    );

    runtime
        .active_windows
        .lock()
        .unwrap()
        .insert(account.clone(), wrong_price_window.clone());
    runtime.process_market_queue_once().await.unwrap();
    assert_eq!(client.actions(), vec![MinecraftAction::CloseWindow]);

    for expected_attempt in 2..=MISSING_LISTING_INVENTORY_MAX_ATTEMPTS {
        runtime
            .pending_listing_price_mismatch_retries
            .get_mut(&account)
            .unwrap()
            .retry_at = Instant::now();
        runtime
            .active_windows
            .lock()
            .unwrap()
            .insert(account.clone(), wrong_price_window.clone());
        runtime.process_market_queue_once().await.unwrap();

        assert_eq!(
            client.actions().len(),
            usize::from(expected_attempt),
            "one close-window action should be sent per mismatch attempt"
        );
    }

    assert_eq!(runtime.report().completed_queue_entries, 1);
    assert!(
        !runtime
            .pending_listing_price_mismatch_retries
            .contains_key(&account)
    );
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(
        snapshot.queue[0].state,
        BotState::Custom("reconcileAuctions".to_string())
    );
    assert_eq!(
        snapshot.queue[0].action["reason"],
        json!("listing-status-unclear")
    );
}

#[tokio::test]
async fn listing_confirm_retries_while_confirmation_window_stays_open() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "item-uuid",
                "inv": "item-uuid",
                "price": 12_345_678,
                "time": 48
            }),
            BotState::ListingNoName,
            4,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Confirm BIN Auction".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_nugget".to_string(),
                display_name: "Confirm Auction".to_string(),
                lore: vec![
                    "Price: 12,345,678 coins".to_string(),
                    "Click to create BIN auction".to_string(),
                ],
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();
    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(31)]);

    runtime
        .pending_market_steps
        .get_mut(&account)
        .unwrap()
        .last_attempt = Instant::now() - MARKET_STEP_RETRY_INTERVAL - Duration::from_millis(1);
    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::ClickSlot(31),
            MinecraftAction::ClickSlot(31)
        ]
    );
    assert_eq!(runtime.report().completed_queue_entries, 0);
    assert_eq!(
        runtime
            .pending_listing_confirmations
            .get(&account)
            .unwrap()
            .attempts,
        2
    );
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );
}

#[tokio::test]
async fn listing_confirm_timeout_releases_queue_entry_for_retry() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "item-uuid",
                "inv": "item-uuid",
                "price": 12_345_678,
                "time": 48
            }),
            BotState::ListingNoName,
            4,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Confirm BIN Auction".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_nugget".to_string(),
                display_name: "Confirm Auction".to_string(),
                lore: vec![
                    "Price: 12,345,678 coins".to_string(),
                    "Click to create BIN auction".to_string(),
                ],
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();
    runtime.active_windows.lock().unwrap().remove(&account);
    runtime
        .pending_listing_confirmations
        .get_mut(&account)
        .unwrap()
        .last_click_at = Instant::now() - Duration::from_secs(16);

    runtime.process_market_queue_once().await.unwrap();

    assert!(runtime.pending_listing_confirmations.is_empty());
    assert_eq!(runtime.report().completed_queue_entries, 0);
    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::ClickSlot(31),
            MinecraftAction::Chat("/ah".to_string())
        ]
    );
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );
}

#[tokio::test]
async fn listing_confirm_active_listing_window_closes_before_next_buy() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let store = FileQueueStore::new(temp.path());
    store
        .add(
            &account,
            json!({
                "auctionID": "item-uuid",
                "inv": "item-uuid",
                "price": 12_345_678,
                "time": 48
            }),
            BotState::ListingNoName,
            4,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Confirm BIN Auction".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_nugget".to_string(),
                display_name: "Confirm Auction".to_string(),
                lore: vec![
                    "Price: 12,345,678 coins".to_string(),
                    "Click to create BIN auction".to_string(),
                ],
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 31,
            name: "gold_nugget".to_string(),
            display_name: "Buy Item Right Now".to_string(),
            lore: vec!["Click to buy this auction".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_minecraft_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![MinecraftAction::ClickSlot(31), MinecraftAction::CloseWindow]
    );
    assert!(
        runtime
            .active_windows
            .lock()
            .unwrap()
            .get(&account)
            .is_none()
    );
    assert_eq!(runtime.report().completed_queue_entries, 1);

    store
        .add(
            &account,
            json!({"auctionID": "next-buy"}),
            BotState::Buying,
            5,
        )
        .await
        .unwrap();
    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::ClickSlot(31),
            MinecraftAction::CloseWindow,
            MinecraftAction::OpenAuction(saf_core::AuctionId::new("next-buy").unwrap())
        ]
    );
}

#[tokio::test]
async fn missing_listing_inventory_retries_before_reconcile_and_clears_entry() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "auction-1",
                "inventory": "missing-uuid",
                "inv": "missing-uuid",
                "price": 12_345_678,
                "time": 48
            }),
            BotState::Listing,
            10,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    let missing_create_window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 13,
                name: "barrier".to_string(),
                display_name: "Item to Auction".to_string(),
                lore: vec!["Click an item in your inventory.".to_string()],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 20,
                name: "diamond_sword".to_string(),
                display_name: "Different Item".to_string(),
                lore: Vec::new(),
                item_uuid: Some("different-uuid".to_string()),
            },
        ],
    };
    runtime
        .active_windows
        .lock()
        .unwrap()
        .insert(account.clone(), missing_create_window.clone());

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::CloseWindow]);
    assert_eq!(runtime.report().completed_queue_entries, 0);
    assert!(
        runtime
            .pending_missing_listing_inventory_retries
            .contains_key(&account)
    );
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::Listing);

    for expected_attempt in 2..=MISSING_LISTING_INVENTORY_MAX_ATTEMPTS {
        runtime
            .pending_missing_listing_inventory_retries
            .get_mut(&account)
            .unwrap()
            .retry_at = Instant::now();
        runtime
            .active_windows
            .lock()
            .unwrap()
            .insert(account.clone(), missing_create_window.clone());
        runtime.process_market_queue_once().await.unwrap();

        assert_eq!(
            client.actions().len(),
            usize::from(expected_attempt),
            "one close-window action should be sent per missing inventory attempt"
        );
    }

    assert!(
        runtime
            .active_windows
            .lock()
            .unwrap()
            .get(&account)
            .is_none()
    );
    assert_eq!(runtime.report().completed_queue_entries, 1);
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(
        snapshot.queue[0].state,
        BotState::Custom("reconcileAuctions".to_string())
    );
    assert_eq!(
        snapshot.queue[0].action["reason"],
        json!("listing-status-unclear")
    );
}

#[tokio::test]
async fn missing_listing_inventory_retries_loaded_empty_create_window() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "auction-1",
                "inventory": "missing-uuid",
                "inv": "missing-uuid",
                "price": 12_345_678,
                "time": 48
            }),
            BotState::Listing,
            10,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    let missing_create_window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: (0..54)
            .map(|slot| saf_core::gui::WindowSlot {
                slot,
                name: if slot == 13 {
                    "stone_button".to_string()
                } else {
                    "black_stained_glass_pane".to_string()
                },
                display_name: if slot == 13 {
                    "Click an item in your inventory!".to_string()
                } else {
                    "Black Stained Glass Pane".to_string()
                },
                lore: if slot == 13 {
                    vec!["Selects it for auction".to_string()]
                } else {
                    Vec::new()
                },
                item_uuid: None,
            })
            .collect(),
    };
    runtime
        .active_windows
        .lock()
        .unwrap()
        .insert(account.clone(), missing_create_window.clone());

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::CloseWindow]);
    assert!(
        runtime
            .pending_missing_listing_inventory_retries
            .contains_key(&account)
    );

    for expected_attempt in 2..=MISSING_LISTING_INVENTORY_MAX_ATTEMPTS {
        runtime
            .pending_missing_listing_inventory_retries
            .get_mut(&account)
            .unwrap()
            .retry_at = Instant::now();
        runtime
            .active_windows
            .lock()
            .unwrap()
            .insert(account.clone(), missing_create_window.clone());
        runtime.process_market_queue_once().await.unwrap();

        assert_eq!(
            client.actions().len(),
            usize::from(expected_attempt),
            "one close-window action should be sent per missing inventory attempt"
        );
    }

    assert_eq!(runtime.report().completed_queue_entries, 1);
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(
        snapshot.queue[0].state,
        BotState::Custom("reconcileAuctions".to_string())
    );
}

#[tokio::test]
async fn missing_expired_relist_inventory_retries_listing_without_reconcile() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let action = json!({
        "auctionID": "expired-item-uuid",
        "inv": "expired-item-uuid",
        "price": 9_700,
        "oldPrice": 10_000,
        "pricePaid": 0,
        "time": 48
    });
    FileQueueStore::new(temp.path())
        .add(&account, action.clone(), BotState::ListingNoName, 4)
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    let missing_create_window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 13,
                name: "barrier".to_string(),
                display_name: "Item to Auction".to_string(),
                lore: vec!["Click an item in your inventory.".to_string()],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 20,
                name: "diamond_sword".to_string(),
                display_name: "Different Item".to_string(),
                lore: Vec::new(),
                item_uuid: Some("different-uuid".to_string()),
            },
        ],
    };
    runtime
        .active_windows
        .lock()
        .unwrap()
        .insert(account.clone(), missing_create_window.clone());

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::CloseWindow]);
    assert_eq!(runtime.report().completed_queue_entries, 0);
    assert!(runtime.pending_market_steps.contains_key(&account));
    assert!(
        runtime
            .pending_missing_listing_inventory_retries
            .contains_key(&account)
    );
    runtime
        .active_windows
        .lock()
        .unwrap()
        .insert(account.clone(), missing_create_window);
    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::CloseWindow]);
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::ListingNoName);
    assert_eq!(snapshot.queue[0].action, action);
}

#[tokio::test]
async fn listing_inventory_guard_allows_present_uuid_selection() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "auction-1",
                "inventory": "wanted-uuid",
                "inv": "wanted-uuid",
                "price": 12_345_678,
                "time": 48
            }),
            BotState::Listing,
            10,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Create BIN Auction".to_string(),
            slots: vec![
                saf_core::gui::WindowSlot {
                    slot: 13,
                    name: "barrier".to_string(),
                    display_name: "Item to Auction".to_string(),
                    lore: vec!["Click an item in your inventory.".to_string()],
                    item_uuid: None,
                },
                saf_core::gui::WindowSlot {
                    slot: 20,
                    name: "diamond_sword".to_string(),
                    display_name: "Wanted Item".to_string(),
                    lore: Vec::new(),
                    item_uuid: Some("wanted-uuid".to_string()),
                },
            ],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(20)]);
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::Listing);
}

#[tokio::test]
async fn listing_inventory_guard_blocks_tag_fallback_for_concrete_uuid() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "auction-1",
                "inventory": "real-purchased-uuid",
                "inv": "real-purchased-uuid",
                "tag": "SHARK_SCALE_BOOTS",
                "itemName": "Submerged Shark Scale Boots",
                "price": 25_100_000,
                "time": 48
            }),
            BotState::Listing,
            10,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Create BIN Auction".to_string(),
            slots: vec![
                saf_core::gui::WindowSlot {
                    slot: 13,
                    name: "barrier".to_string(),
                    display_name: "Item to Auction".to_string(),
                    lore: vec!["Click an item in your inventory.".to_string()],
                    item_uuid: None,
                },
                saf_core::gui::WindowSlot {
                    slot: 82,
                    name: "leather_boots".to_string(),
                    display_name: "Submerged Shark Scale Boots".to_string(),
                    lore: vec!["SHARK_SCALE_BOOTS".to_string()],
                    item_uuid: Some("different-uuid".to_string()),
                },
            ],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::CloseWindow]);
    assert_eq!(runtime.report().completed_queue_entries, 0);
    assert!(
        runtime
            .pending_missing_listing_inventory_retries
            .contains_key(&account)
    );
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::Listing);
}

#[tokio::test]
async fn listing_inventory_guard_accepts_selected_item_without_visible_uuid() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "auction-1",
                "inventory": "real-purchased-uuid",
                "inv": "real-purchased-uuid",
                "tag": "SHARK_SCALE_BOOTS",
                "itemName": "Submerged Shark Scale Boots",
                "price": 25_100_000,
                "time": 48
            }),
            BotState::Listing,
            10,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Create BIN Auction".to_string(),
            slots: vec![
                saf_core::gui::WindowSlot {
                    slot: 13,
                    name: "leather_boots".to_string(),
                    display_name: "Submerged Shark Scale Boots".to_string(),
                    lore: vec!["SHARK_SCALE_BOOTS".to_string()],
                    item_uuid: None,
                },
                saf_core::gui::WindowSlot {
                    slot: 31,
                    name: "gold_ingot".to_string(),
                    display_name: "Auction Price".to_string(),
                    lore: vec!["Price: 0 coins".to_string()],
                    item_uuid: None,
                },
                saf_core::gui::WindowSlot {
                    slot: 82,
                    name: "diamond_sword".to_string(),
                    display_name: "Different Item".to_string(),
                    lore: Vec::new(),
                    item_uuid: Some("other-uuid".to_string()),
                },
            ],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::ClickSlot(31),
            MinecraftAction::TypeText("\"25100000\"".to_string())
        ]
    );
    assert!(
        !runtime
            .pending_missing_listing_inventory_retries
            .contains_key(&account)
    );
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::Listing);
}

#[tokio::test]
async fn listing_inventory_guard_waits_for_partial_create_window() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "auction-1",
                "inventory": "PET_BLACK_CAT",
                "inv": "PET_BLACK_CAT",
                "tag": "PET_BLACK_CAT",
                "itemName": "§7[Lvl 88] §dBlack Cat",
                "price": 70_800_000,
                "time": 48
            }),
            BotState::Listing,
            1,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Create BIN Auction".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 13,
                name: "barrier".to_string(),
                display_name: "Item to Auction".to_string(),
                lore: vec!["Click an item in your inventory.".to_string()],
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert!(client.actions().is_empty());
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::Listing);
    assert_eq!(
        snapshot.queue[0].action["inventory"],
        json!("PET_BLACK_CAT")
    );
}

#[tokio::test]
async fn listing_inventory_guard_allows_tag_fallback_name_selection() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "auction-1",
                "inventory": "PET_BLACK_CAT",
                "inv": "PET_BLACK_CAT",
                "tag": "PET_BLACK_CAT",
                "itemName": "§7[Lvl 88] §dBlack Cat",
                "price": 70_800_000,
                "time": 48
            }),
            BotState::Listing,
            1,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Create BIN Auction".to_string(),
            slots: vec![
                saf_core::gui::WindowSlot {
                    slot: 13,
                    name: "barrier".to_string(),
                    display_name: "Item to Auction".to_string(),
                    lore: vec!["Click an item in your inventory.".to_string()],
                    item_uuid: None,
                },
                saf_core::gui::WindowSlot {
                    slot: 82,
                    name: "player_head".to_string(),
                    display_name: "§7[Lvl 88] §dBlack Cat".to_string(),
                    lore: Vec::new(),
                    item_uuid: Some("actual-pet-uuid".to_string()),
                },
            ],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(82)]);
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::Listing);
}

#[tokio::test]
async fn listing_completion_stats_errors_do_not_restore_completed_queue_entry() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "item-uuid",
                "inv": "item-uuid",
                "price": 12_345_678,
                "time": 48
            }),
            BotState::ListingNoName,
            4,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Confirm BIN Auction".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_nugget".to_string(),
                display_name: "Confirm Auction".to_string(),
                lore: vec![
                    "Price: 12,345,678 coins".to_string(),
                    "Click to create BIN auction".to_string(),
                ],
                item_uuid: None,
            }],
        },
    );
    let auction_slots = runtime.stats.auction_slots.clone();
    let poison_result = std::panic::catch_unwind(move || {
        let _guard = auction_slots.lock().unwrap();
        panic!("poison auction slot stats lock");
    });
    assert!(poison_result.is_err());

    runtime.process_market_queue_once().await.unwrap();
    runtime
        .complete_listing_confirmation_from_chat(&account, "BIN Auction started for Test Item!")
        .await
        .unwrap();

    assert_eq!(runtime.report().completed_queue_entries, 1);
    assert!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .is_empty()
    );
}

#[tokio::test]
async fn completed_queue_cleanup_errors_do_not_repeat_live_action() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "item-uuid",
                "inv": "item-uuid",
                "price": 12_345_678,
                "time": 48
            }),
            BotState::ListingNoName,
            4,
        )
        .await
        .unwrap();
    let saved_file = temp.path().join("SavedData").join("Main.json");
    let saved_data = std::fs::read(&saved_file).unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let actions = Arc::new(Mutex::new(Vec::new()));
    let managed = runtime.managed_minecraft.get(&account).unwrap().clone();
    *managed.inner.lock().await = Some(LiveMinecraftClientBundle {
        minecraft: Arc::new(CorruptingClickMinecraftClient {
            account: account.clone(),
            path: saved_file.clone(),
            corrupted: Arc::new(Mutex::new(false)),
            actions: actions.clone(),
        }),
        inventory_provider: None,
    });
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Confirm BIN Auction".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_nugget".to_string(),
                display_name: "Confirm Auction".to_string(),
                lore: vec![
                    "Price: 12,345,678 coins".to_string(),
                    "Click to create BIN auction".to_string(),
                ],
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();
    runtime
        .complete_listing_confirmation_from_chat(&account, "BIN Auction started for Test Item!")
        .await
        .unwrap();

    assert_eq!(runtime.pending_completed_entries.len(), 1);
    assert_eq!(runtime.report().completed_queue_entries, 1);
    assert_eq!(
        runtime
            .stats
            .stats(&account)
            .await
            .unwrap()
            .auction_slots_used,
        Some(1)
    );
    assert_eq!(
        actions.lock().unwrap().clone(),
        vec![MinecraftAction::ClickSlot(31)]
    );

    std::fs::write(&saved_file, saved_data).unwrap();
    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        actions.lock().unwrap().clone(),
        vec![MinecraftAction::ClickSlot(31)]
    );
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );

    runtime.pending_completed_entries[0].ready_at = Instant::now() - Duration::from_millis(1);
    runtime.drain_pending_completed_entries().await.unwrap();

    assert!(runtime.pending_completed_entries.is_empty());
    assert_eq!(runtime.report().completed_queue_entries, 1);
    assert!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .is_empty()
    );
}
