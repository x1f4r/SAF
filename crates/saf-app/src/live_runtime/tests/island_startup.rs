use super::*;
#[tokio::test]
async fn island_readiness_requests_locraw_and_gates_native_market_work() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"auctionID": "auction-1"}),
            BotState::Buying,
            1,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let island = runtime.island_states.get_mut(&account).unwrap();
    island.locraw_delay = Duration::ZERO;
    island.require_ready_for_market = true;
    island.ready = false;
    runtime
        .remember_window_snapshot(
            account.clone(),
            WindowSnapshot {
                title: "BIN Auction View".to_string(),
                slots: vec![saf_core::gui::WindowSlot {
                    slot: 31,
                    name: "gold_nugget".to_string(),
                    display_name: "Buy Item".to_string(),
                    lore: vec!["Click to buy".to_string()],
                    item_uuid: None,
                }],
            },
        )
        .unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::Ready {
        reason: "spawn".to_string(),
    });
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.poll_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![MinecraftAction::Chat("/locraw".to_string())]
    );
    assert!(runtime.dry_run_records().is_empty());

    client.push_event(MinecraftEvent::Scoreboard {
        lines: vec!["Your Island".to_string(), "Purse: 10,000".to_string()],
    });
    runtime.poll_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::Chat("/locraw".to_string()),
            MinecraftAction::Chat("/profiles".to_string())
        ]
    );
    assert!(runtime.dry_run_records().is_empty());

    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Profiles".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 11,
            name: "emerald_block".to_string(),
            display_name: "Selected Profile".to_string(),
            lore: vec!["Co-op with 2 players:".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_once().await.unwrap();

    assert!(runtime.dry_run_records().is_empty());
    assert!(
        client
            .actions()
            .contains(&MinecraftAction::Chat("/sbmenu".to_string()))
    );
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "SkyBlock Menu".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 13,
            name: "cookie".to_string(),
            display_name: "Booster Cookie".to_string(),
            lore: vec!["Duration: 2h".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_once().await.unwrap();

    assert_eq!(runtime.dry_run_records().len(), 1);
}

#[tokio::test]
async fn unready_startup_account_requests_locraw_without_spawn_event() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let island = runtime.island_states.get_mut(&account).unwrap();
    island.locraw_delay = Duration::ZERO;
    island.require_ready_for_market = true;
    island.ready = false;
    island.locraw_due = None;
    island.awaiting_locraw = false;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.poll_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![MinecraftAction::Chat("/locraw".to_string())]
    );
    assert!(runtime.island_states.get(&account).unwrap().awaiting_locraw);
}

#[tokio::test]
async fn stale_startup_locraw_probe_is_retried() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let island = runtime.island_states.get_mut(&account).unwrap();
    island.locraw_delay = Duration::ZERO;
    island.require_ready_for_market = true;
    island.ready = false;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.poll_once().await.unwrap();
    {
        let island = runtime.island_states.get_mut(&account).unwrap();
        island.locraw_requested_at = Some(Instant::now() - Duration::from_secs(60));
    }
    runtime.poll_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::Chat("/locraw".to_string()),
            MinecraftAction::Chat("/locraw".to_string())
        ]
    );
    let island = runtime.island_states.get(&account).unwrap();
    assert!(island.awaiting_locraw);
    assert_eq!(island.locraw_timeout_count, 1);
}

#[tokio::test]
async fn skyblock_maintenance_chat_backs_off_startup_locraw() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let island = runtime.island_states.get_mut(&account).unwrap();
    island.locraw_delay = Duration::ZERO;
    island.require_ready_for_market = true;
    island.ready = false;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.poll_once().await.unwrap();
    client.push_event(MinecraftEvent::ChatMessage {
        text: "SkyBlock is currently undergoing maintenance, find out more at https://status.hypixel.net"
            .to_string(),
    });
    runtime.poll_minecraft_once().await.unwrap();
    runtime.drive_island_checks_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![MinecraftAction::Chat("/locraw".to_string())]
    );
    assert!(
        runtime
            .island_states
            .get(&account)
            .unwrap()
            .movement_backoff_active(Instant::now())
    );
}

#[tokio::test]
async fn repeated_stale_startup_locraw_probe_reconnects_minecraft() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    {
        let island = runtime.island_states.get_mut(&account).unwrap();
        island.locraw_delay = Duration::ZERO;
        island.require_ready_for_market = true;
        island.ready = false;
        island.awaiting_locraw = true;
        island.locraw_requested_at = Some(Instant::now() - Duration::from_secs(60));
        island.locraw_timeout_count = 2;
    }
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.drive_island_checks_once().await.unwrap();

    assert!(client.actions().is_empty());
    assert!(
        runtime
            .managed_minecraft
            .get(&account)
            .unwrap()
            .inner
            .lock()
            .await
            .is_none()
    );
    let island = runtime.island_states.get(&account).unwrap();
    assert!(!island.ready);
    assert!(!island.awaiting_locraw);
}

#[tokio::test]
async fn due_startup_locraw_waits_for_managed_minecraft_reconnect() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    {
        let island = runtime.island_states.get_mut(&account).unwrap();
        island.locraw_delay = Duration::ZERO;
        island.require_ready_for_market = true;
        island.ready = false;
        island.locraw_due = Some(Instant::now() - Duration::from_secs(1));
    }
    runtime
        .managed_minecraft
        .get(&account)
        .unwrap()
        .mark_runtime_disconnected()
        .await;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.drive_island_checks_once().await.unwrap();

    assert!(client.actions().is_empty());
    let island = runtime.island_states.get(&account).unwrap();
    assert!(island.locraw_due.is_some());
    assert!(!island.awaiting_locraw);
}

#[tokio::test]
async fn native_command_readiness_waits_for_minecraft_ready_event() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let running = std::collections::BTreeSet::from([account.clone()]);

    assert!(
        !runtime
            .minecraft_command_ready_accounts(&running, true)
            .await
            .contains(&account)
    );

    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::Ready {
        reason: "spawn".to_string(),
    });
    runtime.minecraft_clients.insert(account.clone(), client);
    runtime.poll_minecraft_once().await.unwrap();

    assert!(
        runtime
            .minecraft_command_ready_accounts(&running, true)
            .await
            .contains(&account)
    );
}

#[tokio::test]
async fn island_locraw_errors_are_account_scoped() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let island = runtime.island_states.get_mut(&account).unwrap();
    island.locraw_delay = Duration::ZERO;
    island.require_ready_for_market = true;
    island.ready = true;
    runtime
        .remember_window_snapshot(
            account.clone(),
            WindowSnapshot {
                title: "BIN Auction View".to_string(),
                slots: Vec::new(),
            },
        )
        .unwrap();
    runtime.schedule_island_locraw(&account);
    let attempts = install_failing_minecraft_client(&mut runtime, &account).await;

    runtime.drive_island_checks_once().await.unwrap();

    assert_eq!(*attempts.lock().unwrap(), 1);
    assert!(!runtime.island_states.get(&account).unwrap().ready);
    assert!(
        !runtime
            .active_windows
            .lock()
            .unwrap()
            .contains_key(&account)
    );
}

#[tokio::test]
async fn island_movement_command_errors_are_account_scoped() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let island = runtime.island_states.get_mut(&account).unwrap();
    island.require_ready_for_market = true;
    island.ready = true;
    runtime
        .remember_window_snapshot(
            account.clone(),
            WindowSnapshot {
                title: "Auction House".to_string(),
                slots: Vec::new(),
            },
        )
        .unwrap();
    let attempts = install_failing_minecraft_client_with_events(
        &mut runtime,
        &account,
        vec![MinecraftEvent::ChatMessage {
            text: r#"{"server":"mini1A","map":"Hub"}"#.to_string(),
        }],
    )
    .await;

    runtime.poll_minecraft_once().await.unwrap();

    assert_eq!(*attempts.lock().unwrap(), 1);
    assert!(!runtime.island_states.get(&account).unwrap().ready);
    assert!(
        !runtime
            .active_windows
            .lock()
            .unwrap()
            .contains_key(&account)
    );
}

#[tokio::test]
async fn startup_profile_request_errors_are_account_scoped() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let island = runtime.island_states.get_mut(&account).unwrap();
    island.require_ready_for_market = true;
    island.ready = false;
    runtime.handle_island_scoreboard(&account, &["Your Island".to_string()]);
    let attempts = install_failing_minecraft_client(&mut runtime, &account).await;

    runtime.drive_startup_profile_scans_once().await.unwrap();

    let island = runtime.island_states.get(&account).unwrap();
    assert_eq!(*attempts.lock().unwrap(), 1);
    assert!(!island.ready);
    assert!(!island.startup_profile_scan_requested);
    assert!(island.startup_profile_scan_due.is_none());
}

#[tokio::test]
async fn island_locraw_chat_moves_cookie_account_to_private_island() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime
        .island_states
        .get_mut(&account)
        .unwrap()
        .locraw_delay = Duration::ZERO;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::ChatMessage {
        text: r#"{"server":"mini1A","map":"Hub"}"#.to_string(),
    });
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.poll_once().await.unwrap();

    assert!(
        client
            .actions()
            .contains(&MinecraftAction::Chat("/is".to_string()))
    );
}

#[tokio::test]
async fn island_visit_friend_confirms_visit_window_when_on_own_island() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        visit_friend: "Friend".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    {
        let island = runtime.island_states.get_mut(&account).unwrap();
        island.require_ready_for_market = true;
        island.ready = false;
    }
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.handle_island_scoreboard(&account, &["Your Island".to_string()]);
    assert!(!runtime.island_states.get(&account).unwrap().ready);
    client.push_event(MinecraftEvent::ChatMessage {
        text: r#"{"server":"mini1A","map":"Private Island"}"#.to_string(),
    });
    runtime.poll_minecraft_once().await.unwrap();
    assert_eq!(
        client.actions(),
        vec![MinecraftAction::Chat("/visit Friend".to_string())]
    );
    assert!(runtime.island_states.get(&account).unwrap().visit_pending);

    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Visit Friend".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 11,
            name: "player_head".to_string(),
            display_name: "Visit Island".to_string(),
            lore: vec!["Click to visit this island".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_minecraft_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::Chat("/visit Friend".to_string()),
            MinecraftAction::ClickSlot(11)
        ]
    );
    assert!(!runtime.island_states.get(&account).unwrap().visit_pending);
}

#[tokio::test]
async fn island_visit_friend_disables_visit_on_guest_rejection() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        visit_friend: "Friend".to_string(),
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    {
        let island = runtime.island_states.get_mut(&account).unwrap();
        island.require_ready_for_market = true;
        island.ready = false;
    }
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.handle_island_scoreboard(&account, &["Your Island".to_string()]);
    client.push_event(MinecraftEvent::ChatMessage {
        text: r#"{"server":"mini1A","map":"Private Island"}"#.to_string(),
    });
    runtime.poll_minecraft_once().await.unwrap();
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Visit Friend".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 11,
            name: "barrier".to_string(),
            display_name: "Visit Island".to_string(),
            lore: vec!["Island disallows guests!".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_minecraft_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::Chat("/visit Friend".to_string()),
            MinecraftAction::CloseWindow,
            MinecraftAction::Chat("/hub".to_string())
        ]
    );
    let island = runtime.island_states.get(&account).unwrap();
    assert!(island.visit_friend.is_none());
    assert!(!island.visit_pending);
}

#[tokio::test]
async fn island_ready_schedules_delayed_startup_reconcile_once() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(
        config,
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    let island = runtime.island_states.get_mut(&account).unwrap();
    island.require_ready_for_market = true;
    island.ready = false;

    runtime.handle_island_scoreboard(&account, &["Your Island".to_string()]);
    runtime.handle_island_scoreboard(&account, &["Your Island".to_string()]);

    assert_eq!(runtime.deferred_queue_entries.len(), 1);
    runtime.deferred_queue_entries[0].ready_at = Instant::now() - Duration::from_millis(1);
    runtime.drain_deferred_queue_entries().await.unwrap();

    let records = runtime.dry_run_records();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].state,
        BotState::Custom("reconcileAuctions".to_string())
    );
    assert_eq!(records[0].priority, 2);
    assert_eq!(records[0].action["reason"], json!("startup"));
}

#[tokio::test]
async fn island_ready_requests_startup_profiles_scan() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(
        config,
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    let island = runtime.island_states.get_mut(&account).unwrap();
    island.require_ready_for_market = true;
    island.ready = false;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.handle_island_scoreboard(&account, &["Your Island".to_string()]);
    runtime.drive_startup_profile_scans_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![MinecraftAction::Chat("/profiles".to_string())]
    );
    assert_eq!(runtime.processed_queue_steps, 0);
    assert_eq!(runtime.deferred_queue_entries.len(), 1);
}

#[tokio::test]
async fn startup_profiles_window_updates_capacity_and_closes() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(
        config,
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    let island = runtime.island_states.get_mut(&account).unwrap();
    island.require_ready_for_market = true;
    island.ready = false;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.handle_island_scoreboard(&account, &["Your Island".to_string()]);
    assert!(
        !runtime
            .island_states
            .get(&account)
            .unwrap()
            .startup_ready_notified
    );
    runtime.drive_startup_profile_scans_once().await.unwrap();
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Profiles".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 11,
            name: "emerald_block".to_string(),
            display_name: "Selected Profile".to_string(),
            lore: vec!["Co-op with 2 players:".to_string()],
            item_uuid: None,
        }],
    }));

    runtime.poll_minecraft_once().await.unwrap();
    let stats = runtime.stats.stats(&account).await.unwrap();

    assert_eq!(stats.auction_slots_max, Some(20));
    assert!(
        !runtime
            .island_states
            .get(&account)
            .unwrap()
            .startup_ready_notified
    );
    assert!(
        runtime
            .island_states
            .get(&account)
            .unwrap()
            .startup_cookie_scan_due
            .is_some()
    );
    assert!(client.actions().contains(&MinecraftAction::CloseWindow));
    assert!(
        !runtime
            .active_windows
            .lock()
            .unwrap()
            .contains_key(&account)
    );

    runtime.drive_startup_cookie_scans_once().await.unwrap();
    assert!(
        client
            .actions()
            .contains(&MinecraftAction::Chat("/sbmenu".to_string()))
    );
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "SkyBlock Menu".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 13,
            name: "cookie".to_string(),
            display_name: "Booster Cookie".to_string(),
            lore: vec!["Duration: 1h".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_minecraft_once().await.unwrap();
    let stats = runtime.stats.stats(&account).await.unwrap();

    assert!(stats.cookie_expires_at.is_some());
    assert!(
        runtime
            .island_states
            .get(&account)
            .unwrap()
            .startup_ready_notified
    );
}

#[tokio::test]
async fn startup_cookie_scan_ignores_non_menu_skyblock_windows() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(
        config,
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    runtime.island_states.get_mut(&account).unwrap().ready = true;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.schedule_startup_cookie_scan(&account);
    runtime.drive_startup_cookie_scans_once().await.unwrap();
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "SkyBlock Leveling".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 13,
            name: "cookie".to_string(),
            display_name: "Booster Cookie".to_string(),
            lore: vec!["Duration: 1h".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_minecraft_once().await.unwrap();

    let island = runtime.island_states.get(&account).unwrap();
    assert!(island.startup_cookie_scan_requested);
    assert!(!island.startup_cookie_scan_complete);
    assert!(!island.startup_ready_notified);
    assert!(
        runtime
            .active_windows
            .lock()
            .unwrap()
            .contains_key(&account)
    );
    let stats = runtime.stats.stats(&account).await.unwrap();
    assert_eq!(stats.cookie_expires_at, None);
}

#[tokio::test]
async fn startup_cookie_scan_dry_run_records_auto_cookie_without_live_actions() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        auto_cookie: "2h".to_string(),
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(
        config,
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    runtime.cookie_prices = Arc::new(FixedCookiePriceProvider(Some(10_000_000.0)));
    runtime
        .stats
        .record_scoreboard(&account, &["Purse: 50,000,000".to_string()])
        .unwrap();
    runtime.island_states.get_mut(&account).unwrap().ready = true;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.schedule_startup_cookie_scan(&account);
    runtime.drive_startup_cookie_scans_once().await.unwrap();
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "SkyBlock Menu".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 13,
            name: "cookie".to_string(),
            display_name: "Booster Cookie".to_string(),
            lore: vec!["Duration: 30m".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_minecraft_once().await.unwrap();

    let records = runtime.dry_run_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].state, BotState::Custom("autoCookie".to_string()));
    assert_eq!(records[0].action["kind"], json!("autoCookie"));
    assert_eq!(records[0].action["price"], json!(10_000_000.0));
    assert!(
        !client
            .actions()
            .contains(&MinecraftAction::Chat("/bz booster cookie".to_string()))
    );
    assert!(
        runtime
            .island_states
            .get(&account)
            .unwrap()
            .startup_ready_notified
    );
}

#[tokio::test]
async fn startup_cookie_scan_live_buys_and_activates_low_cookie() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        auto_cookie: "2h".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.cookie_prices = Arc::new(FixedCookiePriceProvider(Some(10_000_000.0)));
    runtime
        .stats
        .record_scoreboard(&account, &["Purse: 50,000,000".to_string()])
        .unwrap();
    runtime.island_states.get_mut(&account).unwrap().ready = true;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());
    let inventory = MutableInventoryProvider {
        items: Arc::new(Mutex::new(vec![InventoryItem {
            uuid: None,
            item_name: "Booster Cookie".to_string(),
            lore: Vec::new(),
            price: None,
            tag: Some("BOOSTER_COOKIE".to_string()),
            slot: Some(10),
            in_hotbar: false,
        }])),
    };
    runtime.managed_minecraft.insert(
        account.clone(),
        Arc::new(ManagedMinecraftClient::from_bundle(
            account.clone(),
            MarketActionMode::Live,
            LiveMinecraftClientBundle {
                minecraft: client.clone(),
                inventory_provider: Some(Arc::new(inventory)),
            },
            None,
        )),
    );

    runtime.schedule_startup_cookie_scan(&account);
    runtime.drive_startup_cookie_scans_once().await.unwrap();
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "SkyBlock Menu".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 13,
            name: "cookie".to_string(),
            display_name: "Booster Cookie".to_string(),
            lore: vec!["Duration: 30m".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_minecraft_once().await.unwrap();
    assert!(
        runtime.pending_auto_cookies.contains_key(&account),
        "auto-cookie should gate startup readiness while buying"
    );
    assert!(
        !runtime
            .island_states
            .get(&account)
            .unwrap()
            .startup_ready_notified
    );
    assert!(
        client
            .actions()
            .contains(&MinecraftAction::Chat("/bz booster cookie".to_string()))
    );

    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Bazaar - Booster Cookie".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 11,
            name: "gold_nugget".to_string(),
            display_name: "Buy Instantly".to_string(),
            lore: vec!["Instant Buy".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_minecraft_once().await.unwrap();
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "How many do you want?".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 10,
            name: "cookie".to_string(),
            display_name: "Buy One".to_string(),
            lore: vec!["1x".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_minecraft_once().await.unwrap();
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Confirm Purchase".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 10,
            name: "lime_wool".to_string(),
            display_name: "Confirm".to_string(),
            lore: vec!["Click to buy now".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_minecraft_once().await.unwrap();
    if let Some(pending) = runtime.pending_auto_cookies.get_mut(&account)
        && let AutoCookiePhase::PreparingActivation { due_at } = &mut pending.phase
    {
        *due_at = Instant::now();
    }
    runtime.drive_auto_cookies_once().await.unwrap();
    assert!(
        client
            .actions()
            .contains(&MinecraftAction::SwapSlotToHotbar {
                slot: 10,
                hotbar_slot: 0
            })
    );
    assert!(
        client
            .actions()
            .contains(&MinecraftAction::SetHeldHotbarSlot(0))
    );
    assert!(
        client
            .actions()
            .contains(&MinecraftAction::ActivateHeldItem)
    );

    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Consume Booster Cookie".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 11,
            name: "cookie".to_string(),
            display_name: "Consume Booster Cookie".to_string(),
            lore: vec!["Confirm".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_minecraft_once().await.unwrap();
    assert!(!runtime.pending_auto_cookies.contains_key(&account));
    assert!(
        runtime
            .island_states
            .get(&account)
            .unwrap()
            .startup_ready_notified
    );
    let stats = runtime.stats.stats(&account).await.unwrap();
    assert!(stats.cookie_expires_at.is_some());
}

#[tokio::test]
async fn auto_cookie_timeout_closes_and_clears_cached_window() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        auto_cookie: "2h".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.cookie_prices = Arc::new(FixedCookiePriceProvider(Some(10_000_000.0)));
    runtime
        .stats
        .record_scoreboard(&account, &["Purse: 50,000,000".to_string()])
        .unwrap();
    runtime.island_states.get_mut(&account).unwrap().ready = true;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime
        .start_auto_cookie_if_needed(&account, Some(Duration::ZERO))
        .await
        .unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Bazaar - Booster Cookie".to_string(),
            slots: Vec::new(),
        },
    );
    runtime
        .pending_auto_cookies
        .get_mut(&account)
        .unwrap()
        .force_timeout_for_test();

    runtime.drive_auto_cookies_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::Chat("/bz booster cookie".to_string()),
            MinecraftAction::CloseWindow
        ]
    );
    assert!(!runtime.pending_auto_cookies.contains_key(&account));
    assert!(
        !runtime
            .active_windows
            .lock()
            .unwrap()
            .contains_key(&account)
    );
    assert!(
        runtime
            .island_states
            .get(&account)
            .unwrap()
            .startup_ready_notified
    );
}

#[tokio::test]
async fn startup_profile_scan_blocks_market_queue_until_closed() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let island = runtime.island_states.get_mut(&account).unwrap();
    island.require_ready_for_market = true;
    island.ready = false;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());
    runtime
        .queue
        .add(
            &account,
            json!({"auctionID": "auction-1"}),
            BotState::Buying,
            1,
        )
        .await
        .unwrap();

    runtime.handle_island_scoreboard(&account, &["Your Island".to_string()]);
    runtime.drive_startup_profile_scans_once().await.unwrap();
    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![MinecraftAction::Chat("/profiles".to_string())]
    );

    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Profiles".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 11,
            name: "emerald_block".to_string(),
            display_name: "Selected Profile".to_string(),
            lore: vec!["Co-op with 2 players:".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_market_queue_once().await.unwrap();

    assert!(client.actions().contains(&MinecraftAction::CloseWindow));
    assert_eq!(runtime.processed_queue_steps, 0);

    runtime.drive_startup_cookie_scans_once().await.unwrap();
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "SkyBlock Menu".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 13,
            name: "cookie".to_string(),
            display_name: "Booster Cookie".to_string(),
            lore: vec!["Duration: 2h".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_minecraft_once().await.unwrap();
    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(runtime.processed_queue_steps, 1);
}

#[tokio::test]
async fn startup_profile_scan_timeout_releases_market_queue() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let island = runtime.island_states.get_mut(&account).unwrap();
    island.require_ready_for_market = true;
    island.ready = false;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());
    runtime
        .queue
        .add(
            &account,
            json!({"auctionID": "auction-1"}),
            BotState::Buying,
            1,
        )
        .await
        .unwrap();

    runtime.handle_island_scoreboard(&account, &["Your Island".to_string()]);
    runtime.drive_startup_profile_scans_once().await.unwrap();
    runtime.process_market_queue_once().await.unwrap();
    assert_eq!(runtime.processed_queue_steps, 0);

    let expired_at = Instant::now()
        .checked_sub(STARTUP_PROFILE_SCAN_TIMEOUT + Duration::from_millis(1))
        .unwrap();
    runtime
        .island_states
        .get_mut(&account)
        .unwrap()
        .startup_profile_scan_started_at = Some(expired_at);
    runtime.expire_startup_profile_scans_once().await;
    runtime.process_market_queue_once().await.unwrap();

    let island = runtime.island_states.get(&account).unwrap();
    assert!(island.startup_profile_scan_complete);
    assert!(island.startup_cookie_scan_due.is_some());
    assert!(!island.startup_ready_notified);
    let stats = runtime.stats.stats(&account).await.unwrap();
    assert_eq!(stats.auction_slots_used, None);
    assert_eq!(stats.auction_slots_max, None);
    assert_eq!(runtime.processed_queue_steps, 0);

    runtime.drive_startup_cookie_scans_once().await.unwrap();
    let expired_cookie_at = Instant::now()
        .checked_sub(STARTUP_PROFILE_SCAN_TIMEOUT + Duration::from_millis(1))
        .unwrap();
    runtime
        .island_states
        .get_mut(&account)
        .unwrap()
        .startup_cookie_scan_started_at = Some(expired_cookie_at);
    runtime.expire_startup_cookie_scans_once().await;
    runtime.process_market_queue_once().await.unwrap();

    let island = runtime.island_states.get(&account).unwrap();
    assert!(island.startup_cookie_scan_complete);
    assert!(island.startup_ready_notified);
    assert_eq!(runtime.processed_queue_steps, 1);
}

#[tokio::test]
async fn startup_ready_notification_does_not_require_profile_scan_when_relist_is_off() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: false,
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(
        config,
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    let island = runtime.island_states.get_mut(&account).unwrap();
    island.require_ready_for_market = true;
    island.ready = false;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::Scoreboard {
        lines: vec!["Your Island".to_string()],
    });
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.poll_minecraft_once().await.unwrap();

    let island = runtime.island_states.get(&account).unwrap();
    assert!(island.startup_ready_notified);
    assert!(!island.startup_profile_scan_requested);
    assert!(runtime.deferred_queue_entries.is_empty());
    assert!(client.actions().is_empty());
}

#[test]
fn startup_ready_notification_body_renders_operator_fields() {
    let stats = AccountStats {
        purse: Some(25_500_000.0),
        cofl_tier: Some("premium".to_string()),
        cofl_expires_at: Some(1_900_000_000),
        cookie_expires_at: Some(1_900_100_000),
        auction_slots_used: Some(4),
        auction_slots_max: Some(20),
        ..AccountStats::default()
    };

    let body = startup_ready_notification_body(&stats, Some("cofl-connection-1"));

    assert!(body.contains("Auction Slots: `4/20`"));
    assert!(body.contains("Purse: `25.50m`"));
    assert!(body.contains("Cofl Tier: `premium`"));
    assert!(body.contains("Cofl expires: <t:1900000000:R>"));
    assert!(body.contains("Booster Cookie ends: <t:1900100000:R>"));
    assert!(body.contains("Connection ID: `cofl-connection-1`"));
    assert!(!body.contains("null"));
    assert!(!body.contains("undefined"));
}

#[tokio::test]
async fn startup_reconcile_does_not_duplicate_existing_reconcile_queue() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"reason": "manual"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(
        config,
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    let island = runtime.island_states.get_mut(&account).unwrap();
    island.require_ready_for_market = true;
    island.ready = false;

    runtime.handle_island_scoreboard(&account, &["Your Island".to_string()]);
    runtime.deferred_queue_entries[0].ready_at = Instant::now() - Duration::from_millis(1);
    runtime.drain_deferred_queue_entries().await.unwrap();

    assert!(runtime.dry_run_records().is_empty());
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );
}

#[tokio::test]
async fn island_bad_modification_backoff_suppresses_locraw() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime
        .island_states
        .get_mut(&account)
        .unwrap()
        .locraw_delay = Duration::ZERO;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::ChatMessage {
        text: "You were kicked for badly behaving modifications".to_string(),
    });
    client.push_event(MinecraftEvent::Ready {
        reason: "spawn".to_string(),
    });
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.poll_once().await.unwrap();

    assert!(client.actions().is_empty());
}

#[tokio::test]
async fn bad_modification_kick_reason_suppresses_locraw() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        ..SafConfig::default()
    };
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime
        .island_states
        .get_mut(&account)
        .unwrap()
        .locraw_delay = Duration::ZERO;
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::Kicked {
        reason: "You were kicked for badly behaving modifications".to_string(),
    });
    client.push_event(MinecraftEvent::Ready {
        reason: "spawn".to_string(),
    });
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());

    runtime.poll_once().await.unwrap();

    assert!(client.actions().is_empty());
}

#[tokio::test]
async fn auction_reconcile_poller_records_slot_pressure_for_waiting_listings() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "auctionID": "auction-1",
                "inv": "item-uuid",
                "price": 10_000_000
            }),
            BotState::Listing,
            4,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime
        .auction_reconcile_poller
        .as_mut()
        .unwrap()
        .states
        .get_mut(&account)
        .unwrap()
        .next_poll_at = Instant::now() - Duration::from_millis(1);

    runtime.queue_reconciliation_polls_once().await.unwrap();

    let records = runtime.dry_run_records();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].state,
        BotState::Custom("reconcileAuctions".to_string())
    );
    assert_eq!(records[0].priority, 0);
    assert_eq!(records[0].action["reason"], json!("slot-pressure"));
}

#[tokio::test]
async fn auction_reconcile_poller_records_slot_pressure_when_auctions_are_full() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime
        .remember_window_snapshot(
            account.clone(),
            WindowSnapshot {
                title: "Manage Auctions".to_string(),
                slots: vec![saf_core::gui::WindowSlot {
                    slot: 4,
                    name: "gold_ingot".to_string(),
                    display_name: "Auction Slots".to_string(),
                    lore: vec!["Active auctions: 14/14".to_string()],
                    item_uuid: None,
                }],
            },
        )
        .unwrap();
    runtime
        .auction_reconcile_poller
        .as_mut()
        .unwrap()
        .states
        .get_mut(&account)
        .unwrap()
        .next_poll_at = Instant::now() - Duration::from_millis(1);

    runtime.queue_reconciliation_polls_once().await.unwrap();

    let records = runtime.dry_run_records();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].state,
        BotState::Custom("reconcileAuctions".to_string())
    );
    assert_eq!(records[0].priority, 0);
    assert_eq!(records[0].action["reason"], json!("slot-pressure"));
}

#[tokio::test]
async fn auction_reconcile_poller_queue_errors_do_not_stop_runtime() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let bad_state_base = temp.path().join("state-root-is-file");
    std::fs::write(&bad_state_base, b"not a directory").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), &bad_state_base);
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let poll_state = runtime
        .auction_reconcile_poller
        .as_mut()
        .unwrap()
        .states
        .get_mut(&account)
        .unwrap();
    poll_state.next_poll_at = Instant::now() - Duration::from_millis(1);
    let last_reconcile_at = poll_state.last_reconcile_at;

    runtime.queue_reconciliation_polls_once().await.unwrap();

    let poll_state = runtime
        .auction_reconcile_poller
        .as_ref()
        .unwrap()
        .states
        .get(&account)
        .unwrap();
    assert_eq!(poll_state.last_reconcile_at, last_reconcile_at);
}

#[tokio::test]
async fn auction_reconcile_poller_does_not_duplicate_existing_reconcile() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let store = FileQueueStore::new(temp.path());
    store
        .add(
            &account,
            json!({
                "auctionID": "auction-1",
                "inv": "item-uuid",
                "price": 10_000_000
            }),
            BotState::Listing,
            4,
        )
        .await
        .unwrap();
    store
        .add(
            &account,
            json!({"reason": "manual"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime
        .auction_reconcile_poller
        .as_mut()
        .unwrap()
        .states
        .get_mut(&account)
        .unwrap()
        .next_poll_at = Instant::now() - Duration::from_millis(1);

    runtime.queue_reconciliation_polls_once().await.unwrap();

    assert!(runtime.dry_run_records().is_empty());
}

#[tokio::test]
async fn auction_reconcile_poller_records_regular_idle_reconcile() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        use_cookie: true,
        relist: true,
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let now = Instant::now();
    let poller = runtime.auction_reconcile_poller.as_mut().unwrap();
    let state = poller.states.get_mut(&account).unwrap();
    state.next_poll_at = now - Duration::from_millis(1);
    state.last_reconcile_at = now - Duration::from_millis(DEFAULT_IDLE_AUCTION_RECONCILE_MS + 1);

    runtime.queue_reconciliation_polls_once().await.unwrap();

    let records = runtime.dry_run_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].action["reason"], json!("regular-poll"));
}
