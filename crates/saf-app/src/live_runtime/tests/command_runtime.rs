use super::*;
#[tokio::test]
async fn command_inbox_cursor_only_reads_appended_lines() {
    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands.jsonl");
    tokio::fs::write(&inbox, "{\"type\":\"terminal\",\"line\":\"users\"}\n")
        .await
        .unwrap();

    let mut cursor = CommandInboxCursor::new(&inbox);
    assert_eq!(cursor.read_new().await.unwrap().lines().count(), 1);
    assert!(cursor.read_new().await.unwrap().is_empty());

    tokio::fs::write(
        &inbox,
        "{\"type\":\"terminal\",\"line\":\"users\"}\n{\"type\":\"terminal\",\"line\":\"global_stats\"}\n",
    )
    .await
    .unwrap();
    let appended = cursor.read_new().await.unwrap();
    assert_eq!(
        appended.trim(),
        "{\"type\":\"terminal\",\"line\":\"global_stats\"}"
    );
}

#[tokio::test]
async fn command_inbox_cursor_can_start_at_existing_end() {
    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands.jsonl");
    tokio::fs::write(&inbox, "{\"type\":\"terminal\",\"line\":\"old\"}\n")
        .await
        .unwrap();

    let mut cursor = CommandInboxCursor::new_at_end(&inbox).await;
    assert!(cursor.read_new().await.unwrap().is_empty());

    tokio::fs::write(
        &inbox,
        "{\"type\":\"terminal\",\"line\":\"old\"}\n{\"type\":\"terminal\",\"line\":\"new\"}\n",
    )
    .await
    .unwrap();
    assert_eq!(
        cursor.read_new().await.unwrap().trim(),
        "{\"type\":\"terminal\",\"line\":\"new\"}"
    );
}

#[tokio::test]
async fn command_inbox_cursor_start_at_end_keeps_partial_tail_pending() {
    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands.jsonl");
    tokio::fs::write(
        &inbox,
        "{\"type\":\"terminal\",\"line\":\"old\"}\n{\"type\":\"terminal\",\"line\":\"new\"",
    )
    .await
    .unwrap();

    let mut cursor = CommandInboxCursor::new_at_end(&inbox).await;
    assert!(cursor.read_new().await.unwrap().is_empty());

    tokio::fs::write(
        &inbox,
        "{\"type\":\"terminal\",\"line\":\"old\"}\n{\"type\":\"terminal\",\"line\":\"new\"}\n",
    )
    .await
    .unwrap();
    assert_eq!(
        cursor.read_new().await.unwrap().trim(),
        "{\"type\":\"terminal\",\"line\":\"new\"}"
    );
}

#[tokio::test]
async fn command_inbox_cursor_defers_incomplete_json_tail() {
    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands.jsonl");
    tokio::fs::write(&inbox, "{\"type\":\"terminal\",\"line\":\"users\"")
        .await
        .unwrap();

    let mut cursor = CommandInboxCursor::new(&inbox);
    assert!(cursor.read_new().await.unwrap().is_empty());

    tokio::fs::write(&inbox, "{\"type\":\"terminal\",\"line\":\"users\"}\n")
        .await
        .unwrap();
    assert_eq!(
        cursor.read_new().await.unwrap().trim(),
        "{\"type\":\"terminal\",\"line\":\"users\"}"
    );
}

#[tokio::test]
async fn command_inbox_cursor_reads_complete_lines_before_partial_tail() {
    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands.jsonl");
    tokio::fs::write(
        &inbox,
        "{\"type\":\"terminal\",\"line\":\"users\"}\n{\"type\":\"terminal\",\"line\":\"global_stats\"",
    )
    .await
    .unwrap();

    let mut cursor = CommandInboxCursor::new(&inbox);
    assert_eq!(
        cursor.read_new().await.unwrap().trim(),
        "{\"type\":\"terminal\",\"line\":\"users\"}"
    );

    tokio::fs::write(
        &inbox,
        "{\"type\":\"terminal\",\"line\":\"users\"}\n{\"type\":\"terminal\",\"line\":\"global_stats\"}\n",
    )
    .await
    .unwrap();
    assert_eq!(
        cursor.read_new().await.unwrap().trim(),
        "{\"type\":\"terminal\",\"line\":\"global_stats\"}"
    );
}

#[tokio::test]
async fn command_inbox_cursor_keeps_complete_no_newline_commands() {
    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands.jsonl");
    tokio::fs::write(&inbox, "{\"type\":\"terminal\",\"line\":\"users\"}")
        .await
        .unwrap();

    let mut cursor = CommandInboxCursor::new(&inbox);
    assert_eq!(
        cursor.read_new().await.unwrap().trim(),
        "{\"type\":\"terminal\",\"line\":\"users\"}"
    );
    assert!(cursor.read_new().await.unwrap().is_empty());
}

#[tokio::test]
async fn command_inbox_read_errors_do_not_stop_poll_loop() {
    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands-dir");
    tokio::fs::create_dir(&inbox).await.unwrap();
    let mut runtime = LiveRuntime::start(
        SafConfig::default(),
        RunLiveOptions::new(&inbox, temp.path()),
    )
    .await
    .unwrap();

    let results = runtime.poll_once().await.unwrap();

    assert!(results.is_empty());
    assert_eq!(runtime.report().processed_commands, 0);
}

#[tokio::test]
async fn long_running_live_runtime_skips_stale_inbox_commands_on_start() {
    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands.jsonl");
    tokio::fs::write(
        &inbox,
        "{\"type\":\"terminal\",\"line\":\"users\"}\n{\"type\":\"terminal\",\"line\":\"list_item stale-item 1m\"}\n",
    )
    .await
    .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(config, RunLiveOptions::new(&inbox, temp.path()))
        .await
        .unwrap();

    assert!(runtime.poll_once().await.unwrap().is_empty());
    assert_eq!(runtime.report().processed_commands, 0);
    assert_eq!(runtime.report().dry_run_market_actions, 0);

    tokio::fs::write(
        &inbox,
        "{\"type\":\"terminal\",\"line\":\"users\"}\n{\"type\":\"terminal\",\"line\":\"list_item stale-item 1m\"}\n{\"type\":\"terminal\",\"line\":\"list_item fresh-item 1m\"}\n",
    )
    .await
    .unwrap();

    let results = runtime.poll_once().await.unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(runtime.report().processed_commands, 1);
    assert_eq!(runtime.report().dry_run_market_actions, 1);
}

#[tokio::test]
async fn minecraft_poll_errors_are_account_scoped() {
    let main = AccountId::new("Main").unwrap();
    let alt = AccountId::new("Alt").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string(), "Alt".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(
        config,
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    let main_state = runtime.island_states.get_mut(&main).unwrap();
    main_state.require_ready_for_market = true;
    main_state.ready = true;
    runtime
        .active_windows
        .lock()
        .unwrap()
        .insert(main.clone(), WindowSnapshot::default());
    runtime.minecraft_clients.insert(
        main.clone(),
        Arc::new(FailingMinecraftEventClient {
            account: main.clone(),
        }),
    );
    let alt_client = Arc::new(RecordedMinecraftClient::new(alt.clone()));
    alt_client.push_event(MinecraftEvent::Ready {
        reason: "spawn".to_string(),
    });
    runtime
        .minecraft_clients
        .insert(alt.clone(), alt_client.clone());

    runtime.poll_minecraft_once().await.unwrap();

    assert_eq!(runtime.processed_minecraft_events, 1);
    assert!(!runtime.active_windows.lock().unwrap().contains_key(&main));
    assert!(!runtime.island_states.get(&main).unwrap().ready);
    assert!(
        runtime
            .island_states
            .get(&alt)
            .unwrap()
            .locraw_due
            .is_some()
    );
}

#[tokio::test]
async fn operator_notifications_are_best_effort() {
    let account = AccountId::new("Main").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut session =
        RuntimeSession::new(BotRuntime::from_config(&config, vec!["Main".to_string()]));
    session.set_notifier(Arc::new(FailingNotifier));
    let notification = Notification {
        title: "Item purchased".to_string(),
        body: "Test payload".to_string(),
        account: Some(account),
    };

    assert!(session.notify(notification.clone()).await.is_err());
    notify_operator_best_effort(&session, notification).await;
}

#[tokio::test]
async fn dry_run_queue_store_does_not_persist_market_actions() {
    let temp = tempfile::tempdir().unwrap();
    let store =
        MarketActionQueueStore::new(FileQueueStore::new(temp.path()), MarketActionMode::DryRun);
    let account = AccountId::new("Main").unwrap();

    assert!(
        store
            .add(
                &account,
                json!({"auctionID": "auction-1"}),
                BotState::ListingNoName,
                4,
            )
            .await
            .unwrap()
    );
    assert!(
        store
            .add(&account, json!("expired-item-uuid"), BotState::Expired, 4)
            .await
            .unwrap()
    );
    assert_eq!(
        store.snapshot(&account).await.unwrap(),
        Vec::<QueueEntry>::new()
    );
    assert_eq!(store.dry_run_records().len(), 2);
}

#[tokio::test]
async fn live_queue_store_persists_market_actions() {
    let temp = tempfile::tempdir().unwrap();
    let store =
        MarketActionQueueStore::new(FileQueueStore::new(temp.path()), MarketActionMode::Live);
    let account = AccountId::new("Main").unwrap();

    store
        .add(
            &account,
            json!({"auctionID": "auction-1"}),
            BotState::ListingNoName,
            4,
        )
        .await
        .unwrap();

    assert_eq!(store.snapshot(&account).await.unwrap().len(), 1);
    assert!(store.dry_run_records().is_empty());
}

#[tokio::test]
async fn run_live_once_processes_new_inbox_commands_with_dry_run_guard() {
    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands.jsonl");
    tokio::fs::write(
        &inbox,
        "{\"type\":\"terminal\",\"line\":\"users\"}\n{\"type\":\"terminal\",\"line\":\"list_item item-uuid 1m\"}\n",
    )
    .await
    .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(&inbox, temp.path());
    options.once = true;

    let report = run_live(config, options).await.unwrap();

    assert_eq!(report.processed_commands, 2);
    assert_eq!(report.dry_run_market_actions, 1);
}

#[tokio::test]
async fn start_default_only_keeps_other_configured_accounts_lazy_but_startable() {
    let temp = tempfile::tempdir().unwrap();
    let main = AccountId::new("Main").unwrap();
    let alt = AccountId::new("Alt").unwrap();
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string(), "Alt".to_string()],
            default_ign: "Main".to_string(),
            start_default_only: true,
            ..SafConfig::default()
        },
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();

    assert_eq!(runtime.report().accounts, vec![main.clone(), alt.clone()]);
    assert_eq!(runtime.report().running_accounts, vec![main.clone()]);
    assert!(
        runtime
            .managed_minecraft
            .get(&main)
            .unwrap()
            .is_connected()
            .await
    );
    assert!(
        !runtime
            .managed_minecraft
            .get(&alt)
            .unwrap()
            .is_connected()
            .await
    );
    assert_eq!(runtime.session.running_accounts(), vec!["Main".to_string()]);

    runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "start Alt".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(
        runtime
            .managed_minecraft
            .get(&alt)
            .unwrap()
            .is_connected()
            .await
    );
    assert_eq!(
        runtime.session.running_accounts(),
        vec!["Main".to_string(), "Alt".to_string()]
    );
    assert_eq!(runtime.report().running_accounts, vec![main, alt]);
}

#[test]
fn auto_rotate_schedule_uses_node_compatible_action_order() {
    let account = AccountId::new("Main").unwrap();
    let rest_first = parse_auto_rotate_schedule(&account, "12r:6f").unwrap();
    assert!(rest_first.rest_first());
    assert_eq!(
        rest_first.action_steps(),
        [
            (
                Duration::from_millis(6 * 3_600_000),
                ScheduledAccountAction::Start,
                Duration::from_millis(12 * 3_600_000)
            ),
            (
                Duration::from_millis(12 * 3_600_000),
                ScheduledAccountAction::Stop,
                Duration::from_millis(6 * 3_600_000)
            )
        ]
    );

    let flip_first = parse_auto_rotate_schedule(&account, "4f:2r").unwrap();
    assert!(!flip_first.rest_first());
    assert_eq!(
        flip_first.action_steps(),
        [
            (
                Duration::from_millis(2 * 3_600_000),
                ScheduledAccountAction::Stop,
                Duration::from_millis(4 * 3_600_000)
            ),
            (
                Duration::from_millis(4 * 3_600_000),
                ScheduledAccountAction::Start,
                Duration::from_millis(2 * 3_600_000)
            )
        ]
    );
}

#[tokio::test]
async fn auto_rotate_rest_first_accounts_start_stopped() {
    let temp = tempfile::tempdir().unwrap();
    let main = AccountId::new("Main").unwrap();
    let alt = AccountId::new("Alt").unwrap();
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.once = true;
    let runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string(), "Alt".to_string()],
            default_ign: "Main".to_string(),
            auto_rotate: BTreeMap::from([
                ("Main".to_string(), "12r:6f".to_string()),
                ("Alt".to_string(), "4f:2r".to_string()),
            ]),
            ..SafConfig::default()
        },
        options,
    )
    .await
    .unwrap();

    assert!(
        !runtime
            .managed_minecraft
            .get(&main)
            .unwrap()
            .is_connected()
            .await
    );
    assert!(
        runtime
            .managed_minecraft
            .get(&alt)
            .unwrap()
            .is_connected()
            .await
    );
    assert_eq!(runtime.session.running_accounts(), vec!["Alt".to_string()]);
}

#[tokio::test]
async fn market_guard_blocks_live_market_clicks_in_dry_run() {
    let inner = Arc::new(RecordedMinecraftClient::new(
        AccountId::new("Main").unwrap(),
    ));
    let guarded = MarketGuardMinecraftClient {
        inner: inner.clone(),
        mode: MarketActionMode::DryRun,
    };

    guarded
        .perform(MinecraftAction::OpenAuction(
            saf_core::AuctionId::new("auction-1").unwrap(),
        ))
        .await
        .unwrap();
    guarded
        .perform(MinecraftAction::Chat("/is".to_string()))
        .await
        .unwrap();

    assert_eq!(
        inner.actions(),
        vec![MinecraftAction::Chat("/is".to_string())]
    );
}

#[tokio::test]
async fn live_runtime_session_executes_non_market_commands() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let runtime = LiveRuntime::start(config, options).await.unwrap();

    let outcome = runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "users".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(outcome, RuntimeOutcome::UsersSnapshot { .. }));
}

#[tokio::test]
async fn live_runtime_cofl_command_is_planned_without_socket() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let temp = tempfile::tempdir().unwrap();
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.connect_cofl = false;
    let runtime = LiveRuntime::start(config, options).await.unwrap();

    let outcome = runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "/cofl ping".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::Planned {
            directive: RuntimeDirective::SendCoflCommand { .. }
        }
    ));
}

#[tokio::test]
async fn live_runtime_schedules_account_actions_through_supervisor() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let runtime = LiveRuntime::start(config, options).await.unwrap();

    let stop = runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "timeout 1 Main".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        stop,
        RuntimeOutcome::AccountScheduled { ref result }
            if result.account == AccountId::new("Main").unwrap()
                && result.action == ScheduledAccountAction::Stop
                && result.delay_ms == 1
    ));
    sleep(Duration::from_millis(25)).await;
    assert!(runtime.session.running_accounts().is_empty());

    let start = runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "start_in 1 Main".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        start,
        RuntimeOutcome::AccountScheduled { ref result }
            if result.account == AccountId::new("Main").unwrap()
                && result.action == ScheduledAccountAction::Start
                && result.delay_ms == 1
    ));
    sleep(Duration::from_millis(25)).await;
    assert_eq!(runtime.session.running_accounts(), vec!["Main".to_string()]);
}

#[tokio::test]
async fn scheduled_account_actions_do_not_run_after_shutdown() {
    let config = SafConfig {
        igns: vec!["Main".to_string(), "Alt".to_string()],
        default_ign: "Main".to_string(),
        start_default_only: true,
        ..SafConfig::default()
    };
    let alt = AccountId::new("Alt").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let runtime = LiveRuntime::start(config, options).await.unwrap();

    let scheduled = runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "start_in 50 Alt".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        scheduled,
        RuntimeOutcome::AccountScheduled { ref result }
            if result.account == alt
                && result.action == ScheduledAccountAction::Start
                && result.delay_ms == 50
    ));
    runtime.shutdown_runtime().await;
    sleep(Duration::from_millis(90)).await;

    assert_eq!(runtime.session.running_accounts(), vec!["Main".to_string()]);
    assert!(
        !runtime
            .managed_minecraft
            .get(&alt)
            .unwrap()
            .is_connected()
            .await
    );
}

#[tokio::test]
async fn live_tracked_flips_queue_list_flip_with_target_metadata() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let runtime = LiveRuntime::start(config, options).await.unwrap();
    let flip = FlipEvent::from_payload(&json!({
        "auctionID": "auction-1",
        "itemName": "Ancient Necron's Leggings",
        "startingBid": 31_000_000,
        "target": 56_300_000,
        "tag": "NECRON_LEGGINGS"
    }));
    runtime.tracked_flips.record_flip(&account, &flip).unwrap();

    let outcome = runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "list_flip auction-1 --time 12h".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::Queued { changed: true, .. }
    ));
    let records = runtime.dry_run_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state, BotState::ListingNoName);
    assert_eq!(records[0].action["price"], json!(56_300_000.0));
    assert_eq!(records[0].action["pricePaid"], json!(31_000_000.0));
    assert_eq!(records[0].action["tag"], json!("NECRON_LEGGINGS"));
    assert_eq!(
        records[0].action["weirdItemName"],
        json!("ANCIENT NECRONS LEGGINGS")
    );
}

#[tokio::test]
async fn live_tracked_flips_fall_back_to_saved_bid_data() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let temp = tempfile::tempdir().unwrap();
    let saved_dir = temp.path().join("SavedData");
    std::fs::create_dir_all(&saved_dir).unwrap();
    std::fs::write(
        saved_dir.join("Main.json"),
        serde_json::to_vec(&json!({
            "bidData": {
                "item-uuid": {
                    "auctionID": "auction-1",
                    "target": 56_300_000,
                    "weirdItemName": "Ancient Necron's Leggings",
                    "tag": "NECRON_LEGGINGS",
                    "pricePaid": 31_000_000
                }
            },
            "queue": []
        }))
        .unwrap(),
    )
    .unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let runtime = LiveRuntime::start(config, options).await.unwrap();

    let outcome = runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "list_flip auction-1 --time 12h".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::Queued { changed: true, .. }
    ));
    let records = runtime.dry_run_records();
    assert_eq!(records[0].action["price"], json!(56_300_000.0));
    assert_eq!(records[0].action["pricePaid"], json!(31_000_000.0));
    assert_eq!(
        records[0].action["weirdItemName"],
        json!("Ancient Necron's Leggings")
    );
}

#[tokio::test]
async fn scoreboard_events_update_live_purse_stats() {
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
    client.push_event(MinecraftEvent::Scoreboard {
        lines: vec!["§6Purse: §e3.2B".to_string()],
    });
    runtime.minecraft_clients.insert(account.clone(), client);

    runtime.poll_minecraft_once().await.unwrap();
    let stats = runtime.stats.stats(&account).await.unwrap();

    assert_eq!(stats.purse, Some(3_200_000_000.0));
}

#[cfg(feature = "live-cofl")]
#[test]
fn live_stats_keeps_latest_scoreboard_for_cofl_open_upload() {
    let account = AccountId::new("Main").unwrap();
    let stats = LiveStatsProvider::new(vec![account.clone()]);
    stats
        .record_scoreboard(&account, &["Purse: 1.2M".to_string()])
        .unwrap();
    stats
        .record_scoreboard(
            &account,
            &["Bits: 120".to_string(), "Piggy: 250k".to_string()],
        )
        .unwrap();

    assert_eq!(
        stats.latest_scoreboard(&account).unwrap(),
        Some(vec!["Bits: 120".to_string(), "Piggy: 250k".to_string()])
    );
}

#[tokio::test]
async fn scoreboard_stats_errors_do_not_stop_minecraft_polling() {
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
    client.push_event(MinecraftEvent::Scoreboard {
        lines: vec!["Purse: 3.2B".to_string()],
    });
    client.push_event(MinecraftEvent::Ready {
        reason: "spawn".to_string(),
    });
    runtime.minecraft_clients.insert(account.clone(), client);
    let purses = runtime.stats.purses.clone();
    let poison_result = std::panic::catch_unwind(move || {
        let _guard = purses.lock().unwrap();
        panic!("poison scoreboard stats lock");
    });
    assert!(poison_result.is_err());

    runtime.poll_minecraft_once().await.unwrap();

    assert_eq!(runtime.processed_minecraft_events, 2);
    assert!(
        runtime
            .island_states
            .get(&account)
            .unwrap()
            .locraw_due
            .is_some()
    );
}

#[tokio::test]
async fn chat_events_update_live_hypixel_ping_stats() {
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
        text: "§aYour Ping - 1,234ms".to_string(),
    });
    runtime.minecraft_clients.insert(account.clone(), client);

    runtime.poll_minecraft_once().await.unwrap();
    let ping = runtime.stats.ping(&account).await.unwrap();

    assert_eq!(ping.hypixel_ping_ms, Some(1_234));
}

#[tokio::test]
async fn chat_events_update_live_profit_counters_from_tracked_flips() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime
        .tracked_flips
        .record_flip(
            &account,
            &FlipEvent::from_payload(&json!({
                "id": "auction-1",
                "itemName": "Ancient Necron's Leggings",
                "startingBid": 31_000_000,
                "target": 56_300_000,
                "finder": "USER",
                "vol": 24,
                "profitPerc": 81.2
            })),
        )
        .unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::ChatMessage {
        text: "You purchased Ancient Necron's Leggings for 31,000,000 coins!".to_string(),
    });
    client.push_event(MinecraftEvent::ChatMessage {
        text: "[Auction] Buyer bought Ancient Necron's Leggings for 54,000,000 coins CLICK!"
            .to_string(),
    });
    runtime.minecraft_clients.insert(account.clone(), client);

    runtime.poll_minecraft_once().await.unwrap();
    let stats = runtime.stats.stats(&account).await.unwrap();

    assert_eq!(stats.bought, 1);
    assert_eq!(stats.sold, 1);
    assert_eq!(
        stats.total_profit,
        saf_core::flip::ihate_taxes(56_300_000.0) - 31_000_000.0
    );
    assert_eq!(stats.user_finder_flips, 0);
    assert!(stats.profit_per_hour.is_some());
}

#[tokio::test]
async fn chat_side_effect_errors_do_not_stop_minecraft_polling() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let bad_state_base = temp.path().join("state-root-is-file");
    std::fs::write(&bad_state_base, b"not a directory").unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), &bad_state_base);
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::ChatMessage {
        text: "[Auction] Buyer bought Ancient Necron's Leggings for 54,000,000 coins CLICK!"
            .to_string(),
    });
    client.push_event(MinecraftEvent::ChatMessage {
        text: "§aYour Ping - 987ms".to_string(),
    });
    runtime.minecraft_clients.insert(account.clone(), client);

    runtime.poll_minecraft_once().await.unwrap();

    assert_eq!(runtime.processed_minecraft_events, 2);
    assert_eq!(
        runtime.stats.ping(&account).await.unwrap().hypixel_ping_ms,
        Some(987)
    );
}

#[tokio::test]
async fn chat_stats_errors_do_not_stop_minecraft_polling() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let purchase_flips = runtime.tracked_flips.purchase_flips.clone();
    let poison_result = std::panic::catch_unwind(move || {
        let _guard = purchase_flips.lock().unwrap();
        panic!("poison tracked purchase flips lock");
    });
    assert!(poison_result.is_err());
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::ChatMessage {
        text: "You purchased Ancient Necron's Leggings for 31,000,000 coins!".to_string(),
    });
    client.push_event(MinecraftEvent::Ready {
        reason: "spawn".to_string(),
    });
    runtime.minecraft_clients.insert(account.clone(), client);

    runtime.poll_minecraft_once().await.unwrap();

    assert_eq!(runtime.processed_minecraft_events, 2);
    assert!(
        runtime
            .island_states
            .get(&account)
            .unwrap()
            .locraw_due
            .is_some()
    );
}

#[test]
fn purchase_chat_stats_update_carries_notifier_payload_data() {
    let account = AccountId::new("Main").unwrap();
    let tracked = LiveTrackedFlipProvider::default();
    tracked
        .record_flip(
            &account,
            &FlipEvent::from_payload(&json!({
                "id": "auction-1",
                "itemName": "Ancient Necron's Leggings",
                "startingBid": 31_000_000,
                "target": 56_300_000,
                "finder": "USER",
                "vol": 24,
                "profitPerc": 81.2
            })),
        )
        .unwrap();
    let stats = LiveStatsProvider::new(vec![account.clone()]);

    let update = stats
        .record_chat_message(
            &account,
            "You purchased Ancient Necron's Leggings for 31,000,000 coins!",
            &tracked,
        )
        .unwrap();

    let purchase = update.purchase.unwrap();
    assert_eq!(purchase.auction_id, "auction-1");
    assert_eq!(purchase.item_name, "Ancient Necron's Leggings");
    assert_eq!(purchase.weird_item_name, "ANCIENT NECRONS LEGGINGS");
    assert_eq!(purchase.tag, None);
    assert_eq!(purchase.price, 31_000_000);
    assert_eq!(purchase.target_price, 56_300_000.0);
    assert_eq!(
        purchase.profit,
        saf_core::flip::ihate_taxes(56_300_000.0) - 31_000_000.0
    );
    assert_eq!(purchase.finder, "USER");
    assert_eq!(purchase.volume, Some(24.0));
    assert_eq!(purchase.profit_percentage, Some(81.2));
    assert_eq!(purchase.buy_kind, "NUGGET");
    assert!(purchase.buy_speed_ms.is_some());
}

#[test]
fn purchase_notifications_apply_configured_webhook_format() {
    let account = AccountId::new("Main").unwrap();
    let config = SafConfig {
        webhook_format: "{0}|{1}|{2}|{3}|{4}|{5}|{6}|{7}|{8}|{9}|{10}|{11}".to_string(),
        ..SafConfig::default()
    };
    let purchase = PurchaseStatsUpdate {
        auction_id: "auction-1".to_string(),
        item_name: "Hyperion".to_string(),
        weird_item_name: "HYPERION".to_string(),
        tag: Some("HYPERION".to_string()),
        price: 30_000_000,
        target_price: 60_000_000.0,
        profit: 28_250_000.0,
        finder: "CraftCost".to_string(),
        volume: Some(24.0),
        profit_percentage: Some(94.1666),
        buy_kind: "BED".to_string(),
        buy_speed_ms: Some(42),
    };

    assert_eq!(
        purchase_notification_body(&config, &account, &purchase),
        "Hyperion|28.2M|30,000,000|60.0M|42|BED|Craft Cost|auction-1|30.0M|Main|24|94.1666"
    );
}

#[test]
fn sold_and_claim_chat_stats_update_carries_reconcile_payload_data() {
    let account = AccountId::new("Main").unwrap();
    let tracked = LiveTrackedFlipProvider::default();
    let stats = LiveStatsProvider::new(vec![account.clone()]);

    let sold = stats
        .record_chat_message(
            &account,
            "[Auction] Buyer bought Ancient Necron's Leggings for 54,000,000 coins CLICK!",
            &tracked,
        )
        .unwrap();
    assert_eq!(
        sold.sold,
        Some(SoldStatsUpdate {
            buyer: "Buyer".to_string(),
            item_name: "Ancient Necron's Leggings".to_string(),
            price: 54_000_000,
        })
    );

    let claim = stats
        .record_chat_message(
            &account,
            "You collected 52,380,000 coins from selling Ancient Necron's Leggings to Buyer in an auction!",
            &tracked,
        )
        .unwrap();
    assert_eq!(
        claim.claim,
        Some(ClaimStatsUpdate {
            coins: 52_380_000,
            item_name: "Ancient Necron's Leggings".to_string(),
            buyer: "Buyer".to_string(),
        })
    );
}

#[test]
fn auction_collection_context_fixes_zero_coin_claim_chat() {
    let account = AccountId::new("Main").unwrap();
    let tracked = LiveTrackedFlipProvider::default();
    let stats = LiveStatsProvider::new(vec![account.clone()]);

    let collection = stats
        .record_chat_message(
            &account,
            "[MVP+] Main collected an auction for 52,380,000 coins!",
            &tracked,
        )
        .unwrap();
    assert_eq!(collection.claim, None);

    let claim = stats
        .record_chat_message(
            &account,
            "You collected 0 coins from selling Ancient Necron's Leggings to Buyer in an auction!",
            &tracked,
        )
        .unwrap();
    assert_eq!(
        claim.claim,
        Some(ClaimStatsUpdate {
            coins: 52_380_000,
            item_name: "Ancient Necron's Leggings".to_string(),
            buyer: "Buyer".to_string(),
        })
    );
}

#[test]
fn zero_coin_claim_waits_for_followup_collection_context() {
    let account = AccountId::new("Main").unwrap();
    let tracked = LiveTrackedFlipProvider::default();
    let stats = LiveStatsProvider::new(vec![account.clone()]);

    let pending = stats
        .record_chat_message(
            &account,
            "You collected 0 coins from selling Ancient Necron's Leggings to Buyer in an auction!",
            &tracked,
        )
        .unwrap();
    assert_eq!(pending.claim, None);

    let collection = stats
        .record_chat_message(
            &account,
            "Main collected an auction for 52,380,000 coins!",
            &tracked,
        )
        .unwrap();
    assert_eq!(
        collection.claim,
        Some(ClaimStatsUpdate {
            coins: 52_380_000,
            item_name: "Ancient Necron's Leggings".to_string(),
            buyer: "Buyer".to_string(),
        })
    );
}

#[tokio::test]
async fn sold_chat_queues_reconciliation_through_dry_run_guard() {
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
        text: "[Auction] Buyer bought Ancient Necron's Leggings for 54,000,000 coins CLICK!"
            .to_string(),
    });
    runtime.minecraft_clients.insert(account.clone(), client);

    runtime.poll_minecraft_once().await.unwrap();

    let stats = runtime.stats.stats(&account).await.unwrap();
    let records = runtime.dry_run_records();
    assert_eq!(stats.sold, 1);
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].state,
        BotState::Custom("reconcileAuctions".to_string())
    );
    assert_eq!(records[0].priority, 0);
    assert_eq!(records[0].action["reason"], json!("sold-message"));
    assert_eq!(
        records[0].action["item"],
        json!("Ancient Necron's Leggings")
    );
    assert_eq!(records[0].action["price"], json!(54_000_000));
    assert_eq!(records[0].action["buyer"], json!("Buyer"));
}

#[tokio::test]
async fn sold_chat_queue_write_errors_retry_reconcile() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let bad_state_base = temp.path().join("state-root-is-file");
    std::fs::write(&bad_state_base, b"not a directory").unwrap();
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), &bad_state_base);
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::ChatMessage {
        text: "[Auction] Buyer bought Ancient Necron's Leggings for 54,000,000 coins CLICK!"
            .to_string(),
    });
    runtime.minecraft_clients.insert(account.clone(), client);

    runtime.poll_minecraft_once().await.unwrap();

    assert_eq!(runtime.deferred_queue_entries.len(), 1);
    assert_eq!(
        runtime.deferred_queue_entries[0].state,
        BotState::Custom("reconcileAuctions".to_string())
    );
    assert_eq!(runtime.deferred_queue_entries[0].priority, 0);
    assert_eq!(
        runtime.deferred_queue_entries[0].action["reason"],
        json!("sold-message")
    );

    std::fs::remove_file(&bad_state_base).unwrap();
    std::fs::create_dir_all(&bad_state_base).unwrap();
    runtime.deferred_queue_entries[0].ready_at = Instant::now();
    runtime.drain_deferred_queue_entries().await.unwrap();

    assert!(runtime.deferred_queue_entries.is_empty());
    let snapshot = crate::state_cli::snapshot(&bad_state_base, "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(
        snapshot.queue[0].state,
        BotState::Custom("reconcileAuctions".to_string())
    );
    assert_eq!(snapshot.queue[0].action["buyer"], json!("Buyer"));
}
