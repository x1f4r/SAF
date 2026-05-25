use crate::live_runtime::support::{
    slot_text_plain, strip_minecraft_color_codes, window_text_plain,
};
use saf_core::gui::{WindowSlot, WindowSnapshot};

pub(in crate::live_runtime) fn is_bids_window(window: &WindowSnapshot) -> bool {
    window.title.to_ascii_lowercase().contains("bids")
}

pub(in crate::live_runtime) fn is_create_auction_window(window: &WindowSnapshot) -> bool {
    let title = window.title.to_ascii_lowercase();
    title.contains("create") && title.contains("auction")
}

pub(in crate::live_runtime) fn is_confirm_auction_window(window: &WindowSnapshot) -> bool {
    let title = window.title.to_ascii_lowercase();
    (title.contains("confirm") && title.contains("auction"))
        || window_text_plain(window).contains("confirm auction")
        || window_text_plain(window).contains("confirm bin")
}

pub(in crate::live_runtime) fn create_auction_slot(window: &WindowSnapshot) -> Option<usize> {
    window.resolve_slot(&["create auction", "start auction"], None)
}

pub(in crate::live_runtime) fn submit_auction_slot(window: &WindowSnapshot) -> usize {
    window
        .resolve_slot(
            &[
                "create auction",
                "create bin auction",
                "submit",
                "list auction",
            ],
            Some(29),
        )
        .unwrap_or(29)
}

pub(in crate::live_runtime) fn confirm_auction_slot(window: &WindowSnapshot) -> usize {
    window
        .resolve_slot(&["confirm", "create auction", "create bin auction"], None)
        .unwrap_or_else(|| window.auction_action_slot(11))
}

pub(in crate::live_runtime::auction_flow) fn find_labeled_slot_value(
    slot: &WindowSlot,
    labels: &[&str],
) -> Option<String> {
    std::iter::once(slot.display_name.as_str())
        .chain(slot.lore.iter().map(String::as_str))
        .filter_map(|line| {
            let line = strip_minecraft_color_codes(line);
            let lower = line.to_ascii_lowercase();
            labels
                .iter()
                .any(|label| lower.contains(&label.to_ascii_lowercase()))
                .then_some(line)
        })
        .find_map(|line| {
            line.split_once(':')
                .map(|(_, value)| value.trim().to_string())
                .filter(|value| !value.is_empty())
        })
}

pub(in crate::live_runtime::auction_flow) fn slot_plain_text(slot: &WindowSlot) -> String {
    slot_text_plain(slot)
}
