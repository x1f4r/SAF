use crate::ids::AuctionId;
use crate::numbers::parse_number_input;
use crate::time::{parse_timestamp_millis, unix_millis};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlipEvent {
    pub auction_id: Option<AuctionId>,
    pub item_name: String,
    pub finder: String,
    pub tag: Option<String>,
    pub volume: f64,
    pub starting_bid: f64,
    pub target: f64,
    pub profit: f64,
    pub profit_percentage: f64,
    pub weird_item_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purchase_at_ms: Option<u64>,
    pub invalid_reason: Option<String>,
}

impl FlipEvent {
    pub fn from_payload(payload: &Value) -> Self {
        let auction_id =
            first_string(payload, &["id", "auctionID", "auctionId"]).and_then(AuctionId::new);
        let item_name = first_string(payload, &["itemName"])
            .or_else(|| auction_id.as_ref().map(ToString::to_string))
            .unwrap_or_else(|| "Unknown item".to_string());
        let finder = first_string(payload, &["finder"]).unwrap_or_else(|| "UNKNOWN".to_string());
        let tag = first_string(payload, &["tag"]);
        let volume = first_number(payload, &["vol", "volume"]).unwrap_or(0.0);
        let starting_bid = first_number(payload, &["startingBid"]).unwrap_or(f64::NAN);
        let target = first_number(payload, &["target"]).unwrap_or(f64::NAN);
        let profit = ihate_taxes(target) - starting_bid;
        let profit_percentage = first_number(payload, &["profitPerc"]).unwrap_or_else(|| {
            if starting_bid.is_finite() && starting_bid > 0.0 {
                profit / starting_bid * 100.0
            } else {
                0.0
            }
        });
        let purchase_at_ms = first_timestamp_millis(payload, &["purchaseAt"]);
        let weird_item_name = strip_item_name(&item_name);

        let invalid_reason = if auction_id.is_none() {
            Some("missing auction id".to_string())
        } else if !starting_bid.is_finite() || starting_bid <= 0.0 {
            Some(format!(
                "invalid starting bid {}",
                field_text(payload, "startingBid")
            ))
        } else if !target.is_finite() || target <= 0.0 {
            Some(format!("invalid target {}", field_text(payload, "target")))
        } else if !profit.is_finite() || profit <= 0.0 {
            Some(format!("non-profitable flip {profit}"))
        } else {
            None
        };

        Self {
            auction_id,
            item_name,
            finder,
            tag,
            volume,
            starting_bid,
            target,
            profit,
            profit_percentage,
            weird_item_name,
            purchase_at_ms,
            invalid_reason,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.invalid_reason.is_none()
    }

    pub fn is_future_purchase(&self, now_ms: u64) -> bool {
        self.purchase_at_ms
            .is_some_and(|purchase_at_ms| purchase_at_ms > now_ms)
    }

    pub fn is_timed_bed(&self) -> bool {
        self.is_future_purchase(unix_millis())
    }
}

pub fn ihate_taxes(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }

    ihate_claiming_taxes(value) - (value * creation_fee_rate(value))
}

pub fn ihate_claiming_taxes(value: f64) -> f64 {
    if !value.is_finite() {
        return value;
    }
    if value < 1_000_000.0 {
        return value;
    }
    if value * 0.99 < 1_000_000.0 {
        return 1_000_000.0;
    }
    value * 0.99
}

fn creation_fee_rate(value: f64) -> f64 {
    if value < 10_000_000.0 {
        0.01
    } else if value < 100_000_000.0 {
        0.02
    } else {
        0.025
    }
}

pub fn strip_item_name(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !matches!(ch, '§' | '[' | ']' | '(' | ')' | '\'' | '"'))
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}

fn first_string(payload: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        payload
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    })
}

fn first_number(payload: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| payload.get(*key).cloned())
        .and_then(|value| parse_number_input(value.into()))
}

fn first_timestamp_millis(payload: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| payload.get(*key))
        .and_then(timestamp_millis)
}

fn timestamp_millis(value: &Value) -> Option<u64> {
    match value {
        Value::Number(number) => number.as_u64(),
        Value::String(value) => parse_timestamp_millis(value),
        _ => None,
    }
}

fn field_text(payload: &Value, key: &str) -> String {
    payload
        .get(key)
        .map(Value::to_string)
        .unwrap_or_else(|| "null".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_invalid_flip_targets() {
        let event = FlipEvent::from_payload(&json!({
            "auctionID": "auction-1",
            "itemName": "Hyperion",
            "startingBid": 10_000_000,
            "target": 0
        }));

        assert_eq!(event.invalid_reason, Some("invalid target 0".to_string()));
    }

    #[test]
    fn normalizes_auction_payload_aliases() {
        let event = FlipEvent::from_payload(&json!({
            "id": "auction-1",
            "itemName": "Withered Giant's Sword",
            "finder": "USER",
            "tag": "GIANTS_SWORD",
            "startingBid": "10m",
            "target": "25m",
            "vol": 4
        }));

        assert!(event.is_valid());
        assert_eq!(event.auction_id.unwrap().as_str(), "auction-1");
        assert_eq!(event.weird_item_name, "WITHERED GIANTS SWORD");
        assert_eq!(event.volume, 4.0);
    }

    #[test]
    fn parses_purchase_at_for_timed_bed_flips() {
        let event = FlipEvent::from_payload(&json!({
            "id": "auction-1",
            "itemName": "Hyperion",
            "startingBid": "10m",
            "target": "25m",
            "purchaseAt": "2030-01-01T00:00:00.250Z"
        }));

        assert_eq!(event.purchase_at_ms, Some(1_893_456_000_250));
        assert!(event.is_future_purchase(1_893_456_000_249));
        assert!(!event.is_future_purchase(1_893_456_000_250));
    }

    #[test]
    fn taxes_match_node_claim_and_creation_fee_semantics() {
        assert_eq!(ihate_taxes(500_000.0), 495_000.0);
        assert_eq!(ihate_taxes(25_000_000.0), 24_250_000.0);
        assert_eq!(ihate_taxes(100_000_000.0), 96_500_000.0);
    }
}
