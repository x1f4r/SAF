use super::*;
use crate::gui::{WindowSlot, WindowSnapshot};
use crate::state::{BotState, QueueEntry};
use serde_json::json;

#[test]
fn queue_entries_convert_to_market_workflows() {
    let entry = QueueEntry {
        action: json!({"auctionID": "auction-1"}),
        state: BotState::Buying,
        priority: 5,
    };
    assert_eq!(
        MarketWorkflow::from_queue_entry(&entry),
        Some(MarketWorkflow::BuyNow {
            auction_id: AuctionId::new("auction-1").unwrap()
        })
    );

    let entry = QueueEntry {
        action: json!({"inv": "item-uuid", "price": 1_500_000, "time": 12}),
        state: BotState::ListingNoName,
        priority: 4,
    };
    assert_eq!(
        MarketWorkflow::from_queue_entry(&entry),
        Some(MarketWorkflow::ListItem {
            item_uuid: ItemUuid::new("item-uuid").unwrap(),
            price: 1_500_000.0,
            hours: 12.0
        })
    );
}

#[test]
fn buy_workflow_uses_semantic_auction_action_slot() {
    let workflow = MarketWorkflow::BuyNow {
        auction_id: AuctionId::new("auction-1").unwrap(),
    };
    assert_eq!(
        workflow.next_step(None).instruction,
        MarketInstruction::OpenAuction {
            auction_id: AuctionId::new("auction-1").unwrap()
        }
    );

    let window = WindowSnapshot {
        title: "BIN Auction View".to_string(),
        slots: vec![WindowSlot {
            slot: 33,
            name: "gold_nugget".to_string(),
            display_name: "Buy Item".to_string(),
            lore: vec!["Click to buy".to_string()],
            item_uuid: None,
        }],
    };
    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::ClickSlot { slot: 33 }
    );
}

#[test]
fn listing_workflow_types_price_inside_price_window() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("item-uuid").unwrap(),
        price: 12_345_678.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Create BIN Auction - Price".to_string(),
        slots: Vec::new(),
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::TypeText {
            text: "\"12345678\"".to_string()
        }
    );
}

#[test]
fn listing_workflow_prefers_exact_inventory_uuid_slot() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("wanted-uuid").unwrap(),
        price: 12_345_678.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            WindowSlot {
                slot: 13,
                name: "paper".to_string(),
                display_name: "Auction Item".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
            WindowSlot {
                slot: 82,
                name: "diamond_sword".to_string(),
                display_name: "Sword".to_string(),
                lore: Vec::new(),
                item_uuid: Some("wanted-uuid".to_string()),
            },
        ],
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::ClickSlot { slot: 82 }
    );
}

#[test]
fn listing_workflow_selects_purchased_item_by_name_when_uuid_is_tag_fallback() {
    let entry = QueueEntry {
        action: json!({
            "auctionID": "auction-1",
            "inventory": "PET_BLACK_CAT",
            "tag": "PET_BLACK_CAT",
            "itemName": "§7[Lvl 88] §dBlack Cat",
            "price": 70_800_000,
            "time": 48
        }),
        state: BotState::Listing,
        priority: 1,
    };
    let workflow = MarketWorkflow::from_queue_entry(&entry).unwrap();
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            WindowSlot {
                slot: 13,
                name: "barrier".to_string(),
                display_name: "Item to Auction".to_string(),
                lore: vec!["Click an item in your inventory.".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 82,
                name: "player_head".to_string(),
                display_name: "§7[Lvl 88] §dBlack Cat".to_string(),
                lore: Vec::new(),
                item_uuid: Some("actual-pet-uuid".to_string()),
            },
        ],
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::ClickSlot { slot: 82 }
    );
}

#[test]
fn listing_workflow_does_not_use_tag_fallback_for_concrete_uuid() {
    let entry = QueueEntry {
        action: json!({
            "auctionID": "auction-1",
            "inventory": "real-purchased-uuid",
            "tag": "SHARK_SCALE_BOOTS",
            "itemName": "Submerged Shark Scale Boots",
            "price": 25_100_000,
            "time": 48
        }),
        state: BotState::Listing,
        priority: 1,
    };
    let workflow = MarketWorkflow::from_queue_entry(&entry).unwrap();
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            WindowSlot {
                slot: 13,
                name: "barrier".to_string(),
                display_name: "Item to Auction".to_string(),
                lore: vec!["Click an item in your inventory.".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 82,
                name: "leather_boots".to_string(),
                display_name: "Submerged Shark Scale Boots".to_string(),
                lore: vec!["SHARK_SCALE_BOOTS".to_string()],
                item_uuid: Some("different-uuid".to_string()),
            },
        ],
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::ClickSlot { slot: 13 }
    );
}

#[test]
fn listing_workflow_waits_for_inventory_slots_before_selecting_item() {
    let workflow = MarketWorkflow::ListItemWithContext {
        item_uuid: ItemUuid::new("PET_BLACK_CAT").unwrap(),
        item_name: Some("§7[Lvl 88] §dBlack Cat".to_string()),
        tag: Some("PET_BLACK_CAT".to_string()),
        price: 70_800_000.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![WindowSlot {
            slot: 13,
            name: "barrier".to_string(),
            display_name: "Item to Auction".to_string(),
            lore: vec!["Click an item in your inventory.".to_string()],
            item_uuid: None,
        }],
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::Noop
    );
}

#[test]
fn listing_workflow_enters_price_when_selected_item_is_in_create_window() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("wanted-uuid").unwrap(),
        price: 12_345_678.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            WindowSlot {
                slot: 13,
                name: "diamond_sword".to_string(),
                display_name: "Sword".to_string(),
                lore: Vec::new(),
                item_uuid: Some("wanted-uuid".to_string()),
            },
            WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Auction Price".to_string(),
                lore: vec!["Price: 0 coins".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 33,
                name: "clock".to_string(),
                display_name: "Auction Duration".to_string(),
                lore: vec!["Duration: 2 Days".to_string()],
                item_uuid: None,
            },
        ],
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::ClickSlotThenType {
            slot: 31,
            text: "\"12345678\"".to_string()
        }
    );
}

#[test]
fn listing_workflow_accepts_selected_item_without_visible_uuid_when_name_matches() {
    let entry = QueueEntry {
        action: json!({
            "auctionID": "auction-1",
            "inventory": "real-purchased-uuid",
            "tag": "SHARK_SCALE_BOOTS",
            "itemName": "Submerged Shark Scale Boots",
            "price": 25_100_000,
            "time": 48
        }),
        state: BotState::Listing,
        priority: 1,
    };
    let workflow = MarketWorkflow::from_queue_entry(&entry).unwrap();
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            WindowSlot {
                slot: 13,
                name: "leather_boots".to_string(),
                display_name: "Submerged Shark Scale Boots".to_string(),
                lore: vec!["SHARK_SCALE_BOOTS".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Auction Price".to_string(),
                lore: vec!["Price: 0 coins".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 82,
                name: "diamond_sword".to_string(),
                display_name: "Different Item".to_string(),
                lore: Vec::new(),
                item_uuid: Some("other-uuid".to_string()),
            },
        ],
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::ClickSlotThenType {
            slot: 31,
            text: "\"25100000\"".to_string()
        }
    );
}

#[test]
fn listing_workflow_opens_duration_selector_after_price_is_set() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("wanted-uuid").unwrap(),
        price: 12_345_678.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            WindowSlot {
                slot: 13,
                name: "diamond_sword".to_string(),
                display_name: "Sword".to_string(),
                lore: Vec::new(),
                item_uuid: Some("wanted-uuid".to_string()),
            },
            WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Auction Price".to_string(),
                lore: vec!["Price: 12,345,678 coins".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 33,
                name: "clock".to_string(),
                display_name: "Auction Duration".to_string(),
                lore: vec!["Duration: 1 Hour".to_string()],
                item_uuid: None,
            },
        ],
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::ClickSlot { slot: 33 }
    );
}

#[test]
fn listing_workflow_clicks_custom_duration_and_types_hours() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("wanted-uuid").unwrap(),
        price: 12_345_678.0,
        hours: 12.0,
    };
    let window = WindowSnapshot {
        title: "Auction Duration".to_string(),
        slots: vec![WindowSlot {
            slot: 16,
            name: "oak_sign".to_string(),
            display_name: "Custom Duration".to_string(),
            lore: Vec::new(),
            item_uuid: None,
        }],
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::ClickSlotThenType {
            slot: 16,
            text: "12h".to_string()
        }
    );
}

#[test]
fn listing_workflow_submits_when_price_and_duration_match() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("wanted-uuid").unwrap(),
        price: 12_345_678.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            WindowSlot {
                slot: 13,
                name: "diamond_sword".to_string(),
                display_name: "Sword".to_string(),
                lore: Vec::new(),
                item_uuid: Some("wanted-uuid".to_string()),
            },
            WindowSlot {
                slot: 29,
                name: "gold_nugget".to_string(),
                display_name: "Create BIN Auction".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
            WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Auction Price".to_string(),
                lore: vec!["Price: 12,345,678 coins".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 33,
                name: "clock".to_string(),
                display_name: "Auction Duration".to_string(),
                lore: vec!["Duration: 2 Days".to_string()],
                item_uuid: None,
            },
        ],
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::ClickSlot { slot: 29 }
    );
}

#[test]
fn listing_workflow_accepts_node_style_singular_duration_label() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("wanted-uuid").unwrap(),
        price: 12_345_678.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            WindowSlot {
                slot: 13,
                name: "diamond_sword".to_string(),
                display_name: "Sword".to_string(),
                lore: Vec::new(),
                item_uuid: Some("wanted-uuid".to_string()),
            },
            WindowSlot {
                slot: 29,
                name: "gold_nugget".to_string(),
                display_name: "Create BIN Auction".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
            WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Auction Price".to_string(),
                lore: vec!["Price: 12,345,678 coins".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 33,
                name: "clock".to_string(),
                display_name: "Auction Duration".to_string(),
                lore: vec!["Duration: 2 Day".to_string()],
                item_uuid: None,
            },
        ],
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::ClickSlot { slot: 29 }
    );
}

#[test]
fn listing_workflow_switches_regular_auction_to_bin_before_submitting() {
    // Captured from the live server: the create-auction GUI opened in regular
    // (bid) auction mode, so slot 48 reads "Switch to BIN". Even though price and
    // duration already match, the bot must flip to BIN first rather than submit a
    // regular auction.
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("wanted-uuid").unwrap(),
        price: 94_200_000.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Create Auction".to_string(),
        slots: vec![
            WindowSlot {
                slot: 13,
                name: "diamond_chestplate".to_string(),
                display_name: "Ancient Skeleton Master Chestplate".to_string(),
                lore: Vec::new(),
                item_uuid: Some("wanted-uuid".to_string()),
            },
            WindowSlot {
                slot: 29,
                name: "green_terracotta".to_string(),
                display_name: "Create Auction".to_string(),
                lore: vec!["Starting bid: 94,200,000 coins".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 31,
                name: "powered_rail".to_string(),
                display_name: "Starting bid: 94,200,000 coins".to_string(),
                lore: vec!["Price: 94,200,000 coins".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 33,
                name: "clock".to_string(),
                display_name: "Duration: 2 Days".to_string(),
                lore: vec!["Duration: 2 Days".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 48,
                name: "gold_ingot".to_string(),
                display_name: "Switch to BIN".to_string(),
                lore: vec!["(BIN means Buy It Now)".to_string(), "Click to switch!".to_string()],
                item_uuid: None,
            },
        ],
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::ClickSlot { slot: 48 }
    );
}

#[test]
fn listing_workflow_submits_in_bin_mode_without_switching() {
    // The BIN-mode window (title contains "BIN", toggle reads "Switch to Auction")
    // must submit as before — the switch logic must not loop.
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("wanted-uuid").unwrap(),
        price: 12_345_678.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            WindowSlot {
                slot: 13,
                name: "diamond_sword".to_string(),
                display_name: "Sword".to_string(),
                lore: Vec::new(),
                item_uuid: Some("wanted-uuid".to_string()),
            },
            WindowSlot {
                slot: 29,
                name: "gold_nugget".to_string(),
                display_name: "Create BIN Auction".to_string(),
                lore: Vec::new(),
                item_uuid: None,
            },
            WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Auction Price".to_string(),
                lore: vec!["Price: 12,345,678 coins".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 33,
                name: "clock".to_string(),
                display_name: "Auction Duration".to_string(),
                lore: vec!["Duration: 2 Days".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 48,
                name: "gold_ingot".to_string(),
                display_name: "Switch to Auction".to_string(),
                lore: vec!["Click to switch!".to_string()],
                item_uuid: None,
            },
        ],
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::ClickSlot { slot: 29 }
    );
}

#[test]
fn listing_workflow_uses_inventory_field_for_claimed_purchase() {
    let entry = QueueEntry {
        action: json!({
            "auctionID": "auction-1",
            "inventory": "inventory-uuid",
            "price": 12_345_678,
            "time": 48
        }),
        state: BotState::Listing,
        priority: 4,
    };

    assert_eq!(
        MarketWorkflow::from_queue_entry(&entry),
        Some(MarketWorkflow::ListItem {
            item_uuid: ItemUuid::new("inventory-uuid").unwrap(),
            price: 12_345_678.0,
            hours: 48.0
        })
    );
}

#[test]
fn listing_workflow_confirms_bin_auction_window() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("item-uuid").unwrap(),
        price: 12_345_678.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Confirm BIN Auction".to_string(),
        slots: vec![WindowSlot {
            slot: 31,
            name: "gold_nugget".to_string(),
            display_name: "Confirm Auction".to_string(),
            lore: vec![
                "Price: 12,345,678 coins".to_string(),
                "Click to create BIN auction".to_string(),
            ],
            item_uuid: None,
        }],
    };

    let step = workflow.next_step(Some(&window));

    assert_eq!(step.instruction, MarketInstruction::ClickSlot { slot: 31 });
    assert!(step.done);
}

#[test]
fn listing_workflow_closes_confirmation_with_wrong_price() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("item-uuid").unwrap(),
        price: 124_500_000.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Confirm BIN Auction".to_string(),
        slots: vec![WindowSlot {
            slot: 11,
            name: "gold_nugget".to_string(),
            display_name: "Confirm Auction".to_string(),
            lore: vec![
                "Price: 805,160 coins".to_string(),
                "Click to create BIN auction".to_string(),
            ],
            item_uuid: None,
        }],
    };

    let step = workflow.next_step(Some(&window));

    assert_eq!(step.instruction, MarketInstruction::CloseWindow);
    assert!(!step.done);
}

#[test]
fn listing_workflow_accepts_compact_confirmation_price() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("item-uuid").unwrap(),
        price: 25_100_000.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Confirm BIN Auction".to_string(),
        slots: vec![WindowSlot {
            slot: 11,
            name: "gold_nugget".to_string(),
            display_name: "Confirm Auction".to_string(),
            lore: vec![
                "Price: 25.1M coins".to_string(),
                "Click to create BIN auction".to_string(),
            ],
            item_uuid: None,
        }],
    };

    let step = workflow.next_step(Some(&window));

    assert_eq!(step.instruction, MarketInstruction::ClickSlot { slot: 11 });
    assert!(step.done);
}

#[test]
fn listing_workflow_accepts_confirmation_creation_fee_when_price_is_hidden() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("item-uuid").unwrap(),
        price: 25_100_000.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Confirm BIN Auction".to_string(),
        slots: vec![WindowSlot {
            slot: 11,
            name: "lime_wool".to_string(),
            display_name: "Confirm Auction".to_string(),
            lore: vec![
                "Cost: 503,200 coins".to_string(),
                "Click to create BIN auction".to_string(),
            ],
            item_uuid: None,
        }],
    };

    let step = workflow.next_step(Some(&window));

    assert_eq!(step.instruction, MarketInstruction::ClickSlot { slot: 11 });
    assert!(step.done);
}

#[test]
fn listing_workflow_accepts_one_percent_confirmation_creation_fee_when_price_is_hidden() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("item-uuid").unwrap(),
        price: 5_600_000.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Confirm BIN Auction".to_string(),
        slots: vec![WindowSlot {
            slot: 11,
            name: "lime_wool".to_string(),
            display_name: "Confirm Auction".to_string(),
            lore: vec![
                "Cost: 57,200 coins".to_string(),
                "Click to create BIN auction".to_string(),
            ],
            item_uuid: None,
        }],
    };

    let step = workflow.next_step(Some(&window));

    assert_eq!(step.instruction, MarketInstruction::ClickSlot { slot: 11 });
    assert!(step.done);
}

#[test]
fn listing_workflow_accepts_high_value_confirmation_creation_fee_when_price_is_hidden() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("item-uuid").unwrap(),
        price: 106_500_000.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Confirm BIN Auction".to_string(),
        slots: vec![WindowSlot {
            slot: 11,
            name: "lime_wool".to_string(),
            display_name: "Confirm Auction".to_string(),
            lore: vec![
                "Cost: 2,663,700 coins".to_string(),
                "Click to create BIN auction".to_string(),
            ],
            item_uuid: None,
        }],
    };

    let step = workflow.next_step(Some(&window));

    assert_eq!(step.instruction, MarketInstruction::ClickSlot { slot: 11 });
    assert!(step.done);
}

#[test]
fn listing_workflow_rejects_confirmation_creation_fee_far_from_price() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("item-uuid").unwrap(),
        price: 124_500_000.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Confirm BIN Auction".to_string(),
        slots: vec![WindowSlot {
            slot: 11,
            name: "lime_wool".to_string(),
            display_name: "Confirm Auction".to_string(),
            lore: vec![
                "Cost: 16,103 coins".to_string(),
                "Click to create BIN auction".to_string(),
            ],
            item_uuid: None,
        }],
    };

    let step = workflow.next_step(Some(&window));

    assert_eq!(step.instruction, MarketInstruction::CloseWindow);
    assert!(!step.done);
}

#[test]
fn listing_workflow_does_not_use_submit_button_as_price_control() {
    let workflow = MarketWorkflow::ListItem {
        item_uuid: ItemUuid::new("wanted-uuid").unwrap(),
        price: 124_500_000.0,
        hours: 48.0,
    };
    let window = WindowSnapshot {
        title: "Create BIN Auction".to_string(),
        slots: vec![
            WindowSlot {
                slot: 13,
                name: "leather_leggings".to_string(),
                display_name: "Ancient Necron's Leggings".to_string(),
                lore: Vec::new(),
                item_uuid: Some("wanted-uuid".to_string()),
            },
            WindowSlot {
                slot: 29,
                name: "gold_nugget".to_string(),
                display_name: "Create BIN Auction".to_string(),
                lore: vec!["Price: 805,160 coins".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 31,
                name: "gold_ingot".to_string(),
                display_name: "Auction Price".to_string(),
                lore: vec!["Price: 805,160 coins".to_string()],
                item_uuid: None,
            },
            WindowSlot {
                slot: 33,
                name: "clock".to_string(),
                display_name: "Auction Duration".to_string(),
                lore: vec!["Duration: 2 Days".to_string()],
                item_uuid: None,
            },
        ],
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::ClickSlotThenType {
            slot: 31,
            text: "\"124500000\"".to_string()
        }
    );
}

#[test]
fn bank_workflow_types_requested_amount() {
    let workflow = MarketWorkflow::Bank {
        amount: Some(json!(50000000)),
        withdraw: true,
        personal: false,
    };
    let window = WindowSnapshot {
        title: "Bank Amount".to_string(),
        slots: Vec::new(),
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::TypeText {
            text: "\"50000000\"".to_string()
        }
    );
}

#[test]
fn bank_workflow_clicks_custom_amount_and_types_requested_amount() {
    let workflow = MarketWorkflow::Bank {
        amount: Some(json!(50000000)),
        withdraw: false,
        personal: false,
    };
    let window = WindowSnapshot {
        title: "Deposit Coins".to_string(),
        slots: vec![WindowSlot {
            slot: 15,
            name: "oak_sign".to_string(),
            display_name: "Custom Amount".to_string(),
            lore: Vec::new(),
            item_uuid: None,
        }],
    };

    let step = workflow.next_step(Some(&window));

    assert_eq!(
        step.instruction,
        MarketInstruction::ClickSlotThenType {
            slot: 15,
            text: "\"50000000\"".to_string()
        }
    );
    assert!(step.done);
}

#[test]
fn claim_sold_workflow_uses_manage_auction_claim_slot() {
    let entry = QueueEntry {
        action: json!({}),
        state: BotState::Custom("claimSold".to_string()),
        priority: 5,
    };
    let workflow = MarketWorkflow::from_queue_entry(&entry).unwrap();
    assert_eq!(workflow, MarketWorkflow::ClaimSold);

    let window = WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: vec![WindowSlot {
            slot: 29,
            name: "gold_ingot".to_string(),
            display_name: "Claim All".to_string(),
            lore: vec!["Collect sold auctions".to_string()],
            item_uuid: None,
        }],
    };

    assert_eq!(
        workflow.next_step(Some(&window)).instruction,
        MarketInstruction::ClickSlot { slot: 29 }
    );
}

#[test]
fn claim_sold_workflow_does_not_click_create_auction_as_manage_fallback() {
    let entry = QueueEntry {
        action: json!({}),
        state: BotState::Custom("claimSold".to_string()),
        priority: 5,
    };
    let workflow = MarketWorkflow::from_queue_entry(&entry).unwrap();
    let window = WindowSnapshot {
        title: "Co-op Auction House".to_string(),
        slots: vec![WindowSlot {
            slot: 15,
            name: "gold_block".to_string(),
            display_name: "Create Auction".to_string(),
            lore: vec!["Start a new auction".to_string()],
            item_uuid: None,
        }],
    };

    let step = workflow.next_step(Some(&window));

    assert_eq!(step.instruction, MarketInstruction::CloseWindow);
    assert!(step.done);
}

#[test]
fn reconcile_workflow_closes_manage_window_when_no_claimable_slots() {
    let entry = QueueEntry {
        action: json!({}),
        state: BotState::Custom("reconcileAuctions".to_string()),
        priority: 2,
    };
    let workflow = MarketWorkflow::from_queue_entry(&entry).unwrap();
    let window = WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: Vec::new(),
    };

    let step = workflow.next_step(Some(&window));

    assert_eq!(step.instruction, MarketInstruction::CloseWindow);
    assert!(step.done);
}

#[test]
fn reconcile_workflow_does_not_click_create_auction_as_manage_fallback() {
    let entry = QueueEntry {
        action: json!({}),
        state: BotState::Custom("reconcileAuctions".to_string()),
        priority: 2,
    };
    let workflow = MarketWorkflow::from_queue_entry(&entry).unwrap();
    let window = WindowSnapshot {
        title: "Co-op Auction House".to_string(),
        slots: vec![WindowSlot {
            slot: 15,
            name: "gold_block".to_string(),
            display_name: "Create Auction".to_string(),
            lore: vec!["Start a new auction".to_string()],
            item_uuid: None,
        }],
    };

    let step = workflow.next_step(Some(&window));

    assert_eq!(step.instruction, MarketInstruction::CloseWindow);
    assert!(step.done);
}

#[test]
fn expired_workflow_claims_matching_manage_slot() {
    let entry = QueueEntry {
        action: json!("expired-item-uuid"),
        state: BotState::Expired,
        priority: 4,
    };
    let workflow = MarketWorkflow::from_queue_entry(&entry).unwrap();
    assert_eq!(
        workflow,
        MarketWorkflow::ClaimExpired {
            item_uuid: ItemUuid::new("expired-item-uuid").unwrap()
        }
    );
    let window = WindowSnapshot {
        title: "Manage Auctions".to_string(),
        slots: vec![WindowSlot {
            slot: 10,
            name: "diamond_sword".to_string(),
            display_name: "Expired Sword".to_string(),
            lore: vec!["Expired!".to_string()],
            item_uuid: Some("expired-item-uuid".to_string()),
        }],
    };

    let step = workflow.next_step(Some(&window));

    assert_eq!(step.instruction, MarketInstruction::ClickSlot { slot: 10 });
    assert!(step.done);
}

#[test]
fn expired_workflow_does_not_click_create_auction_as_manage_fallback() {
    let entry = QueueEntry {
        action: json!("expired-item-uuid"),
        state: BotState::Expired,
        priority: 4,
    };
    let workflow = MarketWorkflow::from_queue_entry(&entry).unwrap();
    let window = WindowSnapshot {
        title: "Co-op Auction House".to_string(),
        slots: vec![WindowSlot {
            slot: 15,
            name: "gold_block".to_string(),
            display_name: "Create Auction".to_string(),
            lore: vec!["Start a new auction".to_string()],
            item_uuid: None,
        }],
    };

    let step = workflow.next_step(Some(&window));

    assert_eq!(step.instruction, MarketInstruction::CloseWindow);
    assert!(step.done);
}

#[test]
fn reconcile_workflow_closes_non_auction_window_without_manage_slot() {
    let entry = QueueEntry {
        action: json!({}),
        state: BotState::Custom("reconcileAuctions".to_string()),
        priority: 2,
    };
    let workflow = MarketWorkflow::from_queue_entry(&entry).unwrap();
    let window = WindowSnapshot {
        title: "Create Auction".to_string(),
        slots: vec![WindowSlot {
            slot: 15,
            name: "gold_block".to_string(),
            display_name: "Create Auction".to_string(),
            lore: vec!["Start a new auction".to_string()],
            item_uuid: None,
        }],
    };

    let step = workflow.next_step(Some(&window));

    assert_eq!(step.instruction, MarketInstruction::CloseWindow);
    assert!(step.done);
}
