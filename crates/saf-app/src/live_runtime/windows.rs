use super::LiveRuntime;
use super::support::{find_labeled_uuid, find_uuid_like};
use anyhow::Result;
use saf_core::AccountId;
use saf_core::gui::WindowSnapshot;
use saf_core::ports::ActiveAuction;
use std::time::Duration;

impl LiveRuntime {
    pub(super) fn clear_active_window_cache(&self, account: &AccountId) -> Result<()> {
        self.active_windows
            .lock()
            .map_err(|_| anyhow::anyhow!("active window lock poisoned"))?
            .remove(account);
        self.active_window_received_at
            .lock()
            .map_err(|_| anyhow::anyhow!("active window received timestamp lock poisoned"))?
            .remove(account);
        self.active_window_observed_at
            .lock()
            .map_err(|_| anyhow::anyhow!("active window timestamp lock poisoned"))?
            .remove(account);
        self.market_settle_jitter
            .lock()
            .map_err(|_| anyhow::anyhow!("market settle jitter lock poisoned"))?
            .remove(account);
        Ok(())
    }
}

pub(super) fn window_title_matches(window: &WindowSnapshot, patterns: &[&str]) -> bool {
    let title = window.title.to_ascii_lowercase();
    patterns
        .iter()
        .any(|pattern| title.contains(&pattern.to_ascii_lowercase()))
}

pub(super) fn is_manage_auctions_window(window: &WindowSnapshot) -> bool {
    window_title_matches(window, &["manage auctions"])
}

pub(super) fn is_profiles_window(window: &WindowSnapshot) -> bool {
    window_title_matches(window, &["profiles"])
}

pub(super) fn is_skyblock_menu_window(window: &WindowSnapshot) -> bool {
    window_title_matches(window, &["skyblock menu"])
}

pub(super) fn visit_window_disallows_guests(window: &WindowSnapshot) -> bool {
    window
        .slots
        .iter()
        .any(|slot| slot.text().contains("island disallows guests"))
}

pub(super) fn cookie_duration_from_window(window: &WindowSnapshot) -> Option<Duration> {
    window.slots.iter().find_map(|slot| {
        std::iter::once(slot.display_name.as_str())
            .chain(slot.lore.iter().map(String::as_str))
            .filter(|line| line.to_ascii_lowercase().contains("duration"))
            .find_map(saf_core::time::normal_time)
    })
}

pub(super) fn active_auction_summaries(window: &WindowSnapshot) -> Vec<ActiveAuction> {
    if !is_manage_auctions_window(window) {
        return Vec::new();
    }
    window
        .slots
        .iter()
        .filter(|slot| {
            let text = slot.text();
            text.contains("seller:") && !text.contains("buyer:") && !text.contains("expired")
        })
        .filter_map(|slot| {
            let text = slot.text();
            let item_uuid = slot
                .item_uuid
                .clone()
                .or_else(|| find_labeled_uuid(&text, &["item uuid", "item id"]))
                .or_else(|| find_uuid_like(&text))?;
            let auction_id = find_labeled_uuid(&text, &["auction id", "auction uuid", "auction"])
                .unwrap_or_else(|| item_uuid.clone());
            Some(ActiveAuction {
                auction_id,
                item_uuid,
                name: (!slot.display_name.trim().is_empty()).then(|| slot.display_name.clone()),
            })
        })
        .collect()
}
