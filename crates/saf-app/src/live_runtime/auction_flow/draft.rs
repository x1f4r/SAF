use super::window::{find_labeled_slot_value, is_create_auction_window, slot_plain_text};
use crate::live_runtime::support::strip_minecraft_color_codes;
use saf_core::ItemUuid;
use saf_core::gui::{WindowSlot, WindowSnapshot};

#[derive(Clone, Debug, PartialEq)]
pub(in crate::live_runtime) struct PendingAuctionDraft {
    pub(in crate::live_runtime) item_name: String,
    pub(in crate::live_runtime) item_uuid: ItemUuid,
    pub(in crate::live_runtime) tag: Option<String>,
    pub(in crate::live_runtime) list_price: u64,
    pub(in crate::live_runtime) item_slot: usize,
}

pub(in crate::live_runtime) fn pending_create_auction_draft(
    window: &WindowSnapshot,
) -> Option<PendingAuctionDraft> {
    if !is_create_auction_window(window) {
        return None;
    }
    let item_slot = window
        .slots
        .iter()
        .find(|slot| slot.slot == 13 && selected_draft_item_slot(slot))?;
    let price_slot = pending_draft_price_slot(window)?;
    let price_text = slot_plain_text(price_slot);
    let price_line = price_text
        .lines()
        .find(|line| {
            let lower = line.to_ascii_lowercase();
            lower.contains("price") || lower.contains("coins")
        })
        .unwrap_or(&price_text);
    let list_price = price_line
        .split_once(':')
        .map(|(_, value)| value)
        .unwrap_or(price_line);
    let list_price = saf_core::relist::parse_old_price_from_lore_line(list_price)?
        .round()
        .max(0.0) as u64;
    if list_price < 500 {
        return None;
    }

    let item_name = draft_item_name(item_slot);
    let item_uuid = item_slot
        .item_uuid
        .as_deref()
        .and_then(ItemUuid::new)
        .or_else(|| ItemUuid::new(item_name.clone()))?;
    Some(PendingAuctionDraft {
        item_name,
        item_uuid,
        tag: find_labeled_slot_value(item_slot, &["tag", "item tag", "skyblock id"]),
        list_price,
        item_slot: item_slot.slot,
    })
}

fn selected_draft_item_slot(slot: &WindowSlot) -> bool {
    if slot.item_uuid.is_some() {
        return true;
    }
    let text = slot_plain_text(slot);
    let lower = text.to_ascii_lowercase();
    !lower.contains("click an item in your inventory")
        && !lower.contains("no item selected")
        && !lower.contains("selects it for auction")
        && !lower.contains("select an item")
}

fn pending_draft_price_slot(window: &WindowSnapshot) -> Option<&WindowSlot> {
    window
        .slots
        .iter()
        .filter(|slot| ![13, 29, 33, 48, 49].contains(&slot.slot))
        .find(|slot| {
            let text = slot_plain_text(slot);
            let lower = text.to_ascii_lowercase();
            lower.contains("price") || lower.contains("starting bid")
        })
        .or_else(|| window.slots.iter().find(|slot| slot.slot == 31))
}

fn draft_item_name(item_slot: &saf_core::gui::WindowSlot) -> String {
    let display_name = strip_minecraft_color_codes(&item_slot.display_name);
    let display_label = display_name.trim().to_ascii_lowercase();
    if !display_label.is_empty()
        && display_label != "auction for item:"
        && display_label != "item to auction"
    {
        return display_name;
    }
    item_slot
        .lore
        .iter()
        .map(|line| strip_minecraft_color_codes(line))
        .find(|line| !line.trim().is_empty())
        .unwrap_or(display_name)
}
