use saf_core::numbers::parse_number_input;
use saf_core::ports::TrackedFlip;
use serde_json::Value;

pub(super) fn tracked_flip_from_bid_data(
    bid_data: &Value,
    auction_id: &str,
) -> Option<TrackedFlip> {
    let auction_id = auction_id.trim();
    if auction_id.is_empty() {
        return None;
    }
    let entries = bid_data.as_object()?;
    entries.iter().find_map(|(item_uuid, value)| {
        let object = value.as_object()?;
        let stored_auction = string_field(value, "auctionID")
            .or_else(|| string_field(value, "auctionId"))
            .or_else(|| string_field(value, "auction_id"))
            .unwrap_or_else(|| item_uuid.clone());
        if stored_auction != auction_id && item_uuid != auction_id {
            return None;
        }
        let target_price = parse_number_input(object.get("target")?.clone().into())?;
        (target_price > 0.0).then(|| TrackedFlip {
            auction_id: stored_auction,
            target_price,
            weird_item_name: string_field(value, "weirdItemName")
                .or_else(|| string_field(value, "itemName")),
            tag: string_field(value, "tag"),
            price_paid: object
                .get("pricePaid")
                .cloned()
                .and_then(|value| parse_number_input(value.into())),
            finder: string_field(value, "finder"),
            volume: object
                .get("volume")
                .or_else(|| object.get("vol"))
                .cloned()
                .and_then(|value| parse_number_input(value.into())),
            profit_percentage: object
                .get("profitPerc")
                .cloned()
                .and_then(|value| parse_number_input(value.into())),
            buy_kind: string_field(value, "bed"),
            seen_at_ms: None,
        })
    })
}

fn string_field(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}
