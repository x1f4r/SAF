use saf_core::gui::{WindowSlot, WindowSnapshot};
use serde_json::Value;

pub(super) fn string_value(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

pub(super) fn number_value(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| {
            value
                .get(*key)
                .cloned()
                .and_then(|value| saf_core::numbers::parse_number_input(value.into()))
        })
        .filter(|value| value.is_finite())
}

pub(super) fn slot_text_plain(slot: &WindowSlot) -> String {
    std::iter::once(slot.display_name.as_str())
        .chain(slot.lore.iter().map(String::as_str))
        .map(strip_minecraft_color_codes)
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn window_text_plain(window: &WindowSnapshot) -> String {
    std::iter::once(window.title.as_str())
        .chain(window.slots.iter().flat_map(|slot| {
            std::iter::once(slot.display_name.as_str()).chain(slot.lore.iter().map(String::as_str))
        }))
        .map(strip_minecraft_color_codes)
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase()
}

pub(super) fn purchase_lookup_key(item_name: &str, price: f64) -> Option<(String, u64)> {
    if !price.is_finite() || price <= 0.0 {
        return None;
    }
    let item_name = normalized_purchase_item_name(item_name);
    (!item_name.is_empty()).then_some((item_name, price.round() as u64))
}

pub(super) fn sold_listing_lookup_key(action: &Value, list_price: f64) -> Option<(String, u64)> {
    if !list_price.is_finite() || list_price <= 0.0 {
        return None;
    }
    let item_name = string_value(action, &["weirdItemName"])
        .or_else(|| string_value(action, &["itemName"]))
        .or_else(|| string_value(action, &["auctionID", "auctionId", "auction_id"]))?;
    let item_name = normalized_purchase_item_name(&item_name);
    if item_name.is_empty() {
        return None;
    }
    let collected_price = saf_core::flip::ihate_claiming_taxes(list_price).round() as u64;
    (collected_price > 0).then_some((item_name, collected_price))
}

pub(super) fn normalized_purchase_item_name(item_name: &str) -> String {
    saf_core::flip::strip_item_name(&strip_minecraft_color_codes(item_name))
}

pub(super) fn strip_minecraft_color_codes(text: &str) -> String {
    let mut cleaned = String::new();
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        if ch == '§' {
            chars.next();
        } else {
            cleaned.push(ch);
        }
    }
    cleaned
}

pub(super) fn find_labeled_uuid(text: &str, labels: &[&str]) -> Option<String> {
    text.lines()
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            labels.iter().any(|label| lower.contains(label))
        })
        .and_then(find_uuid_like)
}

pub(super) fn find_uuid_like(text: &str) -> Option<String> {
    text.split(|ch: char| !(ch.is_ascii_alphanumeric() || ch == '-'))
        .map(str::trim)
        .find(|token| {
            token.len() >= 8
                && token.chars().any(|character| character.is_ascii_digit())
                && token
                    .chars()
                    .all(|character| character.is_ascii_hexdigit() || character == '-')
        })
        .map(ToString::to_string)
}
