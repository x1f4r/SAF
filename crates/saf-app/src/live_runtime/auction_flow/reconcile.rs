use super::window::find_labeled_slot_value;
use crate::live_runtime::support::strip_minecraft_color_codes;
use crate::live_runtime::windows::is_manage_auctions_window;
use saf_core::gui::{WindowSlot, WindowSnapshot};
use saf_core::{AccountId, ExpiredRelistPricing, ItemUuid, QueueEntry};
use serde_json::Value;

pub(in crate::live_runtime) struct AuctionReconcileAction {
    pub(in crate::live_runtime) claim_slot: usize,
    pub(in crate::live_runtime) claim_all: bool,
    pub(in crate::live_runtime) sold_count: usize,
    pub(in crate::live_runtime) expired_count: usize,
    pub(in crate::live_runtime) claimed_expired: Vec<ExpiredAuctionReconcile>,
}

impl AuctionReconcileAction {
    pub(in crate::live_runtime) fn reason(&self) -> String {
        match (self.sold_count, self.expired_count) {
            (0, 0) => "auction reconciliation claim".to_string(),
            (sold, 0) => format!("claim {sold} sold auction{}", plural_suffix(sold)),
            (0, expired) => {
                format!("claim {expired} expired auction{}", plural_suffix(expired))
            }
            (sold, expired) => format!(
                "claim {sold} sold and {expired} expired auction{}",
                plural_suffix(expired)
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(in crate::live_runtime) struct ExpiredAuctionReconcile {
    pub(in crate::live_runtime) item_uuid: ItemUuid,
    name: String,
    pub(in crate::live_runtime) tag: Option<String>,
    pub(in crate::live_runtime) old_price: f64,
}

impl ExpiredAuctionReconcile {
    pub(in crate::live_runtime) fn queue_action(
        &self,
        account: &AccountId,
        pricing: &ExpiredRelistPricing,
    ) -> Option<Value> {
        let price = pricing
            .resolve(
                self.old_price,
                Err("live NBT price is unavailable during reconciliation".to_string()),
            )
            .ok()?;
        if !price.is_finite() || price < 500.0 {
            return None;
        }
        let item_uuid = self.item_uuid.to_string();
        Some(serde_json::json!({
            "price": price.round() as u64,
            "auctionID": item_uuid.clone(),
            "username": account.as_str(),
            "inv": item_uuid,
            "weirdItemName": self.name.clone(),
            "tag": self.tag.clone(),
            "pricePaid": 0,
            "oldPrice": self.old_price.max(0.0).round() as u64
        }))
    }
}

pub(in crate::live_runtime) fn reconcile_auction_window(
    account: &AccountId,
    window: &WindowSnapshot,
    angry_coop_prevention: bool,
) -> Option<AuctionReconcileAction> {
    if !is_manage_auctions_window(window) {
        return None;
    }

    let mut claim_all_slot = None;
    let mut sold_slots = Vec::new();
    let mut expired = Vec::new();
    let account_lower = account.as_str().to_ascii_lowercase();
    for slot in &window.slots {
        if is_claim_all_slot(slot) {
            claim_all_slot = Some(slot.slot);
        }

        let text = slot.text();
        let has_seller = text.contains("seller:");
        let is_sold = is_sold_auction_slot(slot);
        let is_expired = text.contains("expired");
        if !has_seller && !is_sold && !is_expired {
            continue;
        }

        let sold_by_coop = seller_line(slot)
            .map(|line| !line.to_ascii_lowercase().contains(&account_lower))
            .unwrap_or(false);
        if is_sold {
            if angry_coop_prevention && sold_by_coop {
                continue;
            }
            tracing::info!(
                account = %account,
                slot = slot.slot,
                name = %slot.name,
                display_name = %slot.display_name,
                lore = ?slot.lore,
                sold_by_coop,
                "auction reconciliation detected sold auction slot"
            );
            sold_slots.push(slot.slot);
            continue;
        }

        if is_expired && let Some(expired_auction) = expired_auction_from_slot(slot) {
            expired.push((slot.slot, expired_auction));
        }
    }

    let sold_count = sold_slots.len();
    let expired_count = expired.len();
    let (claim_slot, claim_all, claimed_expired) = if let Some(claim_all) = claim_all_slot {
        if sold_count == 0 && expired_count == 0 {
            return None;
        }
        (
            claim_all,
            true,
            expired
                .iter()
                .map(|(_, auction)| auction.clone())
                .collect::<Vec<_>>(),
        )
    } else if let Some(slot) = sold_slots.first() {
        (*slot, false, Vec::new())
    } else if let Some((slot, auction)) = expired.first() {
        (*slot, false, vec![auction.clone()])
    } else {
        return None;
    };

    Some(AuctionReconcileAction {
        claim_slot,
        claim_all,
        sold_count,
        expired_count,
        claimed_expired,
    })
}

pub(in crate::live_runtime) fn sold_claim_action_slot(window: &WindowSnapshot) -> Option<usize> {
    let title = window.title.to_ascii_lowercase();
    if !title.contains("auction view") && !title.contains("view auction") {
        return None;
    }
    let slot = window.auction_action_slot(31);
    window
        .slots
        .iter()
        .find(|candidate| candidate.slot == slot)
        .filter(|candidate| {
            let text = candidate.text();
            candidate.name == "gold_block"
                || ((text.contains("claim") || text.contains("collect"))
                    && (text.contains("coin") || text.contains("auction")))
        })
        .map(|candidate| candidate.slot)
}

pub(in crate::live_runtime) fn expired_claim_from_window(
    entry: &QueueEntry,
    window: &WindowSnapshot,
) -> Option<(usize, Option<ExpiredAuctionReconcile>)> {
    let item_uuid = expired_entry_item_uuid(entry)?;
    let slot = window
        .slots
        .iter()
        .find(|slot| slot.item_uuid.as_deref() == Some(item_uuid.as_str()))?;
    Some((slot.slot, expired_auction_from_slot(slot)))
}

fn expired_entry_item_uuid(entry: &QueueEntry) -> Option<String> {
    entry
        .action
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .or_else(|| {
            crate::live_runtime::support::string_value(
                &entry.action,
                &["itemUuid", "itemUUID", "inv", "auctionID"],
            )
        })
}

fn is_claim_all_slot(slot: &WindowSlot) -> bool {
    slot.name == "cauldron" || slot.text().contains("claim all")
}

fn is_sold_auction_slot(slot: &WindowSlot) -> bool {
    let text = slot.text();
    text.contains("buyer:")
        && [
            "sold",
            "claim",
            "collect",
            "auction ended",
            "ended!",
            "completed",
        ]
        .iter()
        .any(|marker| text.contains(marker))
}

fn seller_line(slot: &WindowSlot) -> Option<String> {
    slot.lore
        .iter()
        .map(|line| strip_minecraft_color_codes(line))
        .find(|line| line.to_ascii_lowercase().contains("seller:"))
}

fn expired_auction_from_slot(slot: &WindowSlot) -> Option<ExpiredAuctionReconcile> {
    let item_uuid = slot.item_uuid.as_deref().and_then(ItemUuid::new)?;
    let old_price = slot
        .lore
        .iter()
        .find(|line| {
            let line = strip_minecraft_color_codes(line).to_ascii_lowercase();
            line.contains("buy it now") || line.contains("starting bid")
        })
        .and_then(|line| {
            let line = strip_minecraft_color_codes(line);
            let value = line
                .split_once(':')
                .map(|(_, value)| value)
                .unwrap_or(&line);
            saf_core::relist::parse_old_price_from_lore_line(value)
        })?;
    Some(ExpiredAuctionReconcile {
        item_uuid,
        name: if slot.display_name.trim().is_empty() {
            slot.name.clone()
        } else {
            strip_minecraft_color_codes(&slot.display_name)
        },
        tag: find_labeled_slot_value(slot, &["tag", "item tag", "skyblock id"]),
        old_price,
    })
}

fn plural_suffix(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}
