use crate::live_runtime::stats::SoldStatsUpdate;
use saf_core::{BotState, MarketInstruction, QueueEntry};
use serde_json::Value;

pub(in crate::live_runtime) fn is_reconcile_queue_entry(entry: &QueueEntry) -> bool {
    is_reconcile_state(&entry.state)
}

pub(in crate::live_runtime) fn is_reconcile_state(state: &BotState) -> bool {
    matches!(state, BotState::Custom(name) if name == "reconcileAuctions" || name == "claimSold")
}

pub(in crate::live_runtime) fn is_bids_queue_entry(entry: &QueueEntry) -> bool {
    is_bids_state(&entry.state)
}

pub(in crate::live_runtime) fn is_bids_state(state: &BotState) -> bool {
    matches!(state, BotState::Custom(name) if name == "bids")
}

pub(in crate::live_runtime) fn is_bank_queue_entry(entry: &QueueEntry) -> bool {
    matches!(&entry.state, BotState::Custom(name) if name == "bank")
}

pub(in crate::live_runtime) fn is_open_bank_instruction(instruction: &MarketInstruction) -> bool {
    matches!(instruction, MarketInstruction::Chat { message } if message.trim().eq_ignore_ascii_case("/bank"))
}

pub(in crate::live_runtime) fn is_expired_queue_entry(entry: &QueueEntry) -> bool {
    matches!(entry.state, BotState::Expired)
}

pub(in crate::live_runtime) fn should_recover_pending_draft(entry: &QueueEntry) -> bool {
    match crate::live_runtime::support::string_value(&entry.action, &["reason"]).as_deref() {
        None
        | Some("")
        | Some("manual")
        | Some("manual-discord")
        | Some("listing-status-unclear")
        | Some("slot-pressure") => true,
        Some(_) => false,
    }
}

pub(in crate::live_runtime) fn sold_reconcile_action(sold: &SoldStatsUpdate) -> Value {
    serde_json::json!({
        "reason": "sold-message",
        "item": sold.item_name,
        "price": sold.price,
        "buyer": sold.buyer,
    })
}
