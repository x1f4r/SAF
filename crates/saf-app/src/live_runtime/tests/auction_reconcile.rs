use super::*;
#[test]
fn active_auction_summaries_parse_manage_window() {
    let window = WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 10,
                name: "diamond_sword".to_string(),
                display_name: "Sharp Sword".to_string(),
                lore: vec![
                    "Seller: Main".to_string(),
                    "Auction ID: aaaaaaa1-bbbb-cccc-dddd-eeeeeeeeeeee".to_string(),
                ],
                item_uuid: Some("item-uuid-1".to_string()),
            },
            saf_core::gui::WindowSlot {
                slot: 11,
                name: "gold_ingot".to_string(),
                display_name: "Sold Item".to_string(),
                lore: vec!["Buyer: Someone".to_string(), "Seller: Main".to_string()],
                item_uuid: Some("item-uuid-2".to_string()),
            },
        ],
    };

    assert_eq!(
        active_auction_summaries(&window),
        vec![ActiveAuction {
            auction_id: "aaaaaaa1-bbbb-cccc-dddd-eeeeeeeeeeee".to_string(),
            item_uuid: "item-uuid-1".to_string(),
            name: Some("Sharp Sword".to_string())
        }]
    );
}

#[tokio::test]
async fn active_auction_scan_clears_closed_window_when_stats_recording_fails() {
    let account = AccountId::new("Main").unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Auction House".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 15,
            name: "gold_block".to_string(),
            display_name: "Manage Auctions".to_string(),
            lore: vec!["View your auctions".to_string()],
            item_uuid: None,
        }],
    }));
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 10,
            name: "diamond_sword".to_string(),
            display_name: "Sharp Sword".to_string(),
            lore: vec![
                "Seller: Main".to_string(),
                "Auction ID: aaaaaaa1-bbbb-cccc-dddd-eeeeeeeeeeee".to_string(),
            ],
            item_uuid: Some("item-uuid-1".to_string()),
        }],
    }));
    let active_windows = Arc::new(Mutex::new(BTreeMap::new()));
    let provider = LiveActiveAuctionProvider {
        clients: BTreeMap::from([(account.clone(), client.clone() as Arc<dyn MinecraftClient>)]),
        active_windows: active_windows.clone(),
        deferred_events: Arc::new(Mutex::new(BTreeMap::new())),
        stats: Arc::new(LiveStatsProvider::new(Vec::new())),
        mode: MarketActionMode::Live,
        wait_timeout: Duration::from_millis(50),
    };

    let auctions = provider.active_auctions(&account).await.unwrap();

    assert_eq!(
        auctions,
        vec![ActiveAuction {
            auction_id: "aaaaaaa1-bbbb-cccc-dddd-eeeeeeeeeeee".to_string(),
            item_uuid: "item-uuid-1".to_string(),
            name: Some("Sharp Sword".to_string())
        }]
    );
    assert!(active_windows.lock().unwrap().is_empty());
    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::Chat("/ah".to_string()),
            MinecraftAction::ClickSlot(15),
            MinecraftAction::CloseWindow,
        ]
    );
}

#[tokio::test]
async fn active_auction_cached_manage_window_is_read_without_closing() {
    let account = AccountId::new("Main").unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    let active_windows = Arc::new(Mutex::new(BTreeMap::from([(
        account.clone(),
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 10,
                name: "diamond_sword".to_string(),
                display_name: "Sharp Sword".to_string(),
                lore: vec![
                    "Seller: Main".to_string(),
                    "Auction ID: aaaaaaa1-bbbb-cccc-dddd-eeeeeeeeeeee".to_string(),
                ],
                item_uuid: Some("item-uuid-1".to_string()),
            }],
        },
    )])));
    let provider = LiveActiveAuctionProvider {
        clients: BTreeMap::from([(account.clone(), client.clone() as Arc<dyn MinecraftClient>)]),
        active_windows: active_windows.clone(),
        deferred_events: Arc::new(Mutex::new(BTreeMap::new())),
        stats: Arc::new(LiveStatsProvider::new(vec![account.clone()])),
        mode: MarketActionMode::Live,
        wait_timeout: Duration::from_millis(50),
    };

    let auctions = provider.active_auctions(&account).await.unwrap();

    assert_eq!(
        auctions,
        vec![ActiveAuction {
            auction_id: "aaaaaaa1-bbbb-cccc-dddd-eeeeeeeeeeee".to_string(),
            item_uuid: "item-uuid-1".to_string(),
            name: Some("Sharp Sword".to_string())
        }]
    );
    assert_eq!(client.actions(), Vec::<MinecraftAction>::new());
    assert_eq!(
        active_windows
            .lock()
            .unwrap()
            .get(&account)
            .map(|window| window.title.as_str()),
        Some("Manage Auctions")
    );
}

#[tokio::test]
async fn active_auction_scan_returns_empty_when_auction_house_has_no_manage_slot() {
    let account = AccountId::new("Main").unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Co-op Auction House".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 15,
            name: "gold_block".to_string(),
            display_name: "Create Auction".to_string(),
            lore: vec!["Start a new auction".to_string()],
            item_uuid: None,
        }],
    }));
    let active_windows = Arc::new(Mutex::new(BTreeMap::new()));
    let stats = Arc::new(LiveStatsProvider::new(vec![account.clone()]));
    let provider = LiveActiveAuctionProvider {
        clients: BTreeMap::from([(account.clone(), client.clone() as Arc<dyn MinecraftClient>)]),
        active_windows: active_windows.clone(),
        deferred_events: Arc::new(Mutex::new(BTreeMap::new())),
        stats: stats.clone(),
        mode: MarketActionMode::Live,
        wait_timeout: Duration::from_millis(50),
    };

    let auctions = provider.active_auctions(&account).await.unwrap();
    let live_stats = stats.stats(&account).await.unwrap();

    assert!(auctions.is_empty());
    assert!(active_windows.lock().unwrap().is_empty());
    assert_eq!(live_stats.auction_slots_used, Some(0));
    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::Chat("/ah".to_string()),
            MinecraftAction::CloseWindow,
        ]
    );
}

#[tokio::test]
async fn active_auction_dry_run_requires_cached_manage_window() {
    let account = AccountId::new("Main").unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    let provider = LiveActiveAuctionProvider {
        clients: BTreeMap::from([(account.clone(), client.clone() as Arc<dyn MinecraftClient>)]),
        active_windows: Arc::new(Mutex::new(BTreeMap::new())),
        deferred_events: Arc::new(Mutex::new(BTreeMap::new())),
        stats: Arc::new(LiveStatsProvider::new(vec![account.clone()])),
        mode: MarketActionMode::DryRun,
        wait_timeout: Duration::from_millis(50),
    };

    let error = provider.active_auctions(&account).await.unwrap_err();

    assert!(
        matches!(error, PortError::Unavailable(message) if message.contains("cached Manage Auctions"))
    );
    assert_eq!(client.actions(), Vec::<MinecraftAction>::new());
}

#[tokio::test]
async fn gui_diagnostics_reports_disconnect_during_window_wait() {
    let account = AccountId::new("Main").unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::Disconnected {
        reason: "socket closed".to_string(),
    });
    let deferred_events = Arc::new(Mutex::new(BTreeMap::new()));
    let provider = LiveActiveAuctionProvider {
        clients: BTreeMap::from([(account.clone(), client.clone() as Arc<dyn MinecraftClient>)]),
        active_windows: Arc::new(Mutex::new(BTreeMap::new())),
        deferred_events: deferred_events.clone(),
        stats: Arc::new(LiveStatsProvider::new(vec![account.clone()])),
        mode: MarketActionMode::Live,
        wait_timeout: Duration::from_millis(50),
    };

    let error = provider
        .diagnose_slots(&account, Some("ah"))
        .await
        .unwrap_err();

    assert!(
        matches!(error, PortError::Unavailable(message) if message.contains("disconnected while diagnosing GUI slots"))
    );
    assert_eq!(
        client.actions(),
        vec![MinecraftAction::Chat("/ah".to_string())]
    );
    assert!(matches!(
        pop_deferred_minecraft_event(&deferred_events, &account).unwrap(),
        Some(MinecraftEvent::Disconnected { reason }) if reason == "socket closed"
    ));
}

#[tokio::test]
async fn active_auction_scan_records_scoreboard_events_while_waiting() {
    let account = AccountId::new("Main").unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::Scoreboard {
        lines: vec!["Purse: 1.2M".to_string()],
    });
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Auction House".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 15,
            name: "gold_block".to_string(),
            display_name: "Manage Auctions".to_string(),
            lore: vec!["View your auctions".to_string()],
            item_uuid: None,
        }],
    }));
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: Vec::new(),
    }));
    let active_windows = Arc::new(Mutex::new(BTreeMap::new()));
    let stats = Arc::new(LiveStatsProvider::new(vec![account.clone()]));
    let provider = LiveActiveAuctionProvider {
        clients: BTreeMap::from([(account.clone(), client.clone() as Arc<dyn MinecraftClient>)]),
        active_windows,
        deferred_events: Arc::new(Mutex::new(BTreeMap::new())),
        stats: stats.clone(),
        mode: MarketActionMode::Live,
        wait_timeout: Duration::from_millis(50),
    };

    let auctions = provider.active_auctions(&account).await.unwrap();

    assert!(auctions.is_empty());
    assert_eq!(
        stats.stats(&account).await.unwrap().purse,
        Some(1_200_000.0)
    );
}

#[tokio::test]
async fn active_auction_scan_replays_chat_events_to_runtime_polling() {
    let account = AccountId::new("Main").unwrap();
    let temp = tempfile::tempdir().unwrap();
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        },
        options,
    )
    .await
    .unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::ChatMessage {
        text: "§aYour Ping - 1,234ms".to_string(),
    });
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Auction House".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 15,
            name: "gold_block".to_string(),
            display_name: "Manage Auctions".to_string(),
            lore: vec!["View your auctions".to_string()],
            item_uuid: None,
        }],
    }));
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: Vec::new(),
    }));
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());
    let provider = LiveActiveAuctionProvider {
        clients: BTreeMap::from([(account.clone(), client.clone() as Arc<dyn MinecraftClient>)]),
        active_windows: runtime.active_windows.clone(),
        deferred_events: runtime.deferred_minecraft_events.clone(),
        stats: runtime.stats.clone(),
        mode: MarketActionMode::Live,
        wait_timeout: Duration::from_millis(50),
    };

    let auctions = provider.active_auctions(&account).await.unwrap();
    assert!(auctions.is_empty());
    assert_eq!(
        runtime.stats.ping(&account).await.unwrap().hypixel_ping_ms,
        None
    );

    runtime.poll_minecraft_once().await.unwrap();

    assert_eq!(
        runtime.stats.ping(&account).await.unwrap().hypixel_ping_ms,
        Some(1_234)
    );
    assert!(runtime.deferred_minecraft_events.lock().unwrap().is_empty());
}

#[tokio::test]
async fn gui_diagnostics_close_opened_windows_after_snapshot() {
    let account = AccountId::new("Main").unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Bank".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 11,
            name: "emerald".to_string(),
            display_name: "Co-op Bank Account".to_string(),
            lore: vec!["Deposit or withdraw coins".to_string()],
            item_uuid: None,
        }],
    }));
    let active_windows = Arc::new(Mutex::new(BTreeMap::new()));
    let provider = LiveActiveAuctionProvider {
        clients: BTreeMap::from([(account.clone(), client.clone() as Arc<dyn MinecraftClient>)]),
        active_windows: active_windows.clone(),
        deferred_events: Arc::new(Mutex::new(BTreeMap::new())),
        stats: Arc::new(LiveStatsProvider::new(vec![account.clone()])),
        mode: MarketActionMode::Live,
        wait_timeout: Duration::from_millis(50),
    };

    let diagnostics = provider
        .diagnose_slots(&account, Some("bank"))
        .await
        .unwrap();

    assert_eq!(diagnostics.windows.len(), 1);
    assert_eq!(diagnostics.windows[0].title, "Bank");
    assert!(active_windows.lock().unwrap().is_empty());
    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::Chat("/bank".to_string()),
            MinecraftAction::CloseWindow,
        ]
    );
}

#[tokio::test]
async fn gui_diagnostics_manage_target_opens_manage_auctions() {
    let account = AccountId::new("Main").unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Auction House".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 15,
            name: "gold_block".to_string(),
            display_name: "Manage Auctions".to_string(),
            lore: vec!["View your auctions".to_string()],
            item_uuid: None,
        }],
    }));
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 10,
            name: "diamond_sword".to_string(),
            display_name: "Sharp Sword".to_string(),
            lore: vec!["Seller: Main".to_string()],
            item_uuid: Some("item-uuid-1".to_string()),
        }],
    }));
    let active_windows = Arc::new(Mutex::new(BTreeMap::new()));
    let provider = LiveActiveAuctionProvider {
        clients: BTreeMap::from([(account.clone(), client.clone() as Arc<dyn MinecraftClient>)]),
        active_windows: active_windows.clone(),
        deferred_events: Arc::new(Mutex::new(BTreeMap::new())),
        stats: Arc::new(LiveStatsProvider::new(vec![account.clone()])),
        mode: MarketActionMode::Live,
        wait_timeout: Duration::from_millis(50),
    };

    let diagnostics = provider
        .diagnose_slots(&account, Some("manage"))
        .await
        .unwrap();

    assert_eq!(diagnostics.windows.len(), 2);
    assert_eq!(diagnostics.windows[0].title, "Auction House");
    assert_eq!(diagnostics.windows[1].title, "Manage Auctions");
    assert!(active_windows.lock().unwrap().is_empty());
    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::Chat("/ah".to_string()),
            MinecraftAction::ClickSlot(15),
            MinecraftAction::CloseWindow,
        ]
    );
}

#[tokio::test]
async fn live_stats_records_manage_auction_slot_capacity() {
    let account = AccountId::new("Main").unwrap();
    let provider = LiveStatsProvider::new(vec![account.clone()]);
    let window = WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 4,
                name: "gold_ingot".to_string(),
                display_name: "Auction Slots".to_string(),
                lore: vec!["Active auctions: 1 / 14".to_string()],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 10,
                name: "diamond_sword".to_string(),
                display_name: "Sharp Sword".to_string(),
                lore: vec![
                    "Seller: Main".to_string(),
                    "Auction ID: aaaaaaa1-bbbb-cccc-dddd-eeeeeeeeeeee".to_string(),
                ],
                item_uuid: Some("item-uuid-1".to_string()),
            },
            saf_core::gui::WindowSlot {
                slot: 11,
                name: "gold_ingot".to_string(),
                display_name: "Sold Item".to_string(),
                lore: vec!["Buyer: Someone".to_string(), "Seller: Main".to_string()],
                item_uuid: Some("item-uuid-2".to_string()),
            },
        ],
    };

    provider
        .record_window_snapshot(&account, &window)
        .expect("manage auction window should update stats");
    let stats = provider.stats(&account).await.unwrap();

    assert_eq!(stats.auction_slots_used, Some(1));
    assert_eq!(stats.auction_slots_max, Some(14));
}

#[tokio::test]
async fn live_stats_records_profile_coop_auction_slot_capacity() {
    let account = AccountId::new("Main").unwrap();
    let provider = LiveStatsProvider::new(vec![account.clone()]);
    let window = WindowSnapshot {
        title: "Profiles".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 11,
            name: "emerald_block".to_string(),
            display_name: "Selected Profile".to_string(),
            lore: vec!["Co-op with 2 players:".to_string()],
            item_uuid: None,
        }],
    };

    provider
        .record_window_snapshot(&account, &window)
        .expect("profiles window should update max auction slots");
    let stats = provider.stats(&account).await.unwrap();

    assert_eq!(stats.auction_slots_used, None);
    assert_eq!(stats.auction_slots_max, Some(20));
}

#[tokio::test]
async fn configured_auction_slot_override_sets_account_capacity() {
    let account = AccountId::new("Main").unwrap();
    let provider = LiveStatsProvider::new(vec![account.clone()]);

    provider
        .apply_auction_slot_max_overrides(|key| {
            (key == "SAF_AUCTION_SLOTS_MAX_MAIN").then(|| "20".to_string())
        })
        .unwrap();

    let stats = provider.stats(&account).await.unwrap();
    assert_eq!(stats.auction_slots_used, None);
    assert_eq!(stats.auction_slots_max, Some(20));
}

#[tokio::test]
async fn manage_window_preserves_profile_max_when_capacity_is_missing() {
    let account = AccountId::new("Main").unwrap();
    let provider = LiveStatsProvider::new(vec![account.clone()]);
    provider
        .record_window_snapshot(
            &account,
            &WindowSnapshot {
                title: "Profiles".to_string(),
                slots: vec![saf_core::gui::WindowSlot {
                    slot: 11,
                    name: "emerald_block".to_string(),
                    display_name: "Selected Profile".to_string(),
                    lore: vec!["Co-op with [MVP+] OtherPlayer".to_string()],
                    item_uuid: None,
                }],
            },
        )
        .unwrap();
    provider
        .record_window_snapshot(
            &account,
            &WindowSnapshot {
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
    let stats = provider.stats(&account).await.unwrap();

    assert_eq!(stats.auction_slots_used, Some(1));
    assert_eq!(stats.auction_slots_max, Some(17));
}

#[tokio::test]
async fn live_stats_increment_listing_slots_without_exceeding_known_max() {
    let account = AccountId::new("Main").unwrap();
    let provider = LiveStatsProvider::new(vec![account.clone()]);
    provider
        .record_window_snapshot(
            &account,
            &WindowSnapshot {
                title: "Manage Auctions".to_string(),
                slots: vec![saf_core::gui::WindowSlot {
                    slot: 4,
                    name: "gold_ingot".to_string(),
                    display_name: "Auction Slots".to_string(),
                    lore: vec!["Active auctions: 13/14".to_string()],
                    item_uuid: None,
                }],
            },
        )
        .unwrap();

    provider.increment_auction_slots_used(&account).unwrap();
    provider.increment_auction_slots_used(&account).unwrap();
    let stats = provider.stats(&account).await.unwrap();

    assert_eq!(stats.auction_slots_used, Some(14));
    assert_eq!(stats.auction_slots_max, Some(14));
}

#[test]
fn auction_slot_stats_fall_back_to_active_seller_count() {
    let window = WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 10,
                name: "diamond_sword".to_string(),
                display_name: "Sharp Sword".to_string(),
                lore: vec!["Seller: Main".to_string()],
                item_uuid: Some("item-uuid-1".to_string()),
            },
            saf_core::gui::WindowSlot {
                slot: 11,
                name: "diamond_sword".to_string(),
                display_name: "Second Sword".to_string(),
                lore: vec!["Seller: Main".to_string()],
                item_uuid: Some("item-uuid-2".to_string()),
            },
            saf_core::gui::WindowSlot {
                slot: 12,
                name: "paper".to_string(),
                display_name: "Auction Duration".to_string(),
                lore: vec!["Duration: 1 / 2 days".to_string()],
                item_uuid: None,
            },
        ],
    };

    assert_eq!(
        auction_slot_stats_from_window(&window),
        Some(AuctionSlotStats {
            used: Some(2),
            max: None
        })
    );
}

#[test]
fn auction_slot_stats_count_active_seller_slots_without_item_uuids() {
    let window = WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 10,
                name: "diamond_sword".to_string(),
                display_name: "Sharp Sword".to_string(),
                lore: vec!["Seller: Main".to_string()],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 11,
                name: "gold_ingot".to_string(),
                display_name: "Sold Item".to_string(),
                lore: vec!["Seller: Main".to_string(), "Buyer: Someone".to_string()],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 12,
                name: "diamond_sword".to_string(),
                display_name: "Second Sword".to_string(),
                lore: vec!["Seller: Main".to_string()],
                item_uuid: None,
            },
        ],
    };

    assert_eq!(
        auction_slot_stats_from_window(&window),
        Some(AuctionSlotStats {
            used: Some(2),
            max: None
        })
    );
}

#[test]
fn auction_slot_stats_treat_auction_house_without_manage_slot_as_empty() {
    let window = WindowSnapshot {
        title: "Co-op Auction House".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 15,
            name: "gold_block".to_string(),
            display_name: "Create Auction".to_string(),
            lore: vec!["Start a new auction".to_string()],
            item_uuid: None,
        }],
    };

    assert_eq!(
        auction_slot_stats_from_window(&window),
        Some(AuctionSlotStats {
            used: Some(0),
            max: None
        })
    );
}

#[test]
fn reconcile_auction_window_prefers_claim_all_and_extracts_expired_relist_data() {
    let account = AccountId::new("Main").unwrap();
    let window = WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 29,
                name: "cauldron".to_string(),
                display_name: "Claim All".to_string(),
                lore: vec!["Collect all sold and expired auctions".to_string()],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 10,
                name: "diamond_sword".to_string(),
                display_name: "Expired Sword".to_string(),
                lore: vec![
                    "Seller: Main".to_string(),
                    "Expired!".to_string(),
                    "Buy it now: 10,000 coins".to_string(),
                    "SkyBlock ID: EXPIRED_SWORD".to_string(),
                ],
                item_uuid: Some("expired-item-uuid".to_string()),
            },
            saf_core::gui::WindowSlot {
                slot: 11,
                name: "gold_ingot".to_string(),
                display_name: "Sold Item".to_string(),
                lore: vec!["Seller: Main".to_string(), "Buyer: Someone".to_string()],
                item_uuid: Some("sold-item-uuid".to_string()),
            },
        ],
    };

    let action = reconcile_auction_window(&account, &window, false).unwrap();

    assert_eq!(action.claim_slot, 29);
    assert_eq!(action.sold_count, 1);
    assert_eq!(action.expired_count, 1);
    assert_eq!(action.claimed_expired.len(), 1);
    assert_eq!(
        action.claimed_expired[0].item_uuid.as_str(),
        "expired-item-uuid"
    );
    assert_eq!(action.claimed_expired[0].old_price, 10_000.0);
    assert_eq!(
        action.claimed_expired[0].tag.as_deref(),
        Some("EXPIRED_SWORD")
    );
}

#[test]
fn reconcile_ignores_buyer_lore_without_sold_claim_signal() {
    let account = AccountId::new("Main").unwrap();
    let window = WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 10,
            name: "diamond_sword".to_string(),
            display_name: "Sharp Sword".to_string(),
            lore: vec![
                "Seller: Main".to_string(),
                "Buyer: Someone".to_string(),
                "Auction ID: aaaaaaa1-bbbb-cccc-dddd-eeeeeeeeeeee".to_string(),
            ],
            item_uuid: Some("item-uuid-1".to_string()),
        }],
    };

    assert!(reconcile_auction_window(&account, &window, false).is_none());
}

#[tokio::test]
async fn dry_run_reconcile_records_claim_slot_without_mutating_queue() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"reason": "manual-discord"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(
        config,
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: vec![
                saf_core::gui::WindowSlot {
                    slot: 29,
                    name: "cauldron".to_string(),
                    display_name: "Claim All".to_string(),
                    lore: vec!["Collect all sold and expired auctions".to_string()],
                    item_uuid: None,
                },
                saf_core::gui::WindowSlot {
                    slot: 11,
                    name: "gold_ingot".to_string(),
                    display_name: "Sold Item".to_string(),
                    lore: vec!["Seller: Main".to_string(), "Buyer: Someone".to_string()],
                    item_uuid: Some("sold-item-uuid".to_string()),
                },
            ],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    let records = runtime.dry_run_records();
    assert_eq!(records.len(), 1);
    assert_eq!(
        records[0].action["instruction"],
        json!({"type": "clickSlot", "slot": 29})
    );
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );
    assert!(runtime.deferred_queue_entries.is_empty());
}

#[tokio::test]
async fn live_reconcile_action_errors_leave_entry_queued_for_retry() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"reason": "manual-discord"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
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
    let attempts = install_failing_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: vec![
                saf_core::gui::WindowSlot {
                    slot: 29,
                    name: "cauldron".to_string(),
                    display_name: "Claim All".to_string(),
                    lore: vec!["Collect all sold and expired auctions".to_string()],
                    item_uuid: None,
                },
                saf_core::gui::WindowSlot {
                    slot: 11,
                    name: "gold_ingot".to_string(),
                    display_name: "Sold Item".to_string(),
                    lore: vec!["Seller: Main".to_string(), "Buyer: Someone".to_string()],
                    item_uuid: Some("sold-item-uuid".to_string()),
                },
            ],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(*attempts.lock().unwrap(), 1);
    assert_eq!(runtime.report().completed_queue_entries, 0);
    assert_eq!(runtime.report().processed_queue_steps, 0);
    assert!(runtime.pending_market_steps.contains_key(&account));
    assert!(
        runtime
            .active_windows
            .lock()
            .unwrap()
            .contains_key(&account)
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
async fn live_reconcile_defers_expired_relist_queue_entry() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"reason": "regular-poll"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
        )
        .await
        .unwrap();
    let mut config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    config.do_not_relist.relist_mode = "2:97".to_string();
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: vec![
                saf_core::gui::WindowSlot {
                    slot: 29,
                    name: "cauldron".to_string(),
                    display_name: "Claim All".to_string(),
                    lore: vec!["Collect all sold and expired auctions".to_string()],
                    item_uuid: None,
                },
                saf_core::gui::WindowSlot {
                    slot: 10,
                    name: "diamond_sword".to_string(),
                    display_name: "Expired Sword".to_string(),
                    lore: vec![
                        "Seller: Main".to_string(),
                        "Expired!".to_string(),
                        "Buy it now: 10,000 coins".to_string(),
                        "SkyBlock ID: EXPIRED_SWORD".to_string(),
                    ],
                    item_uuid: Some("expired-item-uuid".to_string()),
                },
            ],
        },
    );

    runtime.process_market_queue_once().await.unwrap();
    assert_eq!(runtime.report().completed_queue_entries, 1);
    assert_eq!(runtime.deferred_queue_entries.len(), 1);
    runtime.deferred_queue_entries[0].ready_at = Instant::now() - Duration::from_secs(1);
    runtime.drain_deferred_queue_entries().await.unwrap();

    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::ListingNoName);
    assert_eq!(snapshot.queue[0].action["price"], json!(9700));
    assert_eq!(snapshot.queue[0].action["inv"], json!("expired-item-uuid"));
    assert_eq!(snapshot.queue[0].action["tag"], json!("EXPIRED_SWORD"));
}

#[tokio::test]
async fn expired_relist_queueing_dedupes_pending_and_persisted_entries() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let mut config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    config.do_not_relist.relist_mode = "2:97".to_string();
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let window = WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 29,
                name: "cauldron".to_string(),
                display_name: "Claim All".to_string(),
                lore: vec!["Collect all sold and expired auctions".to_string()],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 10,
                name: "diamond_sword".to_string(),
                display_name: "Expired Sword".to_string(),
                lore: vec![
                    "Seller: Main".to_string(),
                    "Expired!".to_string(),
                    "Buy it now: 10,000 coins".to_string(),
                    "SkyBlock ID: EXPIRED_SWORD".to_string(),
                ],
                item_uuid: Some("expired-item-uuid".to_string()),
            },
        ],
    };
    let store = FileQueueStore::new(temp.path());

    for reason in ["first", "second"] {
        store
            .add(
                &account,
                json!({"reason": reason}),
                BotState::Custom("reconcileAuctions".to_string()),
                2,
            )
            .await
            .unwrap();
        runtime
            .active_windows
            .lock()
            .unwrap()
            .insert(account.clone(), window.clone());
        runtime.process_market_queue_once().await.unwrap();
    }
    assert_eq!(runtime.deferred_queue_entries.len(), 1);

    runtime.deferred_queue_entries[0].ready_at = Instant::now() - Duration::from_secs(1);
    runtime.drain_deferred_queue_entries().await.unwrap();
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::ListingNoName);

    store
        .add(
            &account,
            json!({"reason": "third"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
        )
        .await
        .unwrap();
    runtime
        .active_windows
        .lock()
        .unwrap()
        .insert(account.clone(), window);
    runtime.process_market_queue_once().await.unwrap();
    assert!(runtime.deferred_queue_entries.is_empty());
    runtime.drain_deferred_queue_entries().await.unwrap();

    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::ListingNoName);
    assert_eq!(snapshot.queue[0].action["inv"], json!("expired-item-uuid"));
}

#[tokio::test]
async fn deferred_queue_write_errors_retry_without_dropping_entry() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let bad_state_base = temp.path().join("state-root-is-file");
    std::fs::write(&bad_state_base, b"not a directory").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), &bad_state_base);
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.deferred_queue_entries.push(DeferredQueueEntry {
        account: account.clone(),
        action: json!({"price": 9700, "inv": "expired-item-uuid"}),
        state: BotState::ListingNoName,
        priority: 4,
        ready_at: Instant::now() - Duration::from_secs(1),
    });

    runtime.drain_deferred_queue_entries().await.unwrap();

    assert_eq!(runtime.deferred_queue_entries.len(), 1);
    assert_eq!(runtime.deferred_queue_entries[0].account, account);
    assert_eq!(
        runtime.deferred_queue_entries[0].state,
        BotState::ListingNoName
    );
    assert!(runtime.deferred_queue_entries[0].ready_at > Instant::now());
}

#[tokio::test]
async fn live_claim_sold_queue_uses_reconcile_relist_path() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"reason": "legacy-claim-sold"}),
            BotState::Custom("claimSold".to_string()),
            2,
        )
        .await
        .unwrap();
    let mut config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    config.do_not_relist.relist_mode = "2:97".to_string();
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: vec![
                saf_core::gui::WindowSlot {
                    slot: 29,
                    name: "cauldron".to_string(),
                    display_name: "Claim All".to_string(),
                    lore: vec!["Collect all sold and expired auctions".to_string()],
                    item_uuid: None,
                },
                saf_core::gui::WindowSlot {
                    slot: 10,
                    name: "diamond_sword".to_string(),
                    display_name: "Expired Sword".to_string(),
                    lore: vec![
                        "Seller: Main".to_string(),
                        "Expired!".to_string(),
                        "Buy it now: 10,000 coins".to_string(),
                        "SkyBlock ID: EXPIRED_SWORD".to_string(),
                    ],
                    item_uuid: Some("expired-item-uuid".to_string()),
                },
            ],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(runtime.report().completed_queue_entries, 1);
    assert_eq!(runtime.deferred_queue_entries.len(), 1);
    assert_eq!(
        runtime.deferred_queue_entries[0].state,
        BotState::ListingNoName
    );
}

#[tokio::test]
async fn dry_run_expired_queue_records_matching_manage_slot() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(&account, json!("expired-item-uuid"), BotState::Expired, 4)
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(
        config,
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account,
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 10,
                name: "diamond_sword".to_string(),
                display_name: "Expired Sword".to_string(),
                lore: vec![
                    "Seller: Main".to_string(),
                    "Expired!".to_string(),
                    "Buy it now: 10,000 coins".to_string(),
                ],
                item_uuid: Some("expired-item-uuid".to_string()),
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        runtime.dry_run_records()[0].action["instruction"],
        json!({"type": "clickSlot", "slot": 10})
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
async fn live_expired_queue_claims_and_defers_relist() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(&account, json!("expired-item-uuid"), BotState::Expired, 4)
        .await
        .unwrap();
    let mut config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    config.do_not_relist.relist_mode = "2:97".to_string();
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 10,
                name: "diamond_sword".to_string(),
                display_name: "Expired Sword".to_string(),
                lore: vec![
                    "Seller: Main".to_string(),
                    "Expired!".to_string(),
                    "Buy it now: 10,000 coins".to_string(),
                    "SkyBlock ID: EXPIRED_SWORD".to_string(),
                ],
                item_uuid: Some("expired-item-uuid".to_string()),
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();
    assert_eq!(runtime.report().completed_queue_entries, 1);
    assert_eq!(runtime.deferred_queue_entries.len(), 1);
    runtime.deferred_queue_entries[0].ready_at = Instant::now() - Duration::from_secs(1);
    runtime.drain_deferred_queue_entries().await.unwrap();

    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::ListingNoName);
    assert_eq!(snapshot.queue[0].action["price"], json!(9700));
    assert_eq!(snapshot.queue[0].action["inv"], json!("expired-item-uuid"));
}

#[test]
fn transfer_followup_reads_source_bank_metadata() {
    let entry = QueueEntry {
        action: json!({
            "amount": 12_345_678,
            "withdraw": false,
            "personal": false,
            "transfer": {
                "to": "Alt",
                "stopSource": true
            }
        }),
        state: BotState::Custom("bank".to_string()),
        priority: 5,
    };

    assert_eq!(
        transfer_followup(&entry),
        Some(TransferFollowup {
            target: AccountId::new("Alt").unwrap(),
            amount: 12_345_678,
            stop_source: true
        })
    );
}

#[test]
fn bank_cooldown_store_matches_node_file_shape() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("SavedData").join("bank-cooldowns.json");
    let store = BankCooldownStore::new(path.clone(), 60_000);
    let account = AccountId::new("Main").unwrap();

    assert_eq!(
        store.try_mark_use_at(&account, 1_000).unwrap(),
        BankCooldownDecision::Ready
    );
    assert_eq!(
        store.try_mark_use_at(&account, 1_500).unwrap(),
        BankCooldownDecision::Wait {
            remaining_ms: 59_500
        }
    );

    let raw = std::fs::read_to_string(&path).unwrap();
    assert!(raw.contains("\"main\""));
    assert!(raw.contains("\"lastUse\": 1000"));

    assert_eq!(
        store.try_mark_use_at(&account, 61_000).unwrap(),
        BankCooldownDecision::Ready
    );
}

#[test]
fn sold_tracker_matches_claimed_collection_by_net_price() {
    let account = AccountId::new("Main").unwrap();
    let tracker = LiveSoldTracker::default();
    tracker
        .record_listing(
            &account,
            &json!({
                "auctionID": "auction-1",
                "weirdItemName": "Ancient Necron's Leggings",
                "price": 54_000_000,
                "profit": 22_000_000,
                "tag": "NECRON_LEGGINGS"
            }),
        )
        .unwrap();

    let collected = saf_core::flip::ihate_claiming_taxes(54_000_000.0).round() as u64;
    let matched = tracker
        .take_claim(&account, "Ancient Necron's Leggings", collected)
        .unwrap();

    assert_eq!(
        matched,
        Some(SoldListingMetadata {
            auction_id: "auction-1".to_string(),
            expected_collected: collected,
            profit: 22_000_000.0,
            tag: Some("NECRON_LEGGINGS".to_string()),
        })
    );
    assert!(
        tracker
            .take_claim(&account, "Ancient Necron's Leggings", collected)
            .unwrap()
            .is_none()
    );
}

#[test]
fn sold_tracker_recovers_closest_collection_amount() {
    let account = AccountId::new("Main").unwrap();
    let tracker = LiveSoldTracker::default();
    tracker
        .record_listing(
            &account,
            &json!({
                "auctionID": "auction-1",
                "weirdItemName": "Ancient Necron's Leggings",
                "price": 54_000_000,
                "profit": 22_000_000
            }),
        )
        .unwrap();

    let collected = saf_core::flip::ihate_claiming_taxes(54_000_000.0).round() as u64;
    let matched = tracker
        .take_claim(
            &account,
            "Ancient Necron's Leggings",
            collected.saturating_sub(1),
        )
        .unwrap();

    assert_eq!(
        matched,
        Some(SoldListingMetadata {
            auction_id: "auction-1".to_string(),
            expected_collected: collected,
            profit: 22_000_000.0,
            tag: None,
        })
    );
}

#[test]
fn sold_claim_notification_uses_tracked_amount_when_claim_chat_is_tiny() {
    let expected_collected = saf_core::flip::ihate_claiming_taxes(85_000_000.0).round() as u64;
    let metadata = SoldListingMetadata {
        auction_id: "auction-1".to_string(),
        expected_collected,
        profit: 25_000_000.0,
        tag: None,
    };

    assert_eq!(
        claim_notification_collected_coins(101, Some(&metadata)),
        expected_collected
    );
    assert_eq!(
        claim_notification_collected_coins(expected_collected - 500, Some(&metadata)),
        expected_collected - 500
    );
    assert_eq!(claim_notification_collected_coins(101, None), 101);
}

#[tokio::test]
async fn live_transfer_deposit_completion_queues_target_withdrawal() {
    let temp = tempfile::tempdir().unwrap();
    let source = AccountId::new("Main").unwrap();
    let target = AccountId::new("Alt").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &source,
            json!({
                "amount": 12_345_678,
                "withdraw": false,
                "personal": false,
                "transfer": {
                    "to": "Alt",
                    "stopSource": true
                }
            }),
            BotState::Custom("bank".to_string()),
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
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.active_windows.lock().unwrap().insert(
        source.clone(),
        WindowSnapshot {
            title: "Bank Amount".to_string(),
            slots: Vec::new(),
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    let source_snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert!(source_snapshot.queue.is_empty());
    let target_snapshot = crate::state_cli::snapshot(temp.path(), "Alt").unwrap();
    assert_eq!(target_snapshot.queue.len(), 1);
    assert_eq!(
        target_snapshot.queue[0].state,
        BotState::Custom("bank".to_string())
    );
    assert_eq!(target_snapshot.queue[0].action["amount"], json!(12_345_678));
    assert_eq!(target_snapshot.queue[0].action["withdraw"], json!(true));
    assert_eq!(target_snapshot.queue[0].action["personal"], json!(false));
    assert_eq!(
        target_snapshot.queue[0].action["transfer"]["from"],
        json!("Main")
    );

    let running = runtime.session.running_accounts();
    assert!(!running.iter().any(|account| account == source.as_str()));
    assert!(running.iter().any(|account| account == target.as_str()));
}

#[tokio::test]
async fn live_bank_queue_respects_existing_bank_cooldown_file() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "amount": 1_000_000,
                "withdraw": false,
                "personal": false
            }),
            BotState::Custom("bank".to_string()),
            5,
        )
        .await
        .unwrap();
    let cooldown_path = temp.path().join("SavedData").join("bank-cooldowns.json");
    BankCooldownStore::new(cooldown_path, DEFAULT_BANK_COOLDOWN_MS)
        .try_mark_use_at(&account, now_ms())
        .unwrap();
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
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert!(runtime.pending_market_steps.contains_key(&account));
}

#[tokio::test]
async fn transfer_followup_errors_retry_without_repeating_source_deposit() {
    let temp = tempfile::tempdir().unwrap();
    let source = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &source,
            json!({
                "amount": 12_345_678,
                "withdraw": false,
                "personal": false,
                "transfer": {
                    "to": "Alt",
                    "stopSource": true
                }
            }),
            BotState::Custom("bank".to_string()),
            5,
        )
        .await
        .unwrap();
    let saved_dir = temp.path().join("SavedData");
    std::fs::write(saved_dir.join("Alt.json"), b"{not valid json").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string(), "Alt".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.active_windows.lock().unwrap().insert(
        source.clone(),
        WindowSnapshot {
            title: "Bank Amount".to_string(),
            slots: Vec::new(),
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    let source_snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert!(source_snapshot.queue.is_empty());
    assert_eq!(runtime.pending_transfer_followups.len(), 1);

    std::fs::write(
        saved_dir.join("Alt.json"),
        serde_json::to_vec(&json!({
            "bidData": {},
            "queue": []
        }))
        .unwrap(),
    )
    .unwrap();
    runtime.pending_transfer_followups[0].ready_at = Instant::now() - Duration::from_millis(1);

    runtime.drain_pending_transfer_followups().await.unwrap();

    assert!(runtime.pending_transfer_followups.is_empty());
    let target_snapshot = crate::state_cli::snapshot(temp.path(), "Alt").unwrap();
    assert_eq!(target_snapshot.queue.len(), 1);
    assert_eq!(
        target_snapshot.queue[0].state,
        BotState::Custom("bank".to_string())
    );
    assert_eq!(target_snapshot.queue[0].action["amount"], json!(12_345_678));
    assert_eq!(
        target_snapshot.queue[0].action["transfer"]["from"],
        json!("Main")
    );
}

#[test]
fn purchased_bid_listing_action_uses_saved_bid_metadata() {
    let slot = saf_core::gui::WindowSlot {
        slot: 12,
        name: "diamond_leggings".to_string(),
        display_name: "Ancient Necron's Leggings".to_string(),
        lore: vec!["Click to claim!".to_string()],
        item_uuid: Some("item-uuid".to_string()),
    };
    let action = purchased_bid_listing_action(
        "item-uuid",
        &json!({
            "auctionID": "auction-1",
            "target": "56.3m",
            "profit": 25_000_000,
            "finder": "SNIPER",
            "itemName": "Necron's Leggings",
            "weirdItemName": "Ancient Necron's Leggings",
            "tag": "NECRON_LEGGINGS"
        }),
        &slot,
    )
    .unwrap();

    assert_eq!(action["auctionID"], json!("auction-1"));
    assert_eq!(action["price"], json!(56_300_000));
    assert_eq!(action["inventory"], json!("item-uuid"));
    assert_eq!(action["finder"], json!("SNIPER"));
    assert_eq!(action["tag"], json!("NECRON_LEGGINGS"));
}

#[tokio::test]
async fn dry_run_bids_window_records_claim_without_mutating_saved_bid_data() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let saved_dir = temp.path().join("SavedData");
    std::fs::create_dir_all(&saved_dir).unwrap();
    std::fs::write(
        saved_dir.join("Main.json"),
        serde_json::to_vec(&json!({
            "bidData": {
                "item-uuid": {
                    "auctionID": "auction-1",
                    "target": "56.3m"
                }
            },
            "queue": [
                {"action": {"reason": "manual"}, "state": "bids", "priority": 2}
            ]
        }))
        .unwrap(),
    )
    .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(
        config,
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Bids".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 12,
                name: "diamond_leggings".to_string(),
                display_name: "Ancient Necron's Leggings".to_string(),
                lore: vec!["Click to claim!".to_string()],
                item_uuid: Some("item-uuid".to_string()),
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        runtime.dry_run_records()[0].action["instruction"],
        json!({"type": "clickSlot", "slot": 12})
    );
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(
        snapshot.bid_data["item-uuid"]["auctionID"],
        json!("auction-1")
    );
    assert_eq!(snapshot.queue.len(), 1);
}

#[tokio::test]
async fn live_bids_window_claims_bid_and_queues_relist_then_clears_bids_entry() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let saved_dir = temp.path().join("SavedData");
    std::fs::create_dir_all(&saved_dir).unwrap();
    std::fs::write(
        saved_dir.join("Main.json"),
        serde_json::to_vec(&json!({
            "bidData": {
                "item-uuid": {
                    "auctionID": "auction-1",
                    "target": "56.3m",
                    "profit": 25_000_000,
                    "finder": "SNIPER",
                    "itemName": "Necron's Leggings",
                    "weirdItemName": "Ancient Necron's Leggings",
                    "tag": "NECRON_LEGGINGS"
                }
            },
            "queue": [
                {"action": {"reason": "manual"}, "state": "bids", "priority": 2}
            ]
        }))
        .unwrap(),
    )
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
            title: "Bids".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 12,
                name: "diamond_leggings".to_string(),
                display_name: "Ancient Necron's Leggings".to_string(),
                lore: vec!["Click to claim!".to_string()],
                item_uuid: Some("item-uuid".to_string()),
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.bid_data, json!({}));
    assert_eq!(snapshot.queue.len(), 2);
    assert_eq!(
        snapshot.queue[0].state,
        BotState::Custom("bids".to_string())
    );
    assert_eq!(snapshot.queue[1].state, BotState::Listing);
    assert_eq!(snapshot.queue[1].action["price"], json!(56_300_000));
    assert_eq!(snapshot.queue[1].action["inventory"], json!("item-uuid"));

    runtime.active_windows.lock().unwrap().insert(
        account,
        WindowSnapshot {
            title: "Bids".to_string(),
            slots: Vec::new(),
        },
    );
    runtime.process_market_queue_once().await.unwrap();

    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::Listing);
}

#[tokio::test]
async fn claimed_bid_relist_errors_retry_without_reclaiming_bid() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let saved_dir = temp.path().join("SavedData");
    std::fs::create_dir_all(&saved_dir).unwrap();
    let saved_data = json!({
        "bidData": {
            "item-uuid": {
                "auctionID": "auction-1",
                "target": "56.3m",
                "profit": 25_000_000,
                "finder": "SNIPER",
                "itemName": "Necron's Leggings",
                "weirdItemName": "Ancient Necron's Leggings",
                "tag": "NECRON_LEGGINGS"
            }
        },
        "queue": [
            {"action": {"reason": "manual"}, "state": "bids", "priority": 2}
        ]
    });
    std::fs::write(
        saved_dir.join("Main.json"),
        serde_json::to_vec(&saved_data).unwrap(),
    )
    .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let managed = runtime.managed_minecraft.get(&account).unwrap().clone();
    *managed.inner.lock().await = Some(LiveMinecraftClientBundle {
        minecraft: Arc::new(CorruptingClickMinecraftClient {
            account: account.clone(),
            path: saved_dir.join("Main.json"),
            corrupted: Arc::new(Mutex::new(false)),
            actions: Arc::new(Mutex::new(Vec::new())),
        }),
        inventory_provider: None,
    });
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Bids".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 12,
                name: "diamond_leggings".to_string(),
                display_name: "Ancient Necron's Leggings".to_string(),
                lore: vec!["Click to claim!".to_string()],
                item_uuid: Some("item-uuid".to_string()),
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(runtime.pending_claimed_bid_relists.len(), 1);
    assert!(
        !runtime
            .active_windows
            .lock()
            .unwrap()
            .contains_key(&account)
    );

    std::fs::write(
        saved_dir.join("Main.json"),
        serde_json::to_vec(&saved_data).unwrap(),
    )
    .unwrap();
    runtime.pending_claimed_bid_relists[0].ready_at = Instant::now() - Duration::from_millis(1);

    runtime.drain_pending_claimed_bid_relists().await.unwrap();

    assert!(runtime.pending_claimed_bid_relists.is_empty());
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.bid_data, json!({}));
    assert_eq!(snapshot.queue.len(), 2);
    assert_eq!(snapshot.queue[1].state, BotState::Listing);
    assert_eq!(snapshot.queue[1].action["inventory"], json!("item-uuid"));
}

#[tokio::test]
async fn live_reconcile_queues_bids_followup_when_bid_data_remains() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let saved_dir = temp.path().join("SavedData");
    std::fs::create_dir_all(&saved_dir).unwrap();
    std::fs::write(
        saved_dir.join("Main.json"),
        serde_json::to_vec(&json!({
            "bidData": {
                "item-uuid": {
                    "auctionID": "auction-1",
                    "target": "56.3m"
                }
            },
            "queue": [
                {"action": {"reason": "manual"}, "state": "reconcileAuctions", "priority": 2}
            ]
        }))
        .unwrap(),
    )
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
        account,
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: Vec::new(),
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(
        snapshot.queue[0].state,
        BotState::Custom("bids".to_string())
    );
}

#[tokio::test]
async fn bids_followup_errors_defer_retry_without_dropping_work() {
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

    runtime
        .queue_bids_followup_if_needed(&account)
        .await
        .unwrap();

    assert_eq!(runtime.deferred_queue_entries.len(), 1);
    assert_eq!(
        runtime.deferred_queue_entries[0].state,
        BotState::Custom("bids".to_string())
    );

    std::fs::write(
        saved_dir.join("Main.json"),
        serde_json::to_vec(&json!({
            "bidData": {
                "item-uuid": {
                    "auctionID": "auction-1",
                    "target": "56.3m"
                }
            },
            "queue": []
        }))
        .unwrap(),
    )
    .unwrap();
    runtime.deferred_queue_entries[0].ready_at = Instant::now() - Duration::from_millis(1);

    runtime.drain_deferred_queue_entries().await.unwrap();

    assert!(runtime.deferred_queue_entries.is_empty());
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(
        snapshot.queue[0].state,
        BotState::Custom("bids".to_string())
    );
}

#[test]
fn pending_create_auction_draft_parser_recovers_prefilled_item_details() {
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 13,
                name: "leather_leggings".to_string(),
                display_name: "AUCTION FOR ITEM:".to_string(),
                lore: vec![
                    "Ancient Necron's Leggings".to_string(),
                    "SkyBlock ID: POWER_WITHER_LEGGINGS".to_string(),
                ],
                item_uuid: Some("cea07d2a-106a-475a-9569-5c639766a9e7".to_string()),
            },
            saf_core::gui::WindowSlot {
                slot: 29,
                name: "green_terracotta".to_string(),
                display_name: "Create BIN Auction".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Item price: 56,300,000 coins".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
        ],
    };

    let draft = pending_create_auction_draft(&window).unwrap();

    assert_eq!(draft.item_name, "Ancient Necron's Leggings");
    assert_eq!(
        draft.item_uuid.as_str(),
        "cea07d2a-106a-475a-9569-5c639766a9e7"
    );
    assert_eq!(draft.tag.as_deref(), Some("POWER_WITHER_LEGGINGS"));
    assert_eq!(draft.list_price, 56_300_000);
    assert_eq!(draft.item_slot, 13);
}

#[test]
fn pending_create_auction_draft_parser_recovers_selected_item_without_uuid() {
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 13,
                name: "leather_boots".to_string(),
                display_name: "AUCTION FOR ITEM:".to_string(),
                lore: vec![
                    "Slug Boots".to_string(),
                    "Health: +150".to_string(),
                    "Click to pickup!".to_string(),
                ],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 29,
                name: "green_terracotta".to_string(),
                display_name: "Create BIN Auction".to_string(),
                lore: vec![
                    "Item: Slug Boots".to_string(),
                    "Item price: 5,600,000 coins".to_string(),
                    "Creation fee: 57,200 coins".to_string(),
                ],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Item price: 5,600,000 coins".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
        ],
    };

    let draft = pending_create_auction_draft(&window).unwrap();

    assert_eq!(draft.item_name, "Slug Boots");
    assert_eq!(draft.item_uuid.as_str(), "Slug Boots");
    assert_eq!(draft.list_price, 5_600_000);
    assert_eq!(draft.item_slot, 13);
}

#[test]
fn pending_create_auction_draft_parser_does_not_read_submit_button_price() {
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 13,
                name: "leather_leggings".to_string(),
                display_name: "AUCTION FOR ITEM:".to_string(),
                lore: vec!["Ancient Necron's Leggings".to_string()],
                item_uuid: Some("cea07d2a-106a-475a-9569-5c639766a9e7".to_string()),
            },
            saf_core::gui::WindowSlot {
                slot: 29,
                name: "green_terracotta".to_string(),
                display_name: "Create BIN Auction".to_string(),
                lore: vec!["Price: 805,160 coins".to_string()],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Item price: 56,300,000 coins".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
        ],
    };

    let draft = pending_create_auction_draft(&window).unwrap();

    assert_eq!(draft.list_price, 56_300_000);
}

#[test]
fn pending_create_auction_draft_parser_ignores_inventory_items() {
    let window = WindowSnapshot {
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
                display_name: "Ancient Rogue Sword".to_string(),
                lore: vec!["Health: +132 (+12)".to_string()],
                item_uuid: Some("f3d14872-a71d-42e0-a117-743f9380e5be".to_string()),
            },
            saf_core::gui::WindowSlot {
                slot: 29,
                name: "green_terracotta".to_string(),
                display_name: "Create BIN Auction".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Item price: 500 coins".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
        ],
    };

    assert!(pending_create_auction_draft(&window).is_none());
}

#[test]
fn pending_create_auction_draft_parser_prefers_real_display_name() {
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 13,
                name: "diamond_sword".to_string(),
                display_name: "Ancient Rogue Sword".to_string(),
                lore: vec!["Health: +132 (+12)".to_string()],
                item_uuid: Some("f3d14872-a71d-42e0-a117-743f9380e5be".to_string()),
            },
            saf_core::gui::WindowSlot {
                slot: 29,
                name: "green_terracotta".to_string(),
                display_name: "Create BIN Auction".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Item price: 500 coins".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
        ],
    };

    let draft = pending_create_auction_draft(&window).unwrap();

    assert_eq!(draft.item_name, "Ancient Rogue Sword");
    assert_eq!(draft.item_slot, 13);
}

#[tokio::test]
async fn dry_run_reconcile_opens_pending_draft_without_mutating_queue() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "reason": "listing-status-unclear",
                "inventory": "cea07d2a-106a-475a-9569-5c639766a9e7",
                "price": 56_300_000
            }),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(
        config,
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account,
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 15,
                name: "crafting_table".to_string(),
                display_name: "Create Auction".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        runtime.dry_run_records()[0].action["instruction"],
        json!({"type": "clickSlot", "slot": 15})
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
async fn startup_reconcile_skips_unknown_pending_draft_recovery() {
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
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(
        config,
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account,
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 15,
                name: "crafting_table".to_string(),
                display_name: "Create Auction".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        runtime.dry_run_records()[0].action["instruction"],
        json!({"type": "closeWindow"})
    );
    assert_eq!(
        runtime.dry_run_records()[0].action["reason"],
        json!("auction management reconciled")
    );
}

#[tokio::test]
async fn live_reconcile_recovers_untracked_pending_draft_as_listing() {
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
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Create BIN Auction".to_string(),
            slots: vec![
                saf_core::gui::WindowSlot {
                    slot: 13,
                    name: "diamond_boots".to_string(),
                    display_name: "AUCTION FOR ITEM:".to_string(),
                    lore: vec![
                        "Submerged Shark Scale Boots".to_string(),
                        "Item Tag: SHARK_SCALE_BOOTS".to_string(),
                    ],
                    item_uuid: Some("1e447a92-6ec9-43e6-a0b2-deb1572af882".to_string()),
                },
                saf_core::gui::WindowSlot {
                    slot: 29,
                    name: "green_terracotta".to_string(),
                    display_name: "Create BIN Auction".to_string(),
                    lore: Vec::new(),
                    item_uuid: None,
                },
                saf_core::gui::WindowSlot {
                    slot: 31,
                    name: "gold_ingot".to_string(),
                    display_name: "Item price: 25,100,000 coins".to_string(),
                    lore: Vec::new(),
                    item_uuid: None,
                },
            ],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 1);
    assert_eq!(snapshot.queue[0].state, BotState::Listing);
    assert_eq!(
        snapshot.queue[0].action["inventory"],
        json!("1e447a92-6ec9-43e6-a0b2-deb1572af882")
    );
    assert_eq!(snapshot.queue[0].action["tag"], json!("SHARK_SCALE_BOOTS"));
    assert_eq!(snapshot.queue[0].action["price"], json!(25_100_000));
    assert_eq!(
        snapshot.queue[0].action["reason"],
        json!("pending-draft-recovery")
    );
    assert_eq!(runtime.report().completed_queue_entries, 1);
}

#[tokio::test]
async fn live_listing_prioritizes_reconcile_when_pending_draft_blocks_next_listing() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let store = FileQueueStore::new(temp.path());
    store
        .add(
            &account,
            json!({
                "auctionID": "hedgehog-auction",
                "inventory": "12a29de8-9313-4bb9-822c-6fb4962b78ab",
                "inv": "12a29de8-9313-4bb9-822c-6fb4962b78ab",
                "itemName": "[Lvl 100] Hedgehog",
                "tag": "PET_HEDGEHOG",
                "price": 34_100_000,
                "pricePaid": 32_000_000,
                "targetPrice": 35_139_888,
                "time": 48.0
            }),
            BotState::Listing,
            1,
        )
        .await
        .unwrap();
    store
        .add(
            &account,
            json!({"reason": "slot-pressure"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
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
    runtime
        .active_windows
        .lock()
        .unwrap()
        .insert(account.clone(), pending_slug_boots_create_window(None));

    runtime.process_market_queue_once().await.unwrap();

    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 2);
    assert!(snapshot.queue.iter().any(|entry| {
        entry.state == BotState::Listing
            && entry.action["inventory"] == json!("12a29de8-9313-4bb9-822c-6fb4962b78ab")
    }));
    assert!(snapshot.queue.iter().any(|entry| {
        entry.state == BotState::Listing
            && entry.action["inventory"] == json!("Slug Boots")
            && entry.action["price"] == json!(5_600_000)
            && entry.action["reason"] == json!("pending-draft-recovery")
    }));
    assert_eq!(runtime.report().completed_queue_entries, 1);
}

#[tokio::test]
async fn live_reconcile_preempts_listing_before_opening_new_auction() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let store = FileQueueStore::new(temp.path());
    store
        .add(
            &account,
            json!({
                "auctionID": "rancher-boots-auction",
                "inventory": "c002995c-1c64-437b-a49c-b681a66803d6",
                "inv": "c002995c-1c64-437b-a49c-b681a66803d6",
                "itemName": "Mossy Rancher's Boots",
                "tag": "RANCHERS_BOOTS",
                "price": 48_300_000,
                "pricePaid": 13_000_000,
                "targetPrice": 49_807_602,
                "time": 48.0
            }),
            BotState::Listing,
            1,
        )
        .await
        .unwrap();
    store
        .add(
            &account,
            json!({"reason": "slot-pressure"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
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
            title: "Co-op Auction House".to_string(),
            slots: vec![
                saf_core::gui::WindowSlot {
                    slot: 13,
                    name: "book".to_string(),
                    display_name: "Manage Auctions".to_string(),
                    lore: vec!["View your auctions".to_string()],
                    item_uuid: None,
                },
                saf_core::gui::WindowSlot {
                    slot: 15,
                    name: "gold_block".to_string(),
                    display_name: "Create Auction".to_string(),
                    lore: vec!["Start a new auction".to_string()],
                    item_uuid: None,
                },
            ],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(13)]);
    let pending = runtime.pending_market_steps.get(&account).unwrap();
    assert_eq!(
        pending.entry.state,
        BotState::Custom("reconcileAuctions".to_string())
    );
}

#[tokio::test]
async fn live_listing_prioritizes_matching_pending_draft_before_next_listing() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let store = FileQueueStore::new(temp.path());
    store
        .add(
            &account,
            json!({
                "auctionID": "hedgehog-auction",
                "inventory": "12a29de8-9313-4bb9-822c-6fb4962b78ab",
                "inv": "12a29de8-9313-4bb9-822c-6fb4962b78ab",
                "itemName": "[Lvl 100] Hedgehog",
                "tag": "PET_HEDGEHOG",
                "price": 34_100_000,
                "pricePaid": 32_000_000,
                "targetPrice": 35_139_888,
                "time": 48.0
            }),
            BotState::Listing,
            1,
        )
        .await
        .unwrap();
    store
        .add(
            &account,
            json!({
                "reason": "pending-draft-recovery",
                "inventory": "4e36b990-5063-4d28-ada4-bb43ea33238a",
                "inv": "4e36b990-5063-4d28-ada4-bb43ea33238a",
                "itemName": "Slug Boots",
                "weirdItemName": "Slug Boots",
                "tag": "SLUG_BOOTS",
                "price": 5_600_000,
                "pricePaid": 0,
                "targetPrice": 5_600_000,
                "time": 48.0
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
        pending_slug_boots_create_window(Some("4e36b990-5063-4d28-ada4-bb43ea33238a")),
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(29)]);

    client.push_event(MinecraftEvent::WindowOpen(
        pending_slug_boots_confirm_window(),
    ));
    runtime.poll_minecraft_once().await.unwrap();
    runtime.active_window_observed_at.lock().unwrap().insert(
        account.clone(),
        Instant::now() - MARKET_WINDOW_SETTLE_DELAY - Duration::from_millis(1),
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::ClickSlot(29),
            MinecraftAction::ClickSlot(11)
        ]
    );
    let pending = runtime.pending_listing_confirmations.get(&account).unwrap();
    assert_eq!(pending.entry.action["itemName"], json!("Slug Boots"));
    assert!(
        !runtime
            .pending_listing_price_mismatch_retries
            .contains_key(&account)
    );
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 2);
}

#[tokio::test]
async fn live_listing_continues_matching_pending_draft_with_default_price() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let store = FileQueueStore::new(temp.path());
    store
        .add(
            &account,
            json!({
                "auctionID": "hot-crimson-auction",
                "inventory": "068aac49-a7c7-4829-b0b3-fa9c1b89be44",
                "inv": "068aac49-a7c7-4829-b0b3-fa9c1b89be44",
                "itemName": "Ancient Hot Crimson Helmet ✪✪✪✪✪",
                "tag": "HOT_CRIMSON_HELMET",
                "price": 14_500_000,
                "pricePaid": 11_000_000,
                "targetPrice": 14_913_560,
                "time": 48.0
            }),
            BotState::Listing,
            1,
        )
        .await
        .unwrap();
    store
        .add(
            &account,
            json!({"reason": "slot-pressure"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
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
        pending_hot_crimson_create_window_with_default_price(),
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::ClickSlot(31),
            MinecraftAction::TypeText("\"14500000\"".to_string())
        ]
    );
    let snapshot = crate::state_cli::snapshot(temp.path(), "Main").unwrap();
    assert_eq!(snapshot.queue.len(), 2);
    assert!(!snapshot.queue.iter().any(|entry| {
        entry.state == BotState::Custom("reconcileAuctions".to_string())
            && entry.action["reason"] == json!("listing-status-unclear")
    }));
}

fn pending_slug_boots_confirm_window() -> WindowSnapshot {
    WindowSnapshot {
        title: "Confirm BIN Auction".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 11,
            name: "lime_wool".to_string(),
            display_name: "Confirm Auction".to_string(),
            lore: vec!["Cost: 57,200 coins".to_string()],
            item_uuid: None,
        }],
    }
}

fn pending_slug_boots_create_window(item_uuid: Option<&str>) -> WindowSnapshot {
    let mut item_lore = vec!["Slug Boots".to_string()];
    if item_uuid.is_some() {
        item_lore.push("Item Tag: SLUG_BOOTS".to_string());
    }
    item_lore.push("Click to pickup!".to_string());

    WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 13,
                name: "leather_boots".to_string(),
                display_name: "AUCTION FOR ITEM:".to_string(),
                lore: item_lore,
                item_uuid: item_uuid.map(str::to_string),
            },
            saf_core::gui::WindowSlot {
                slot: 29,
                name: "green_terracotta".to_string(),
                display_name: "Create BIN Auction".to_string(),
                lore: vec![
                    "Item: Slug Boots".to_string(),
                    "Auction duration: 2 Days".to_string(),
                    "Item price: 5,600,000 coins".to_string(),
                    "Creation fee: 57,200 coins".to_string(),
                    "Click to submit!".to_string(),
                ],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Item price: 5,600,000 coins".to_string(),
                lore: vec!["Extra fee: +56,000 coins (1%)".to_string()],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 33,
                name: "clock".to_string(),
                display_name: "Duration: 2 Days".to_string(),
                lore: vec!["Extra fee: +1,200 coins".to_string()],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 86,
                name: "player_head".to_string(),
                display_name: "[Lvl 100] Hedgehog".to_string(),
                lore: vec!["LEGENDARY".to_string()],
                item_uuid: Some("12a29de8-9313-4bb9-822c-6fb4962b78ab".to_string()),
            },
        ],
    }
}

fn pending_hot_crimson_create_window_with_default_price() -> WindowSnapshot {
    WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 13,
                name: "diamond_helmet".to_string(),
                display_name: "AUCTION FOR ITEM:".to_string(),
                lore: vec![
                    "Ancient Hot Crimson Helmet ✪✪✪✪✪".to_string(),
                    "Item Tag: HOT_CRIMSON_HELMET".to_string(),
                    "Click to pickup!".to_string(),
                ],
                item_uuid: Some("068aac49-a7c7-4829-b0b3-fa9c1b89be44".to_string()),
            },
            saf_core::gui::WindowSlot {
                slot: 29,
                name: "red_terracotta".to_string(),
                display_name: "Create BIN Auction".to_string(),
                lore: vec![
                    "Item: Ancient Hot Crimson Helmet ✪✪✪✪✪".to_string(),
                    "Item price: 500 coins".to_string(),
                    "Click to submit!".to_string(),
                ],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Item price: 500 coins".to_string(),
                lore: vec!["Click to edit!".to_string()],
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 33,
                name: "clock".to_string(),
                display_name: "Duration: 6 Hours".to_string(),
                lore: vec!["Click to edit!".to_string()],
                item_uuid: None,
            },
        ],
    }
}

#[tokio::test]
async fn dry_run_reconcile_skips_pending_draft_recovery_when_auction_slots_are_full() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"reason": "listing-status-unclear"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
        )
        .await
        .unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut runtime = LiveRuntime::start(
        config,
        RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path()),
    )
    .await
    .unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account,
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: vec![
                saf_core::gui::WindowSlot {
                    slot: 4,
                    name: "gold_ingot".to_string(),
                    display_name: "Auction Slots".to_string(),
                    lore: vec!["Active auctions: 14/14".to_string()],
                    item_uuid: None,
                },
                saf_core::gui::WindowSlot {
                    slot: 15,
                    name: "crafting_table".to_string(),
                    display_name: "Create Auction".to_string(),
                    lore: Vec::new(),
                    item_uuid: None,
                },
            ],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        runtime.dry_run_records()[0].action["instruction"],
        json!({"type": "closeWindow"})
    );
    assert_eq!(
        runtime.dry_run_records()[0].action["reason"],
        json!("auction management reconciled")
    );
}

#[tokio::test]
async fn live_reconcile_submits_pending_draft_through_confirm_window() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "reason": "listing-status-unclear",
                "inventory": "cea07d2a-106a-475a-9569-5c639766a9e7",
                "price": 56_300_000
            }),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
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
            title: "Manage Auctions".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 15,
                name: "crafting_table".to_string(),
                display_name: "Create Auction".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );

    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Create BIN Auction".to_string(),
            slots: vec![
                saf_core::gui::WindowSlot {
                    slot: 13,
                    name: "leather_leggings".to_string(),
                    display_name: "AUCTION FOR ITEM:".to_string(),
                    lore: vec!["Ancient Necron's Leggings".to_string()],
                    item_uuid: Some("cea07d2a-106a-475a-9569-5c639766a9e7".to_string()),
                },
                saf_core::gui::WindowSlot {
                    slot: 29,
                    name: "green_terracotta".to_string(),
                    display_name: "Create BIN Auction".to_string(),
                    lore: Vec::new(),
                    item_uuid: None,
                },
                saf_core::gui::WindowSlot {
                    slot: 31,
                    name: "gold_ingot".to_string(),
                    display_name: "Item price: 56,300,000 coins".to_string(),
                    lore: Vec::new(),
                    item_uuid: None,
                },
            ],
        },
    );

    runtime.process_market_queue_once().await.unwrap();
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );

    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Confirm BIN Auction".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 11,
                name: "lime_wool".to_string(),
                display_name: "Confirm Auction".to_string(),
                lore: vec!["Price: 56,300,000 coins".to_string()],
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .is_empty()
    );
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
}

#[tokio::test]
async fn live_reconcile_pending_draft_click_keeps_window_until_transition() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "reason": "listing-status-unclear",
                "inventory": "cea07d2a-106a-475a-9569-5c639766a9e7",
                "price": 56_300_000
            }),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
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
    let manage_window = WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 15,
            name: "crafting_table".to_string(),
            display_name: "Create Auction".to_string(),
            lore: Vec::new(),
            item_uuid: None,
        }],
    };
    runtime
        .active_windows
        .lock()
        .unwrap()
        .insert(account.clone(), manage_window.clone());

    runtime.process_market_queue_once().await.unwrap();
    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(15)]);
    assert_eq!(
        runtime.active_windows.lock().unwrap().get(&account),
        Some(&manage_window)
    );
    assert!(runtime.pending_market_steps.contains_key(&account));
}

#[tokio::test]
async fn live_reconcile_submit_draft_click_keeps_window_until_confirm_transition() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "reason": "listing-status-unclear",
                "inventory": "cea07d2a-106a-475a-9569-5c639766a9e7",
                "price": 56_300_000
            }),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
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
    let create_window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 13,
                name: "leather_leggings".to_string(),
                display_name: "AUCTION FOR ITEM:".to_string(),
                lore: vec!["Ancient Necron's Leggings".to_string()],
                item_uuid: Some("cea07d2a-106a-475a-9569-5c639766a9e7".to_string()),
            },
            saf_core::gui::WindowSlot {
                slot: 29,
                name: "green_terracotta".to_string(),
                display_name: "Create BIN Auction".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Item price: 56,300,000 coins".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
        ],
    };
    runtime
        .active_windows
        .lock()
        .unwrap()
        .insert(account.clone(), create_window.clone());

    runtime.process_market_queue_once().await.unwrap();
    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(29)]);
    assert_eq!(
        runtime.active_windows.lock().unwrap().get(&account),
        Some(&create_window)
    );
    assert!(runtime.pending_market_steps.contains_key(&account));
}

#[tokio::test]
async fn live_reconcile_claims_individual_sold_auction_action_window() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"reason": "sold-message"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
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
            title: "Manage Auctions".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 10,
                name: "diamond_chestplate".to_string(),
                display_name: "Sold Chestplate".to_string(),
                lore: vec!["Buyer: Someone".to_string(), "Seller: Main".to_string()],
                item_uuid: Some("sold-item-uuid".to_string()),
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(10)]);
    assert!(runtime.pending_market_steps.contains_key(&account));
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );

    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "BIN Auction View".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_block".to_string(),
                display_name: "Claim Auction".to_string(),
                lore: vec!["Click to collect your coins.".to_string()],
                item_uuid: None,
            }],
        },
    );
    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::ClickSlot(10),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::CloseWindow,
        ]
    );
    assert!(!runtime.pending_market_steps.contains_key(&account));
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        0
    );
}

#[tokio::test]
async fn live_reconcile_retries_individual_sold_claim_until_action_window() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"reason": "sold-message"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
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
            title: "Manage Auctions".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 10,
                name: "diamond_chestplate".to_string(),
                display_name: "Sold Chestplate".to_string(),
                lore: vec![
                    "Buyer: Someone".to_string(),
                    "Seller: Main".to_string(),
                    "Status: Sold!".to_string(),
                    "Click to inspect!".to_string(),
                ],
                item_uuid: Some("sold-item-uuid".to_string()),
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(10)]);
    assert!(runtime.pending_market_steps.contains_key(&account));

    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 0,
                name: "black_stained_glass_pane".to_string(),
                display_name: "Black Stained Glass Pane".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            }],
        },
    );
    runtime.active_window_observed_at.lock().unwrap().insert(
        account.clone(),
        Instant::now() - MARKET_WINDOW_SETTLE_DELAY - Duration::from_millis(1),
    );
    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(10)]);
    assert!(runtime.pending_market_steps.contains_key(&account));
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );

    runtime
        .pending_market_steps
        .get_mut(&account)
        .unwrap()
        .last_attempt = Instant::now() - MARKET_STEP_RETRY_INTERVAL - Duration::from_millis(1);
    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![MinecraftAction::ClickSlot(10), MinecraftAction::CloseWindow]
    );
    assert!(!runtime.pending_market_steps.contains_key(&account));
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );
}

#[tokio::test]
async fn live_reconcile_keeps_sold_claim_context_across_reconcile_entry_refresh() {
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
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 10,
                name: "leather_leggings".to_string(),
                display_name: "Ancient Necron's Leggings".to_string(),
                lore: vec![
                    "Seller: Main".to_string(),
                    "Buyer: Someone".to_string(),
                    "Sold for: 805,160 coins".to_string(),
                    "Status: Sold!".to_string(),
                    "Click to inspect!".to_string(),
                ],
                item_uuid: Some("sold-item-uuid".to_string()),
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    runtime
        .pending_market_steps
        .get_mut(&account)
        .unwrap()
        .entry
        .action = json!({"reason": "regular-poll"});
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "BIN Auction View".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_block".to_string(),
                display_name: "Claim Auction".to_string(),
                lore: vec!["Click to collect your coins.".to_string()],
                item_uuid: None,
            }],
        },
    );
    runtime.active_window_observed_at.lock().unwrap().insert(
        account.clone(),
        Instant::now() - MARKET_WINDOW_SETTLE_DELAY - Duration::from_millis(1),
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::ClickSlot(10),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::CloseWindow,
        ]
    );
    assert!(!runtime.pending_market_steps.contains_key(&account));
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        0
    );
}

#[tokio::test]
async fn live_reconcile_preserves_sold_claim_context_after_window_open_event() {
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
        ..SafConfig::default()
    };
    let mut options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    options.market_actions = MarketActionMode::Live;
    let mut runtime = LiveRuntime::start(config, options).await.unwrap();
    let client = install_recorded_minecraft_client(&mut runtime, &account).await;
    runtime.active_windows.lock().unwrap().insert(
        account.clone(),
        WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 10,
                name: "leather_leggings".to_string(),
                display_name: "Ancient Necron's Leggings".to_string(),
                lore: vec![
                    "Seller: Main".to_string(),
                    "Buyer: Someone".to_string(),
                    "Sold for: 805,160 coins".to_string(),
                    "Status: Sold!".to_string(),
                    "Click to inspect!".to_string(),
                ],
                item_uuid: Some("sold-item-uuid".to_string()),
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::ClickSlot(10)]);
    assert!(runtime.pending_market_steps.contains_key(&account));

    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 31,
            name: "gold_block".to_string(),
            display_name: "Claim Auction".to_string(),
            lore: vec!["Click to collect your coins.".to_string()],
            item_uuid: None,
        }],
    }));
    runtime.poll_minecraft_once().await.unwrap();

    assert!(runtime.pending_market_steps.contains_key(&account));
    runtime.active_window_observed_at.lock().unwrap().insert(
        account.clone(),
        Instant::now() - MARKET_WINDOW_SETTLE_DELAY - Duration::from_millis(1),
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![
            MinecraftAction::ClickSlot(10),
            MinecraftAction::ClickSlot(31),
            MinecraftAction::CloseWindow,
        ]
    );
    assert!(!runtime.pending_market_steps.contains_key(&account));
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        0
    );
}

#[tokio::test]
async fn live_reconcile_closes_stale_auction_view_without_sold_claim_context() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"reason": "regular-poll"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
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
            title: "BIN Auction View".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_block".to_string(),
                display_name: "Claim Auction".to_string(),
                lore: vec!["Click to collect your coins.".to_string()],
                item_uuid: None,
            }],
        },
    );

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(client.actions(), vec![MinecraftAction::CloseWindow]);
    assert!(
        !runtime
            .active_windows
            .lock()
            .unwrap()
            .contains_key(&account)
    );
    assert!(!runtime.pending_market_steps.contains_key(&account));
    assert_eq!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .len(),
        1
    );
}

#[tokio::test]
async fn stale_pending_draft_submit_clears_window_before_reclicking() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({
                "reason": "listing-status-unclear",
                "inventory": "cea07d2a-106a-475a-9569-5c639766a9e7",
                "price": 56_300_000
            }),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
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
    let create_window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            saf_core::gui::WindowSlot {
                slot: 13,
                name: "leather_leggings".to_string(),
                display_name: "AUCTION FOR ITEM:".to_string(),
                lore: vec!["Ancient Necron's Leggings".to_string()],
                item_uuid: Some("cea07d2a-106a-475a-9569-5c639766a9e7".to_string()),
            },
            saf_core::gui::WindowSlot {
                slot: 29,
                name: "green_terracotta".to_string(),
                display_name: "Create BIN Auction".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
            saf_core::gui::WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Item price: 56,300,000 coins".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
        ],
    };
    runtime
        .active_windows
        .lock()
        .unwrap()
        .insert(account.clone(), create_window);

    runtime.process_market_queue_once().await.unwrap();
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
        .insert(account.clone(), stale_attempt);

    runtime.process_market_queue_once().await.unwrap();

    assert_eq!(
        client.actions(),
        vec![MinecraftAction::ClickSlot(29), MinecraftAction::CloseWindow]
    );
    assert!(
        !runtime
            .active_windows
            .lock()
            .unwrap()
            .contains_key(&account)
    );
    assert!(!runtime.pending_market_steps.contains_key(&account));
    assert!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .is_empty()
    );
    assert_eq!(runtime.report().completed_queue_entries, 1);
}

#[tokio::test]
async fn pending_draft_confirm_stats_errors_do_not_keep_reconcile_queued() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"reason": "listing-status-unclear"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2,
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
                slot: 11,
                name: "lime_wool".to_string(),
                display_name: "Confirm Auction".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            }],
        },
    );
    let auction_slots = runtime.stats.auction_slots.clone();
    let poison_result = std::panic::catch_unwind(move || {
        let _guard = auction_slots.lock().unwrap();
        panic!("poison pending draft slot stats lock");
    });
    assert!(poison_result.is_err());

    runtime.process_market_queue_once().await.unwrap();

    assert!(
        crate::state_cli::snapshot(temp.path(), "Main")
            .unwrap()
            .queue
            .is_empty()
    );
    assert!(
        !runtime
            .active_windows
            .lock()
            .unwrap()
            .contains_key(&account)
    );
    assert_eq!(runtime.report().completed_queue_entries, 1);
}

#[test]
fn scoreboard_upload_requires_purse_or_piggy_line() {
    assert!(should_upload_scoreboard(&[
        "Bits: 120".to_string(),
        "Purse: 1.2M".to_string()
    ]));
    assert!(should_upload_scoreboard(&["Piggy: 250k".to_string()]));
    assert!(!should_upload_scoreboard(&[
        "Your Island".to_string(),
        "Bits: 120".to_string()
    ]));
}

#[tokio::test]
async fn delist_everything_queues_from_cached_active_window() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let options = RunLiveOptions::new(temp.path().join("commands.jsonl"), temp.path());
    let runtime = LiveRuntime::start(config, options).await.unwrap();
    runtime.active_windows.lock().unwrap().insert(
        account,
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
    );

    let outcome = runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "delist_everything".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::DelistAllQueued { queued: 1, .. }
    ));
    assert_eq!(runtime.dry_run_records().len(), 1);
}

#[tokio::test]
async fn live_diagnostics_returns_cached_window_snapshot() {
    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new("Main").unwrap();
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
            account.clone(),
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

    let outcome = runtime
        .session
        .process_local_command(LocalCommand::Terminal {
            line: "diagslots current".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::GuiSlotDiagnostics {
            diagnostics: GuiSlotDiagnostics {
                account: ref diagnostic_account,
                ref windows,
                ..
            },
        } if diagnostic_account == &account && windows[0].title == "Manage Auctions"
    ));
}

#[tokio::test]
async fn live_diagnostics_explicit_target_does_not_return_stale_cached_window() {
    let account = AccountId::new("Main").unwrap();
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    client.push_event(MinecraftEvent::WindowOpen(WindowSnapshot {
        title: "Co-op Auction House".to_string(),
        slots: vec![saf_core::gui::WindowSlot {
            slot: 15,
            name: "gold_block".to_string(),
            display_name: "Create Auction".to_string(),
            lore: vec!["Start a new auction".to_string()],
            item_uuid: None,
        }],
    }));
    let active_windows = Arc::new(Mutex::new(BTreeMap::from([(
        account.clone(),
        WindowSnapshot {
            title: "Create BIN Auction".to_string(),
            slots: vec![saf_core::gui::WindowSlot {
                slot: 13,
                name: "stone_button".to_string(),
                display_name: "Click an item in your inventory!".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            }],
        },
    )])));
    let provider = LiveActiveAuctionProvider {
        clients: BTreeMap::from([(account.clone(), client.clone() as Arc<dyn MinecraftClient>)]),
        active_windows,
        deferred_events: Arc::new(Mutex::new(BTreeMap::new())),
        stats: Arc::new(LiveStatsProvider::new(vec![account.clone()])),
        mode: MarketActionMode::Live,
        wait_timeout: Duration::from_millis(50),
    };

    let error = provider
        .diagnose_slots(&account, Some("manage"))
        .await
        .unwrap_err();

    assert!(
        matches!(error, PortError::Unavailable(message) if message.contains("no GUI window snapshot"))
    );
    assert_eq!(
        client.actions(),
        vec![MinecraftAction::Chat("/ah".to_string())]
    );
}

#[tokio::test]
async fn window_snapshot_is_cached_when_stats_recording_fails() {
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
    let account = AccountId::new("Alt").unwrap();

    runtime
        .remember_window_snapshot(
            account.clone(),
            WindowSnapshot {
                title: "Manage Auctions".to_string(),
                slots: Vec::new(),
            },
        )
        .unwrap();

    assert!(
        runtime
            .active_windows
            .lock()
            .unwrap()
            .contains_key(&account)
    );
}
