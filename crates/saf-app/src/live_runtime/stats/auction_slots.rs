use super::super::BASE_AUCTION_SLOTS;
use super::super::support::{slot_text_plain, strip_minecraft_color_codes};
use super::super::windows::{is_manage_auctions_window, is_profiles_window};
use saf_core::gui::WindowSnapshot;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct AuctionSlotStats {
    pub(crate) used: Option<usize>,
    pub(crate) max: Option<usize>,
}

pub(crate) fn auction_slot_stats_from_window(window: &WindowSnapshot) -> Option<AuctionSlotStats> {
    if !is_manage_auctions_window(window) {
        return empty_auction_house_slot_stats(window);
    }
    let active_count = active_auction_slot_count(window);
    let parsed_slots = window
        .slots
        .iter()
        .flat_map(|slot| {
            std::iter::once(slot.display_name.as_str()).chain(slot.lore.iter().map(String::as_str))
        })
        .find_map(parse_auction_slot_fraction);
    Some(AuctionSlotStats {
        used: Some(parsed_slots.map_or(active_count, |(used, _)| used)),
        max: parsed_slots.map(|(_, max)| max),
    })
}

fn active_auction_slot_count(window: &WindowSnapshot) -> usize {
    window
        .slots
        .iter()
        .filter(|slot| {
            let text = slot.text();
            text.contains("seller:") && !text.contains("buyer:") && !text.contains("expired")
        })
        .count()
}

fn empty_auction_house_slot_stats(window: &WindowSnapshot) -> Option<AuctionSlotStats> {
    let title = window.title.to_ascii_lowercase();
    if !title.contains("auction house") {
        return None;
    }

    let has_create_auction = window.slots.iter().any(|slot| {
        let text = slot_text_plain(slot).to_ascii_lowercase();
        text.contains("create auction") || text.contains("start auction")
    });
    let has_manage_auctions = window.slots.iter().any(|slot| {
        let text = slot_text_plain(slot).to_ascii_lowercase();
        text.contains("manage auctions") || text.contains("your auctions")
    });

    (has_create_auction && !has_manage_auctions).then_some(AuctionSlotStats {
        used: Some(0),
        max: None,
    })
}

pub(crate) fn auction_slot_stats_full(stats: AuctionSlotStats) -> bool {
    matches!(
        (stats.used, stats.max),
        (Some(used), Some(max)) if max > 0 && used >= max
    )
}

pub(crate) fn profile_auction_slots_max(window: &WindowSnapshot) -> Option<usize> {
    if !is_profiles_window(window) {
        return None;
    }
    let coop_bonus = window
        .slots
        .iter()
        .filter(|slot| {
            let text = slot_text_plain(slot).to_ascii_lowercase();
            slot.name == "emerald_block"
                || text.contains("co-op with")
                || text.contains("selected profile")
                || text.contains("profile")
        })
        .flat_map(|slot| {
            std::iter::once(slot.display_name.as_str()).chain(slot.lore.iter().map(String::as_str))
        })
        .find_map(parse_coop_auction_slot_bonus)
        .unwrap_or(0);
    Some(BASE_AUCTION_SLOTS + coop_bonus)
}

fn parse_coop_auction_slot_bonus(line: &str) -> Option<usize> {
    let cleaned = strip_minecraft_color_codes(line);
    let lower = cleaned.to_ascii_lowercase();
    let marker = "co-op with";
    let start = lower.find(marker)? + marker.len();
    let rest = cleaned[start..].trim();
    if rest.is_empty() {
        return None;
    }
    let leading_digits = rest
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    if !leading_digits.is_empty() && lower[start..].contains("players") {
        return leading_digits
            .parse::<usize>()
            .ok()
            .map(|players| players.saturating_mul(3));
    }
    Some(3)
}

fn parse_auction_slot_fraction(text: &str) -> Option<(usize, usize)> {
    let cleaned = strip_minecraft_color_codes(text);
    let lower = cleaned.to_ascii_lowercase();
    if !lower.contains("auction") {
        return None;
    }
    if lower.contains("page") || lower.contains("duration") || lower.contains("expires") {
        return None;
    }
    if !(lower.contains("slot")
        || lower.contains("active")
        || lower.contains("limit")
        || lower.contains("auctions:")
        || lower.contains(" auctions"))
    {
        return None;
    }

    let chars = cleaned.chars().collect::<Vec<_>>();
    for (index, ch) in chars.iter().enumerate() {
        if *ch != '/' {
            continue;
        }
        let Some(used) = parse_number_before(&chars, index) else {
            continue;
        };
        let Some(max) = parse_number_after(&chars, index + 1) else {
            continue;
        };
        if max > 0 && used <= max {
            return Some((used, max));
        }
    }
    None
}

fn parse_number_before(chars: &[char], index: usize) -> Option<usize> {
    let mut cursor = index;
    while cursor > 0 && chars[cursor - 1].is_ascii_whitespace() {
        cursor -= 1;
    }
    let end = cursor;
    while cursor > 0 && (chars[cursor - 1].is_ascii_digit() || chars[cursor - 1] == ',') {
        cursor -= 1;
    }
    parse_compact_usize(&chars[cursor..end])
}

fn parse_number_after(chars: &[char], index: usize) -> Option<usize> {
    let mut cursor = index;
    while cursor < chars.len() && chars[cursor].is_ascii_whitespace() {
        cursor += 1;
    }
    let start = cursor;
    while cursor < chars.len() && (chars[cursor].is_ascii_digit() || chars[cursor] == ',') {
        cursor += 1;
    }
    parse_compact_usize(&chars[start..cursor])
}

fn parse_compact_usize(chars: &[char]) -> Option<usize> {
    let digits = chars
        .iter()
        .filter(|character| character.is_ascii_digit())
        .collect::<String>();
    (!digits.is_empty()).then(|| digits.parse::<usize>().ok())?
}
