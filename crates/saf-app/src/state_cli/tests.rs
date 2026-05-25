use super::*;
use saf_core::AccountId;
use saf_core::ports::{QueueStore, TrackedFlipProvider};
use serde_json::json;
use std::fs;

#[test]
fn queue_mutations_use_node_saved_data_layout() {
    let temp = tempfile::tempdir().unwrap();
    let added = add_queue_entry(
        temp.path(),
        "account",
        json!({"auctionID": "auction-1"}),
        BotState::Listing,
        4,
        true,
    )
    .unwrap();
    assert!(added.changed);
    assert!(added.saved);
    assert_eq!(added.snapshot.queue.len(), 1);

    let restored = snapshot(temp.path(), "account").unwrap();
    assert_eq!(restored.queue.len(), 1);
    assert_eq!(restored.queue[0].state, BotState::Listing);

    let removed = remove_next(temp.path(), "account", true).unwrap();
    assert!(removed.changed);
    assert_eq!(removed.removed.unwrap().state, BotState::Listing);
    assert!(snapshot(temp.path(), "account").unwrap().queue.is_empty());
}

#[test]
fn queue_mutations_persist_custom_states_for_operator_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let added = add_queue_entry(
        temp.path(),
        "account",
        json!({"auctionID": "auction-1"}),
        BotState::Custom("claimPurchased".to_string()),
        1,
        true,
    )
    .unwrap();

    assert!(added.changed);
    assert!(added.saved);

    let restored = snapshot(temp.path(), "account").unwrap();
    assert_eq!(restored.queue.len(), 1);
    assert_eq!(
        restored.queue[0].state,
        BotState::Custom("claimPurchased".to_string())
    );
}

#[test]
fn clear_queue_removes_every_entry() {
    let temp = tempfile::tempdir().unwrap();
    add_queue_entry(
        temp.path(),
        "account",
        json!({"auctionID": "auction-1"}),
        BotState::Listing,
        4,
        true,
    )
    .unwrap();
    add_queue_entry(
        temp.path(),
        "account",
        json!({"auctionID": "auction-2"}),
        BotState::ListingNoName,
        4,
        true,
    )
    .unwrap();

    let report = clear_queue(temp.path(), "account", true).unwrap();
    assert_eq!(report.removed, 2);
    assert!(report.snapshot.queue.is_empty());
    assert!(snapshot(temp.path(), "account").unwrap().queue.is_empty());
}

#[test]
fn clear_saved_data_removes_queue_and_bid_data() {
    let temp = tempfile::tempdir().unwrap();
    let saved_dir = temp.path().join("SavedData");
    fs::create_dir_all(&saved_dir).unwrap();
    fs::write(
        saved_dir.join("account.json"),
        serde_json::to_vec(&json!({
            "bidData": {"auction-1": {"target": 10_000_000}},
            "queue": [
                {"action": {"auctionID": "auction-1"}, "state": "listing", "priority": 4},
                {"action": {"auctionID": "auction-2"}, "state": "listingNoName", "priority": 4}
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let report = clear_saved_data(temp.path(), "account", true).unwrap();
    assert_eq!(report.queue_removed, 2);
    assert!(report.bid_data_cleared);
    assert!(report.snapshot.queue.is_empty());
    assert_eq!(report.snapshot.bid_data, json!({}));

    let restored = snapshot(temp.path(), "account").unwrap();
    assert!(restored.queue.is_empty());
    assert_eq!(restored.bid_data, json!({}));
}

#[test]
fn duplicate_reconciliation_entries_are_reported_as_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let saved_dir = temp.path().join("SavedData");
    std::fs::create_dir_all(&saved_dir).unwrap();
    std::fs::write(
        saved_dir.join("account.json"),
        serde_json::to_vec(&json!({
            "bidData": {},
            "queue": [
                {"action": {"reason": "one"}, "state": "claimSold", "priority": 2}
            ]
        }))
        .unwrap(),
    )
    .unwrap();

    let duplicate = add_queue_entry(
        temp.path(),
        "account",
        json!({"reason": "two"}),
        BotState::Custom("reconcileAuctions".to_string()),
        2,
        false,
    )
    .unwrap();

    assert!(!duplicate.changed);
    assert_eq!(duplicate.snapshot.queue.len(), 1);
}

#[tokio::test]
async fn file_queue_store_persists_through_state_store() {
    let temp = tempfile::tempdir().unwrap();
    let store = FileQueueStore::new(temp.path());
    let account = AccountId::new("account").unwrap();

    let changed = store
        .add(
            &account,
            json!({"amount": 50_000_000}),
            BotState::Custom("bank".to_string()),
            5,
        )
        .await
        .unwrap();

    assert!(changed);
    let restored = snapshot(temp.path(), "account").unwrap();
    assert_eq!(restored.queue.len(), 1);
    assert_eq!(
        restored.queue[0].state,
        BotState::Custom("bank".to_string())
    );

    assert_eq!(store.snapshot(&account).await.unwrap().len(), 1);
    assert_eq!(store.clear(&account).await.unwrap(), 1);
    assert!(store.snapshot(&account).await.unwrap().is_empty());
}

#[tokio::test]
async fn file_queue_store_reads_tracked_flips_from_bid_data() {
    let temp = tempfile::tempdir().unwrap();
    let saved_dir = temp.path().join("SavedData");
    fs::create_dir_all(&saved_dir).unwrap();
    fs::write(
        saved_dir.join("account.json"),
        serde_json::to_vec(&json!({
            "bidData": {
                "item-uuid": {
                    "auctionID": "auction-1",
                    "target": "56.3m",
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
    let store = FileQueueStore::new(temp.path());
    let account = AccountId::new("account").unwrap();

    let tracked = store.lookup(&account, "auction-1").await.unwrap().unwrap();

    assert_eq!(tracked.auction_id, "auction-1");
    assert_eq!(tracked.target_price, 56_300_000.0);
    assert_eq!(
        tracked.weird_item_name.as_deref(),
        Some("Ancient Necron's Leggings")
    );
    assert_eq!(tracked.tag.as_deref(), Some("NECRON_LEGGINGS"));
    assert_eq!(tracked.price_paid, Some(31_000_000.0));
}

#[tokio::test]
async fn file_queue_store_moves_claimed_bid_data_into_listing_queue() {
    let temp = tempfile::tempdir().unwrap();
    let saved_dir = temp.path().join("SavedData");
    fs::create_dir_all(&saved_dir).unwrap();
    fs::write(
        saved_dir.join("account.json"),
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
    let store = FileQueueStore::new(temp.path());
    let account = AccountId::new("account").unwrap();

    let changed = store
        .queue_claimed_bid_relist(
            &account,
            "item-uuid",
            json!({"auctionID": "auction-1", "inventory": "item-uuid", "price": 56_300_000}),
        )
        .await
        .unwrap();

    assert!(changed);
    let restored = snapshot(temp.path(), "account").unwrap();
    assert_eq!(restored.bid_data, json!({}));
    assert_eq!(restored.queue.len(), 2);
    assert_eq!(restored.queue[1].state, BotState::Listing);
}
