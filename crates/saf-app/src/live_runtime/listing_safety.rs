use super::support::{number_value, string_value};
use saf_core::{BotState, QueueEntry};

const HYPIXEL_MIN_BIN_PRICE: f64 = 500.0;
const CONTEXTUAL_TINY_PRICE: f64 = 1_000.0;
const REFERENCE_PRICE_FLOOR: f64 = 1_000_000.0;
const MIN_OLD_PRICE_FRACTION: f64 = 0.25;
const MIN_PAID_PRICE_FRACTION: f64 = 0.50;

pub(super) fn unsafe_listing_entry_reason(entry: &QueueEntry) -> Option<String> {
    if !matches!(entry.state, BotState::Listing | BotState::ListingNoName) {
        return None;
    }

    let price = number_value(&entry.action, &["price"])?;
    if !price.is_finite() {
        return Some("listing price is not finite".to_string());
    }
    if price < HYPIXEL_MIN_BIN_PRICE {
        return Some(format!(
            "listing price {} is below the Hypixel BIN minimum",
            price.round() as u64
        ));
    }

    if has_price_reference_context(entry)
        && let Some(selector) = listing_inventory_selector(entry)
        && let Some(tag) = string_value(&entry.action, &["tag"])
        && selector.eq_ignore_ascii_case(&tag)
    {
        return Some(format!(
            "automated listing selector `{selector}` is only a SkyBlock item tag, not a unique inventory UUID"
        ));
    }

    let has_automated_context = string_value(&entry.action, &["itemName", "weirdItemName", "tag"])
        .is_some()
        || number_value(
            &entry.action,
            &[
                "oldPrice",
                "old_price",
                "pricePaid",
                "price_paid",
                "target",
                "targetPrice",
            ],
        )
        .is_some();
    if has_automated_context && price <= CONTEXTUAL_TINY_PRICE {
        return Some(format!(
            "automated listing price {} is implausibly tiny",
            price.round() as u64
        ));
    }

    let contextual_references = [
        (
            "old auction price",
            &["oldPrice", "old_price"][..],
            MIN_OLD_PRICE_FRACTION,
        ),
        (
            "paid price",
            &["pricePaid", "price_paid"][..],
            MIN_PAID_PRICE_FRACTION,
        ),
        (
            "target price",
            &["target", "targetPrice", "target_price"][..],
            MIN_OLD_PRICE_FRACTION,
        ),
    ];
    for (label, keys, min_fraction) in contextual_references {
        let Some(reference) = number_value(&entry.action, keys) else {
            continue;
        };
        if reference >= REFERENCE_PRICE_FLOOR && price < reference * min_fraction {
            return Some(format!(
                "listing price {} is less than {:.0}% of {label} {}",
                price.round() as u64,
                min_fraction * 100.0,
                reference.round() as u64
            ));
        }
    }

    None
}

fn has_price_reference_context(entry: &QueueEntry) -> bool {
    number_value(
        &entry.action,
        &[
            "oldPrice",
            "old_price",
            "pricePaid",
            "price_paid",
            "target",
            "targetPrice",
        ],
    )
    .is_some()
}

fn listing_inventory_selector(entry: &QueueEntry) -> Option<String> {
    string_value(
        &entry.action,
        &["inventory", "inv", "itemUuid", "itemUUID", "auctionID"],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn blocks_tiny_automated_listing_prices() {
        let entry = QueueEntry {
            state: BotState::Listing,
            priority: 4,
            action: json!({
                "auctionID": "ab458ac7-f7ff-4056-a710-fa2d8089703c",
                "inv": "ab458ac7-f7ff-4056-a710-fa2d8089703c",
                "weirdItemName": "Ancient Skeleton Master Chestplate",
                "price": 101,
                "oldPrice": 90_500_000
            }),
        };

        assert!(unsafe_listing_entry_reason(&entry).is_some());
    }

    #[test]
    fn allows_manual_low_prices_without_reference_context() {
        let entry = QueueEntry {
            state: BotState::ListingNoName,
            priority: 4,
            action: json!({
                "auctionID": "cheap-item",
                "inv": "cheap-item",
                "price": 1_500
            }),
        };

        assert_eq!(unsafe_listing_entry_reason(&entry), None);
    }

    #[test]
    fn blocks_prices_far_below_purchase_context() {
        let entry = QueueEntry {
            state: BotState::Listing,
            priority: 1,
            action: json!({
                "auctionID": "auction-1",
                "inventory": "item-uuid",
                "price": 350_000,
                "pricePaid": 86_000_000,
                "weirdItemName": "Renowned Magma Lord Leggings"
            }),
        };

        assert!(unsafe_listing_entry_reason(&entry).is_some());
    }

    #[test]
    fn blocks_automated_tag_only_listing_selectors() {
        let entry = QueueEntry {
            state: BotState::Listing,
            priority: 1,
            action: json!({
                "auctionID": "auction-1",
                "inventory": "POWER_WITHER_LEGGINGS",
                "inv": "POWER_WITHER_LEGGINGS",
                "price": 124_500_000,
                "pricePaid": 99_000_000,
                "targetPrice": 128_327_984,
                "tag": "POWER_WITHER_LEGGINGS",
                "itemName": "Ancient Necron's Leggings"
            }),
        };

        let reason = unsafe_listing_entry_reason(&entry).unwrap();

        assert!(reason.contains("SkyBlock item tag"));
    }
}
