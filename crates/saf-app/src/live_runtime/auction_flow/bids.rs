use super::window::find_labeled_slot_value;
use crate::live_runtime::support::{number_value, string_value, strip_minecraft_color_codes};
use saf_core::gui::WindowSlot;
use saf_core::{BotState, QueueEntry};
use serde_json::Value;
use std::time::Instant;

#[derive(Clone, Debug)]
pub(in crate::live_runtime) struct PendingClaimedBidRelist {
    pub(in crate::live_runtime) account: saf_core::AccountId,
    pub(in crate::live_runtime) item_uuid: String,
    pub(in crate::live_runtime) listing_action: Value,
    pub(in crate::live_runtime) ready_at: Instant,
}

#[derive(Clone, Debug)]
pub(in crate::live_runtime) struct ClaimablePurchasedBid {
    pub(in crate::live_runtime) slot: usize,
    pub(in crate::live_runtime) item_uuid: String,
    pub(in crate::live_runtime) listing_action: Value,
}

pub(in crate::live_runtime) fn is_claimed_bid_listing_entry(
    entry: &QueueEntry,
    relist: &PendingClaimedBidRelist,
) -> bool {
    matches!(entry.state, BotState::Listing | BotState::ListingNoName)
        && string_value(&entry.action, &["inventory", "inv"]).as_deref()
            == Some(relist.item_uuid.as_str())
}

pub(in crate::live_runtime) fn is_purchased_bid_slot(slot: &WindowSlot) -> bool {
    let text = slot.text();
    [
        "click to claim",
        "claim item",
        "you won",
        "you bought",
        "you purchased",
        "status: won",
        "status: purchased",
        "status: ended",
    ]
    .iter()
    .any(|pattern| text.contains(pattern))
}

pub(in crate::live_runtime) fn purchased_bid_listing_action(
    item_uuid: &str,
    bid_data: &Value,
    slot: &WindowSlot,
) -> Option<Value> {
    let auction_id = string_value(bid_data, &["auctionID", "auctionId", "auction_id"])
        .unwrap_or_else(|| item_uuid.to_string());
    let target = number_value(bid_data, &["target"])?;
    if !target.is_finite() || target < 500.0 {
        return None;
    }
    let fallback_name = if slot.display_name.trim().is_empty() {
        auction_id.clone()
    } else {
        strip_minecraft_color_codes(&slot.display_name)
    };
    let item_name = string_value(bid_data, &["itemName"]).unwrap_or_else(|| fallback_name.clone());
    let weird_item_name =
        string_value(bid_data, &["weirdItemName"]).unwrap_or_else(|| fallback_name.clone());
    let tag = string_value(bid_data, &["tag"])
        .or_else(|| find_labeled_slot_value(slot, &["tag", "item tag", "skyblock id"]));
    let profit = number_value(bid_data, &["profit"]).unwrap_or(0.0);
    let finder = string_value(bid_data, &["finder"]).unwrap_or_else(|| "UNKNOWN".to_string());

    Some(serde_json::json!({
        "profit": profit,
        "finder": finder,
        "itemName": item_name,
        "tag": tag,
        "auctionID": auction_id,
        "price": target.round() as u64,
        "weirdItemName": weird_item_name,
        "inventory": item_uuid
    }))
}
