use crate::ids::{AuctionId, ItemUuid};
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod listing;
mod parsing;
mod planning;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MarketWorkflow {
    BuyNow {
        auction_id: AuctionId,
    },
    ClaimPurchased {
        auction_id: AuctionId,
    },
    ClaimBids,
    ClaimSold,
    ListItem {
        item_uuid: ItemUuid,
        price: f64,
        hours: f64,
    },
    ListItemWithContext {
        item_uuid: ItemUuid,
        item_name: Option<String>,
        tag: Option<String>,
        price: f64,
        hours: f64,
    },
    Delist {
        auction_id: AuctionId,
        item_uuid: ItemUuid,
    },
    ClaimExpired {
        item_uuid: ItemUuid,
    },
    Bank {
        amount: Option<Value>,
        withdraw: bool,
        personal: bool,
    },
    ReconcileAuctions,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketStep {
    pub instruction: MarketInstruction,
    pub done: bool,
    pub reason: String,
}

impl MarketStep {
    pub(super) fn next(instruction: MarketInstruction, reason: impl Into<String>) -> Self {
        Self {
            instruction,
            done: false,
            reason: reason.into(),
        }
    }

    pub(super) fn finish(instruction: MarketInstruction, reason: impl Into<String>) -> Self {
        Self {
            instruction,
            done: true,
            reason: reason.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum MarketInstruction {
    Noop,
    Chat { message: String },
    OpenAuction { auction_id: AuctionId },
    ClickSlot { slot: usize },
    ClickSlotThenType { slot: usize, text: String },
    CloseWindow,
    TypeText { text: String },
}

#[cfg(test)]
mod tests;
