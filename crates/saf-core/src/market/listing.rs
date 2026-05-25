use super::*;
use crate::auction::calc_duration_visual;
use crate::gui::{WindowSlot, WindowSnapshot};
use crate::numbers::add_commas_to_number;
use crate::relist::parse_old_price_from_lore_line;

pub(super) fn plan_list_item(
    item_uuid: &ItemUuid,
    price: f64,
    hours: f64,
    window: Option<&WindowSnapshot>,
) -> MarketStep {
    plan_list_item_with_context(item_uuid, None, None, price, hours, window)
}

pub(super) fn plan_list_item_with_context(
    item_uuid: &ItemUuid,
    item_name: Option<&str>,
    tag: Option<&str>,
    price: f64,
    hours: f64,
    window: Option<&WindowSnapshot>,
) -> MarketStep {
    let Some(window) = window else {
        return MarketStep::next(
            MarketInstruction::Chat {
                message: "/ah".to_string(),
            },
            "open auction house",
        );
    };
    let title = window.title.to_ascii_lowercase();
    if is_confirm_listing_window(window) {
        if !window_contains_listing_price(window, price) {
            return MarketStep::next(
                MarketInstruction::CloseWindow,
                "close listing confirmation with unexpected price",
            );
        }
        return MarketStep::finish(
            MarketInstruction::ClickSlot {
                slot: window
                    .resolve_slot(
                        &["confirm", "create auction", "create bin auction"],
                        Some(11),
                    )
                    .unwrap_or(11),
            },
            "confirm listing",
        );
    }
    if title.contains("create") {
        let selector = ListingSelector {
            item_uuid,
            item_name,
            tag,
        };
        return plan_create_listing_window(selector, price, hours, window);
    }
    if title.contains("duration")
        && let Some(slot) = window.resolve_slot(&["custom duration", "custom"], Some(16))
    {
        return MarketStep::next(
            MarketInstruction::ClickSlotThenType {
                slot,
                text: format_listing_hours(hours),
            },
            "enter custom listing duration",
        );
    }
    if title.contains("price") {
        return MarketStep::next(
            MarketInstruction::TypeText {
                text: quoted_sign_text(price.round()),
            },
            "enter listing price",
        );
    }
    MarketStep::next(
        MarketInstruction::ClickSlot {
            slot: window
                .resolve_slot(&["create auction", "start auction"], Some(15))
                .unwrap_or(15),
        },
        "open create auction",
    )
}

#[derive(Clone, Copy)]
struct ListingSelector<'a> {
    item_uuid: &'a ItemUuid,
    item_name: Option<&'a str>,
    tag: Option<&'a str>,
}

fn plan_create_listing_window(
    selector: ListingSelector<'_>,
    price: f64,
    hours: f64,
    window: &WindowSnapshot,
) -> MarketStep {
    let title = window.title.to_ascii_lowercase();
    if title.contains("price") {
        return MarketStep::next(
            MarketInstruction::TypeText {
                text: quoted_sign_text(price.round()),
            },
            "enter listing price",
        );
    }
    if title.contains("duration") {
        if let Some(slot) = window.resolve_slot(&["custom duration", "custom"], None) {
            return MarketStep::next(
                MarketInstruction::ClickSlotThenType {
                    slot,
                    text: format_listing_hours(hours),
                },
                "enter custom listing duration",
            );
        }
        return MarketStep::next(
            MarketInstruction::TypeText {
                text: format_listing_hours(hours),
            },
            "enter listing duration",
        );
    }
    if !listing_item_selected(window, selector) {
        if let Some(slot) = listing_inventory_item_slot(window, selector) {
            return MarketStep::next(
                MarketInstruction::ClickSlot { slot },
                format!("select listing item {}", selector.item_uuid),
            );
        }
        if !window_has_selectable_inventory_items(window) {
            return MarketStep::next(
                MarketInstruction::Noop,
                format!("wait for listing inventory item {}", selector.item_uuid),
            );
        }
        return MarketStep::next(
            MarketInstruction::ClickSlot {
                slot: window
                    .resolve_slot(
                        &["create auction", "submit auction", "auction item"],
                        Some(13),
                    )
                    .unwrap_or(13),
            },
            format!("prepare listing for {}", selector.item_uuid),
        );
    }
    if !listing_price_matches(window, price) {
        let Some(slot) = listing_price_slot(window).map(|slot| slot.slot) else {
            return MarketStep::next(MarketInstruction::Noop, "wait for listing price control");
        };
        return MarketStep::next(
            MarketInstruction::ClickSlotThenType {
                slot,
                text: quoted_sign_text(price.round()),
            },
            "enter listing price",
        );
    }
    if !listing_duration_matches(window, hours) {
        let Some(slot) = listing_duration_slot(window).map(|slot| slot.slot) else {
            return MarketStep::next(MarketInstruction::Noop, "wait for listing duration control");
        };
        return MarketStep::next(
            MarketInstruction::ClickSlot { slot },
            "open listing duration",
        );
    }
    MarketStep::next(
        MarketInstruction::ClickSlot {
            slot: window
                .resolve_slot(
                    &[
                        "create auction",
                        "create bin auction",
                        "submit",
                        "list auction",
                    ],
                    Some(29),
                )
                .unwrap_or(29),
        },
        "submit listing",
    )
}

fn is_confirm_listing_window(window: &WindowSnapshot) -> bool {
    let title = window.title.to_ascii_lowercase();
    if title.contains("confirm") && (title.contains("auction") || title.contains("bin")) {
        return true;
    }
    let text = window_text(window);
    text.contains("confirm auction") || text.contains("confirm bin")
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

fn format_listing_hours(hours: f64) -> String {
    if hours.fract() == 0.0 {
        format!("{}h", hours as u64)
    } else {
        format!("{hours}h")
    }
}

fn quoted_sign_text(value: impl ToString) -> String {
    format!("\"{}\"", value.to_string())
}

fn listing_item_selected(window: &WindowSnapshot, selector: ListingSelector<'_>) -> bool {
    window
        .slots
        .iter()
        .any(|slot| slot.slot == 13 && slot_matches_listing_selector(slot, selector))
}

fn listing_inventory_item_slot(
    window: &WindowSnapshot,
    selector: ListingSelector<'_>,
) -> Option<usize> {
    window
        .slots
        .iter()
        .find(|slot| {
            slot_matches_listing_selector(slot, selector)
                && ![13, 29, 31, 33, 48, 49].contains(&slot.slot)
        })
        .map(|slot| slot.slot)
}

fn slot_matches_listing_selector(slot: &WindowSlot, selector: ListingSelector<'_>) -> bool {
    if slot
        .item_uuid
        .as_deref()
        .is_some_and(|uuid| uuid.eq_ignore_ascii_case(selector.item_uuid.as_str()))
    {
        return true;
    }
    if selected_auction_item_without_uuid(slot) && slot_matches_item_name_context(slot, selector) {
        return true;
    }
    if !selector_allows_context_fallback(selector) {
        return false;
    }
    if let Some(item_name) = selector.item_name {
        if slot_matches_item_name(slot, item_name) {
            return true;
        }
    }
    if let Some(tag) = selector.tag
        && slot_matches_tag(slot, tag)
    {
        return true;
    }
    false
}

fn selector_allows_context_fallback(selector: ListingSelector<'_>) -> bool {
    selector
        .tag
        .is_some_and(|tag| selector.item_uuid.as_str().eq_ignore_ascii_case(tag))
}

fn selected_auction_item_without_uuid(slot: &WindowSlot) -> bool {
    slot.slot == 13 && slot.item_uuid.is_none()
}

fn slot_matches_item_name_context(slot: &WindowSlot, selector: ListingSelector<'_>) -> bool {
    selector
        .item_name
        .is_some_and(|item_name| slot_matches_item_name(slot, item_name))
}

fn slot_matches_item_name(slot: &WindowSlot, item_name: &str) -> bool {
    let wanted = normalized_listing_text(item_name);
    !wanted.is_empty() && normalized_slot_text(slot).contains(&wanted)
}

fn slot_matches_tag(slot: &WindowSlot, tag: &str) -> bool {
    if slot
        .item_uuid
        .as_deref()
        .is_some_and(|uuid| uuid.eq_ignore_ascii_case(tag))
    {
        return true;
    }
    let wanted = normalized_tag_text(tag);
    !wanted.is_empty() && normalized_slot_text(slot).contains(&wanted)
}

fn normalized_slot_text(slot: &WindowSlot) -> String {
    normalized_listing_text(
        &std::iter::once(slot.display_name.as_str())
            .chain(slot.lore.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn normalized_tag_text(tag: &str) -> String {
    let mut parts = tag
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts
        .first()
        .is_some_and(|first| matches!(*first, "PET" | "RUNE" | "UNIQUE"))
    {
        parts.remove(0);
    }
    normalized_listing_text(&parts.join(" "))
}

fn normalized_listing_text(text: &str) -> String {
    let mut plain = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '§' {
            chars.next();
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            plain.push(ch.to_ascii_lowercase());
        } else {
            plain.push(' ');
        }
    }
    plain.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn window_has_selectable_inventory_items(window: &WindowSnapshot) -> bool {
    window
        .slots
        .iter()
        .any(|slot| ![13, 29, 31, 33, 48, 49].contains(&slot.slot) && slot.item_uuid.is_some())
}

fn listing_price_matches(window: &WindowSnapshot, price: f64) -> bool {
    listing_price_slot(window).is_some_and(|slot| slot_contains_listing_price(slot, price))
}

fn window_contains_listing_price(window: &WindowSnapshot, price: f64) -> bool {
    let expected = add_commas_to_number(price.round());
    let text = window_text(window);
    if text.contains(&expected.to_ascii_lowercase())
        || text.contains(&price.round().to_string())
        || text.contains(&format!("{} coins", expected.to_ascii_lowercase()))
    {
        return true;
    }
    window
        .slots
        .iter()
        .flat_map(|slot| {
            std::iter::once(slot.display_name.as_str()).chain(slot.lore.iter().map(String::as_str))
        })
        .any(|line| line_contains_listing_price(line, price))
        || confirmation_fee_matches_listing_price(window, price)
}

fn slot_contains_listing_price(slot: &WindowSlot, price: f64) -> bool {
    let expected = add_commas_to_number(price.round());
    let text = slot.text();
    if text.contains(&expected.to_ascii_lowercase())
        || text.contains(&price.round().to_string())
        || text.contains(&format!("{} coins", expected.to_ascii_lowercase()))
    {
        return true;
    }
    std::iter::once(slot.display_name.as_str())
        .chain(slot.lore.iter().map(String::as_str))
        .any(|line| line_contains_listing_price(line, price))
}

fn line_contains_listing_price(line: &str, price: f64) -> bool {
    let lower = line.to_ascii_lowercase();
    let has_price_marker =
        lower.contains("price") || lower.contains("starting bid") || lower.contains("coins");
    let value = labeled_line_value(line);
    has_price_marker
        && parse_old_price_from_lore_line(value)
            .is_some_and(|parsed| parsed_listing_price_matches(parsed, price))
}

fn parsed_listing_price_matches(parsed: f64, expected: f64) -> bool {
    parsed.is_finite() && expected.is_finite() && (parsed.round() - expected.round()).abs() <= 1.0
}

fn confirmation_fee_matches_listing_price(window: &WindowSnapshot, price: f64) -> bool {
    window.slots.iter().any(|slot| {
        std::iter::once(slot.display_name.as_str())
            .chain(slot.lore.iter().map(String::as_str))
            .any(|line| line_creation_fee_matches_listing_price(line, price))
    })
}

fn line_creation_fee_matches_listing_price(line: &str, expected: f64) -> bool {
    let lower = line.to_ascii_lowercase();
    if !lower.contains("coin") || !(lower.contains("cost") || lower.contains("fee")) {
        return false;
    }
    let Some(fee) = parse_old_price_from_lore_line(labeled_line_value(line)) else {
        return false;
    };
    confirmation_creation_fee_candidates(expected)
        .into_iter()
        .any(|expected_fee| parsed_listing_price_fuzzy_matches(fee, expected_fee))
}

fn confirmation_creation_fee_candidates(expected: f64) -> Vec<f64> {
    const BASE_RATES: [f64; 3] = [0.01, 0.02, 0.025];
    const COMMON_DURATION_FEES: [f64; 3] = [0.0, 1_200.0, 2_400.0];

    if !expected.is_finite() || expected <= 0.0 {
        return Vec::new();
    }

    BASE_RATES
        .into_iter()
        .flat_map(|rate| {
            COMMON_DURATION_FEES
                .into_iter()
                .map(move |duration_fee| expected * rate + duration_fee)
        })
        .collect()
}

fn labeled_line_value(line: &str) -> &str {
    line.split_once(':')
        .map(|(_, value)| value.trim())
        .unwrap_or(line)
}

fn parsed_listing_price_fuzzy_matches(parsed: f64, expected: f64) -> bool {
    if !parsed.is_finite() || !expected.is_finite() {
        return false;
    }
    let tolerance = (expected.abs() * 0.005).max(5_000.0);
    (parsed.round() - expected.round()).abs() <= tolerance
}

fn listing_duration_matches(window: &WindowSnapshot, hours: f64) -> bool {
    let Some(slot) = listing_duration_slot(window) else {
        return false;
    };
    let text = slot.text();
    listing_duration_visuals(hours)
        .into_iter()
        .any(|expected| text.contains(&expected.to_ascii_lowercase()))
}

fn listing_price_slot(window: &WindowSnapshot) -> Option<&WindowSlot> {
    slot_for_excluding(
        window,
        &["price", "starting bid"],
        Some(31),
        &[13, 29, 33, 48, 49],
    )
}

fn listing_duration_slot(window: &WindowSnapshot) -> Option<&WindowSlot> {
    slot_for_excluding(window, &["duration"], Some(33), &[13, 29, 31, 48, 49])
}

fn slot_for_excluding<'a>(
    window: &'a WindowSnapshot,
    patterns: &[&str],
    fallback: Option<usize>,
    excluded_slots: &[usize],
) -> Option<&'a WindowSlot> {
    window
        .slots
        .iter()
        .filter(|slot| !excluded_slots.contains(&slot.slot))
        .find(|slot| {
            let text = slot.text();
            patterns
                .iter()
                .any(|pattern| text.contains(&pattern.to_ascii_lowercase()))
        })
        .or_else(|| {
            fallback.and_then(|fallback| {
                (!excluded_slots.contains(&fallback))
                    .then(|| window.slots.iter().find(|slot| slot.slot == fallback))
                    .flatten()
            })
        })
}

fn format_listing_duration_visual(hours: f64) -> String {
    if hours >= 336.0 {
        return "14 Days".to_string();
    }
    if hours >= 24.0 {
        let days = (hours / 24.0).floor() as u64;
        return if hours >= 48.0 {
            format!("{days} Days")
        } else {
            format!("{days} Day")
        };
    }
    let suffix = if hours == 1.0 { "Hour" } else { "Hours" };
    if hours.fract() == 0.0 {
        format!("{} {suffix}", hours as u64)
    } else {
        format!("{hours} {suffix}")
    }
}

fn listing_duration_visuals(hours: f64) -> Vec<String> {
    let mut visuals = vec![
        format_listing_duration_visual(hours),
        calc_duration_visual(hours),
    ];
    visuals.sort();
    visuals.dedup();
    visuals
}
