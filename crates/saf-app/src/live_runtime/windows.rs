use super::LiveRuntime;
use super::support::{find_labeled_uuid, find_uuid_like};
use anyhow::Result;
use saf_core::AccountId;
use saf_core::gui::WindowSnapshot;
use saf_core::ports::ActiveAuction;
use serde::Serialize;
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

/// Where one of the operator's auctions stands, derived from its Manage
/// Auctions tooltip. `Active` is still listed; `Sold` has a buyer and the coins
/// are waiting to be collected; `Expired` ended unsold and the item is waiting
/// to be collected back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(in crate::live_runtime) enum AuctionViewStatus {
    Active,
    Sold,
    Expired,
}

/// One auction in the dashboard's auctions view. Fields are best-effort: the
/// tooltip does not always expose every value, so most are optional.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::live_runtime) struct AuctionViewEntry {
    pub item_uuid: Option<String>,
    pub auction_id: Option<String>,
    pub name: Option<String>,
    pub status: AuctionViewStatus,
    pub price: Option<f64>,
    pub ends_in: Option<String>,
    pub buyer: Option<String>,
}

/// The last Manage Auctions window the bot observed for an account, captured
/// passively whenever it opens that menu during reconciliation. `observed_at_ms`
/// is wall-clock so the dashboard can show how fresh it is.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::live_runtime) struct AuctionViewSnapshot {
    pub observed_at_ms: u64,
    pub entries: Vec<AuctionViewEntry>,
}

/// Parses every auction tooltip in a Manage Auctions window into a categorized
/// entry. Returns `None` for any other window so callers can cheaply ignore
/// non-auction snapshots.
pub(in crate::live_runtime) fn manage_auction_entries(
    window: &WindowSnapshot,
) -> Option<Vec<AuctionViewEntry>> {
    if !is_manage_auctions_window(window) {
        return None;
    }
    let entries = window
        .slots
        .iter()
        .filter(|slot| {
            // Real auction tiles carry a seller/buyer/expired marker or an
            // auction id; filler glass panes and nav arrows do not.
            let text = slot.text().to_ascii_lowercase();
            text.contains("seller:")
                || text.contains("buyer:")
                || text.contains("expired")
                || text.contains("auction id")
        })
        .map(|slot| {
            let text = slot.text();
            let lower = text.to_ascii_lowercase();
            let status = if lower.contains("expired") {
                AuctionViewStatus::Expired
            } else if lower.contains("buyer:") || lower.contains("sold for") {
                AuctionViewStatus::Sold
            } else {
                AuctionViewStatus::Active
            };
            let item_uuid = slot
                .item_uuid
                .clone()
                .or_else(|| find_labeled_uuid(&text, &["item uuid", "item id"]))
                .or_else(|| find_uuid_like(&text));
            let auction_id = find_labeled_uuid(&text, &["auction id", "auction uuid", "auction"]);
            AuctionViewEntry {
                item_uuid,
                auction_id,
                name: (!slot.display_name.trim().is_empty()).then(|| slot.display_name.clone()),
                status,
                price: auction_tooltip_price(slot),
                ends_in: labeled_tooltip_value(slot, "ends in"),
                buyer: labeled_tooltip_value(slot, "buyer"),
            }
        })
        .collect();
    Some(entries)
}

/// Pulls the most relevant coin amount from an auction tooltip, trying the
/// common BIN / bid / sale labels in priority order.
fn auction_tooltip_price(slot: &saf_core::gui::WindowSlot) -> Option<f64> {
    const LABELS: [&str; 6] = [
        "buy it now",
        "sold for",
        "top bid",
        "highest bid",
        "starting bid",
        "price",
    ];
    for label in LABELS {
        for line in std::iter::once(slot.display_name.as_str())
            .chain(slot.lore.iter().map(String::as_str))
        {
            if line.to_ascii_lowercase().contains(label)
                && let Some(value) = parse_trailing_coins(line)
            {
                return Some(value);
            }
        }
    }
    None
}

/// Returns the trimmed value after `label:` in a tooltip (e.g. the player name
/// after `Buyer:` or the time after `Ends in:`), stripped of color codes.
fn labeled_tooltip_value(slot: &saf_core::gui::WindowSlot, label: &str) -> Option<String> {
    let needle = format!("{}:", label.to_ascii_lowercase());
    for line in
        std::iter::once(slot.display_name.as_str()).chain(slot.lore.iter().map(String::as_str))
    {
        let lower = line.to_ascii_lowercase();
        if let Some(idx) = lower.find(&needle) {
            let value = super::support::strip_minecraft_color_codes(&line[idx + needle.len()..]);
            let value = value.trim().trim_end_matches('!').trim().to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// Parses the coin amount immediately before "coins" in a tooltip line,
/// ignoring thousands separators.
fn parse_trailing_coins(line: &str) -> Option<f64> {
    let lower = line.to_ascii_lowercase();
    let head = lower.split("coin").next()?;
    let digits: String = head
        .chars()
        .rev()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit() || *c == ',')
        .filter(char::is_ascii_digit)
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.chars().rev().collect::<String>().parse::<f64>().ok()
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

#[cfg(test)]
mod tests {
    use super::*;
    use saf_core::gui::WindowSlot;

    fn slot(slot: usize, name: &str, lore: &[&str]) -> WindowSlot {
        WindowSlot {
            slot,
            name: "paper".to_string(),
            display_name: name.to_string(),
            lore: lore.iter().map(|line| line.to_string()).collect(),
            item_uuid: Some(format!("uuid-{slot}")),
        }
    }

    #[test]
    fn manage_auction_entries_categorizes_active_sold_and_expired() {
        let window = WindowSnapshot {
            title: "Manage Auctions".to_string(),
            slots: vec![
                slot(
                    10,
                    "Active Sword",
                    &["Seller: x1f4r", "Buy it now: 1,000,000 coins", "Ends in: 1d 2h"],
                ),
                slot(
                    11,
                    "Sold Bow",
                    &["Seller: x1f4r", "Buyer: SomeOne", "Sold for: 2,500,000 coins"],
                ),
                slot(
                    12,
                    "Expired Helmet",
                    &["Seller: x1f4r", "Status: Expired!", "Starting bid: 500,000 coins"],
                ),
                WindowSlot {
                    slot: 13,
                    name: "black_stained_glass_pane".to_string(),
                    display_name: "Black Stained Glass Pane".to_string(),
                    lore: Vec::new(),
                    item_uuid: None,
                },
            ],
        };

        let entries = manage_auction_entries(&window).expect("manage window should parse");
        assert_eq!(entries.len(), 3, "filler panes must be excluded");

        let active = &entries[0];
        assert_eq!(active.status, AuctionViewStatus::Active);
        assert_eq!(active.price, Some(1_000_000.0));
        assert_eq!(active.ends_in.as_deref(), Some("1d 2h"));

        let sold = &entries[1];
        assert_eq!(sold.status, AuctionViewStatus::Sold);
        assert_eq!(sold.price, Some(2_500_000.0));
        assert_eq!(sold.buyer.as_deref(), Some("SomeOne"));

        let expired = &entries[2];
        assert_eq!(expired.status, AuctionViewStatus::Expired);
        assert_eq!(expired.price, Some(500_000.0));
    }

    #[test]
    fn manage_auction_entries_ignores_non_manage_windows() {
        let window = WindowSnapshot {
            title: "Create Auction".to_string(),
            slots: vec![slot(10, "Something", &["Seller: x1f4r"])],
        };
        assert!(manage_auction_entries(&window).is_none());
    }
}
