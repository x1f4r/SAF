use super::listing::{plan_list_item, plan_list_item_with_context};
use super::*;
use crate::gui::WindowSnapshot;
use serde_json::Value;

impl MarketWorkflow {
    pub fn next_step(&self, window: Option<&WindowSnapshot>) -> MarketStep {
        match self {
            Self::BuyNow { auction_id } => plan_buy_now(auction_id, window),
            Self::ClaimPurchased { auction_id } => plan_claim_purchased(auction_id, window),
            Self::ClaimBids => plan_claim_bids(window),
            Self::ClaimSold => plan_claim_sold(window),
            Self::ListItem {
                item_uuid,
                price,
                hours,
            } => plan_list_item(item_uuid, *price, *hours, window),
            Self::ListItemWithContext {
                item_uuid,
                item_name,
                tag,
                price,
                hours,
            } => plan_list_item_with_context(
                item_uuid,
                item_name.as_deref(),
                tag.as_deref(),
                *price,
                *hours,
                window,
            ),
            Self::Delist { auction_id, .. } => plan_delist(auction_id, window),
            Self::ClaimExpired { item_uuid } => plan_claim_expired(item_uuid, window),
            Self::Bank {
                amount,
                withdraw,
                personal,
            } => plan_bank(amount, *withdraw, *personal, window),
            Self::ReconcileAuctions => plan_reconcile(window),
        }
    }
}

fn plan_buy_now(auction_id: &AuctionId, window: Option<&WindowSnapshot>) -> MarketStep {
    let Some(window) = window else {
        return MarketStep::next(
            MarketInstruction::OpenAuction {
                auction_id: auction_id.clone(),
            },
            "open auction",
        );
    };
    let text = window_text(window);
    if text.contains("already bought")
        || text.contains("too late")
        || text.contains("not enough coins")
        || text.contains("auction ended")
    {
        return MarketStep::finish(MarketInstruction::CloseWindow, "terminal auction state");
    }
    MarketStep::finish(
        MarketInstruction::ClickSlot {
            slot: window.auction_action_slot(31),
        },
        "click auction action",
    )
}

fn plan_claim_purchased(auction_id: &AuctionId, window: Option<&WindowSnapshot>) -> MarketStep {
    let Some(window) = window else {
        return MarketStep::next(
            MarketInstruction::OpenAuction {
                auction_id: auction_id.clone(),
            },
            "open purchased auction",
        );
    };
    let title = window.title.to_ascii_lowercase();
    if !title.contains("auction view") && !title.contains("view auction") {
        return MarketStep::next(
            MarketInstruction::OpenAuction {
                auction_id: auction_id.clone(),
            },
            "open purchased auction",
        );
    }
    let text = window_text(window);
    if text.contains("already claimed") || text.contains("auction not found") {
        return MarketStep::finish(
            MarketInstruction::CloseWindow,
            "purchased auction is not claimable",
        );
    }
    if let Some(slot) = purchased_auction_claim_slot(window) {
        return MarketStep::finish(
            MarketInstruction::ClickSlot { slot },
            "claim purchased auction item",
        );
    }
    MarketStep::next(
        MarketInstruction::Noop,
        "wait for purchased auction claim slot",
    )
}

fn purchased_auction_claim_slot(window: &WindowSnapshot) -> Option<usize> {
    window
        .slots
        .iter()
        .find(|slot| {
            let text = slot.text();
            let claim_text = (text.contains("claim") || text.contains("collect"))
                && (text.contains("item") || text.contains("auction"));
            slot.name == "gold_block" || claim_text
        })
        .map(|slot| slot.slot)
}

fn plan_claim_bids(window: Option<&WindowSnapshot>) -> MarketStep {
    let Some(window) = window else {
        return MarketStep::next(
            MarketInstruction::Chat {
                message: "/ah".to_string(),
            },
            "open auction house",
        );
    };
    let title = window.title.to_ascii_lowercase();
    if title.contains("bids") {
        return MarketStep::finish(
            MarketInstruction::ClickSlot {
                slot: window.auction_action_slot(31),
            },
            "claim bid slot",
        );
    }
    MarketStep::next(
        MarketInstruction::ClickSlot {
            slot: window
                .resolve_slot(&["bids", "manage bids"], Some(13))
                .unwrap_or(13),
        },
        "open bids",
    )
}

fn plan_claim_sold(window: Option<&WindowSnapshot>) -> MarketStep {
    let Some(window) = window else {
        return MarketStep::next(
            MarketInstruction::Chat {
                message: "/ah".to_string(),
            },
            "open auction house",
        );
    };
    let title = window.title.to_ascii_lowercase();
    if title.contains("manage") || title.contains("your auctions") {
        return MarketStep::finish(
            MarketInstruction::ClickSlot {
                slot: window
                    .resolve_slot(
                        &["claim all", "claim sold", "collect all", "collect auctions"],
                        Some(31),
                    )
                    .unwrap_or(31),
            },
            "claim sold auctions",
        );
    }
    open_auction_management_step(window)
}

fn plan_delist(auction_id: &AuctionId, window: Option<&WindowSnapshot>) -> MarketStep {
    let Some(window) = window else {
        return MarketStep::next(
            MarketInstruction::OpenAuction {
                auction_id: auction_id.clone(),
            },
            "open auction before delist",
        );
    };
    MarketStep::finish(
        MarketInstruction::ClickSlot {
            slot: window.auction_action_slot(31),
        },
        "click delist action",
    )
}

fn plan_claim_expired(item_uuid: &ItemUuid, window: Option<&WindowSnapshot>) -> MarketStep {
    let Some(window) = window else {
        return MarketStep::next(
            MarketInstruction::Chat {
                message: "/ah".to_string(),
            },
            "open auction house",
        );
    };
    let title = window.title.to_ascii_lowercase();
    if title.contains("manage") || title.contains("your auctions") {
        if let Some(slot) = window
            .slots
            .iter()
            .find(|slot| slot.item_uuid.as_deref() == Some(item_uuid.as_str()))
            .map(|slot| slot.slot)
        {
            return MarketStep::finish(
                MarketInstruction::ClickSlot { slot },
                "claim expired auction",
            );
        }
        return MarketStep::finish(MarketInstruction::CloseWindow, "expired auction not found");
    }
    open_auction_management_step(window)
}

fn plan_bank(
    amount: &Option<Value>,
    withdraw: bool,
    personal: bool,
    window: Option<&WindowSnapshot>,
) -> MarketStep {
    let Some(window) = window else {
        return MarketStep::next(
            MarketInstruction::Chat {
                message: "/bank".to_string(),
            },
            "open bank",
        );
    };
    let title = window.title.to_ascii_lowercase();
    if title.contains("amount") {
        return MarketStep::finish(
            MarketInstruction::TypeText {
                text: quoted_sign_text(bank_amount_text(amount)),
            },
            "enter bank amount",
        );
    }
    let account_patterns = if personal {
        ["personal", "private"]
    } else {
        ["shared", "co-op"]
    };
    let amount_patterns = ["custom amount", "specific amount", "custom"];
    let amount_fallback = if withdraw { 16 } else { 15 };
    if let Some(slot) = window.resolve_slot(&amount_patterns, None) {
        return MarketStep::finish(
            MarketInstruction::ClickSlotThenType {
                slot,
                text: quoted_sign_text(bank_amount_text(amount)),
            },
            "enter bank amount",
        );
    }
    if title.contains("withdraw") || title.contains("deposit") {
        return MarketStep::finish(
            MarketInstruction::ClickSlotThenType {
                slot: amount_fallback,
                text: quoted_sign_text(bank_amount_text(amount)),
            },
            "enter bank amount",
        );
    }
    if !title.contains("bank") {
        return MarketStep::next(
            MarketInstruction::Chat {
                message: "/bank".to_string(),
            },
            "return to bank",
        );
    }
    if title.trim() == "bank" {
        let account_fallback = if personal { 15 } else { 11 };
        return MarketStep::next(
            MarketInstruction::ClickSlot {
                slot: window
                    .resolve_slot(&account_patterns, Some(account_fallback))
                    .unwrap_or(account_fallback),
            },
            "choose bank account",
        );
    }
    let action_patterns = if withdraw {
        ["withdraw", "take coins"]
    } else {
        ["deposit", "put coins"]
    };
    let action_fallback = if withdraw { 13 } else { 11 };
    MarketStep::next(
        MarketInstruction::ClickSlot {
            slot: window
                .resolve_slot(&action_patterns, Some(action_fallback))
                .unwrap_or(action_fallback),
        },
        "choose bank action",
    )
}

fn plan_reconcile(window: Option<&WindowSnapshot>) -> MarketStep {
    let Some(window) = window else {
        return MarketStep::next(
            MarketInstruction::Chat {
                message: "/ah".to_string(),
            },
            "open auction house",
        );
    };
    if window.title.to_ascii_lowercase().contains("manage") {
        return MarketStep::finish(
            MarketInstruction::CloseWindow,
            "auction management reconciled",
        );
    }
    open_auction_management_step(window)
}

fn open_auction_management_step(window: &WindowSnapshot) -> MarketStep {
    if let Some(slot) = window.resolve_slot(
        &[
            "manage auctions",
            "manage auction",
            "your auctions",
            "view auctions",
        ],
        None,
    ) {
        return MarketStep::next(
            MarketInstruction::ClickSlot { slot },
            "open auction management",
        );
    }
    MarketStep::finish(
        MarketInstruction::CloseWindow,
        "auction management unavailable",
    )
}

fn value_to_text(value: &Value) -> String {
    value
        .as_str()
        .map(ToString::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn bank_amount_text(amount: &Option<Value>) -> String {
    amount
        .as_ref()
        .map(value_to_text)
        .unwrap_or_else(|| "all".to_string())
}

fn window_text(window: &WindowSnapshot) -> String {
    std::iter::once(window.title.as_str())
        .chain(window.slots.iter().flat_map(|slot| {
            std::iter::once(slot.display_name.as_str()).chain(slot.lore.iter().map(String::as_str))
        }))
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase()
}

fn quoted_sign_text(value: impl ToString) -> String {
    format!("\"{}\"", value.to_string())
}
