use super::*;
use crate::state::{BotState, QueueEntry};
use serde_json::Value;

impl MarketWorkflow {
    pub fn from_queue_entry(entry: &QueueEntry) -> Option<Self> {
        match &entry.state {
            BotState::Buying => auction_id(&entry.action).and_then(|auction_id| {
                Some(Self::BuyNow {
                    auction_id: AuctionId::new(auction_id)?,
                })
            }),
            BotState::Listing | BotState::ListingNoName => {
                let item_uuid = item_uuid(&entry.action)?;
                let price = number_field(&entry.action, "price")?;
                let hours = number_field(&entry.action, "time").unwrap_or(48.0);
                let item_uuid = ItemUuid::new(item_uuid)?;
                let item_name = string_field(&entry.action, "itemName")
                    .or_else(|| string_field(&entry.action, "item_name"));
                let tag = string_field(&entry.action, "tag");
                if item_name.is_some() || tag.is_some() {
                    Some(Self::ListItemWithContext {
                        item_uuid,
                        item_name,
                        tag,
                        price,
                        hours,
                    })
                } else {
                    Some(Self::ListItem {
                        item_uuid,
                        price,
                        hours,
                    })
                }
            }
            BotState::Delisting => Some(Self::Delist {
                auction_id: AuctionId::new(auction_id(&entry.action)?)?,
                item_uuid: ItemUuid::new(item_uuid(&entry.action)?)?,
            }),
            BotState::Expired => Some(Self::ClaimExpired {
                item_uuid: ItemUuid::new(item_uuid(&entry.action)?)?,
            }),
            BotState::Custom(name) if name == "externalBuying" => auction_id(&entry.action)
                .and_then(|auction_id| {
                    Some(Self::BuyNow {
                        auction_id: AuctionId::new(auction_id)?,
                    })
                }),
            BotState::Custom(name) if name == "claimPurchased" => auction_id(&entry.action)
                .and_then(|auction_id| {
                    Some(Self::ClaimPurchased {
                        auction_id: AuctionId::new(auction_id)?,
                    })
                }),
            BotState::Custom(name) if name == "bank" => Some(Self::Bank {
                amount: entry.action.get("amount").cloned(),
                withdraw: bool_field(&entry.action, "withdraw"),
                personal: bool_field(&entry.action, "personal"),
            }),
            BotState::Custom(name) if name == "bids" => Some(Self::ClaimBids),
            BotState::Custom(name) if name == "claimSold" => Some(Self::ClaimSold),
            BotState::Custom(name) if name == "reconcileAuctions" => Some(Self::ReconcileAuctions),
            _ => None,
        }
    }
}

fn auction_id(action: &Value) -> Option<String> {
    string_field(action, "auctionID").or_else(|| string_field(action, "auction_id"))
}

fn item_uuid(action: &Value) -> Option<String> {
    if let Some(value) = action.as_str() {
        let value = value.trim();
        if !value.is_empty() {
            return Some(value.to_string());
        }
    }
    string_field(action, "itemUuid")
        .or_else(|| string_field(action, "itemUUID"))
        .or_else(|| string_field(action, "inv"))
        .or_else(|| string_field(action, "inventory"))
        .or_else(|| auction_id(action))
}

fn string_field(action: &Value, key: &str) -> Option<String> {
    action
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

fn number_field(action: &Value, key: &str) -> Option<f64> {
    action
        .get(key)
        .and_then(|value| value.as_f64().or_else(|| value.as_str()?.parse().ok()))
        .filter(|value| value.is_finite())
}

fn bool_field(action: &Value, key: &str) -> bool {
    action.get(key).and_then(Value::as_bool).unwrap_or(false)
}
