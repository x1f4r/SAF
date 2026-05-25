use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSlot {
    pub slot: usize,
    pub name: String,
    pub display_name: String,
    pub lore: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_uuid: Option<String>,
}

impl WindowSlot {
    pub fn text(&self) -> String {
        std::iter::once(self.display_name.as_str())
            .chain(self.lore.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("\n")
            .to_ascii_lowercase()
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowSnapshot {
    pub title: String,
    pub slots: Vec<WindowSlot>,
}

impl WindowSnapshot {
    pub fn resolve_slot(&self, patterns: &[&str], fallback: Option<usize>) -> Option<usize> {
        self.slots
            .iter()
            .find(|slot| {
                let text = slot.text();
                patterns
                    .iter()
                    .any(|pattern| text.contains(&pattern.to_ascii_lowercase()))
            })
            .map(|slot| slot.slot)
            .or(fallback)
    }

    pub fn auction_action_slot(&self, fallback: usize) -> usize {
        self.slots
            .iter()
            .find(|slot| is_auction_action_slot(slot))
            .map(|slot| slot.slot)
            .unwrap_or(fallback)
    }
}

pub fn is_auction_action_slot(slot: &WindowSlot) -> bool {
    if slot.name == "bed" || slot.name.ends_with("_bed") {
        return true;
    }

    matches!(
        slot.name.as_str(),
        "gold_nugget" | "gold_block" | "poisonous_potato" | "potato" | "feather"
    ) || {
        let text = action_label_text(slot);
        [
            "buy item",
            "buy it now",
            "click to buy",
            "claim",
            "collect",
            "confirm",
            "purchase",
            "auction ended",
            "already bought",
            "already sold",
            "already claimed",
            "not enough coins",
            "too late",
            "cancel auction",
            "remove auction",
            "delist",
        ]
        .iter()
        .any(|pattern| text.contains(pattern))
    }
}

fn action_label_text(slot: &WindowSlot) -> String {
    [slot.display_name.as_str(), slot.name.as_str()]
        .into_iter()
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_slot_skips_decorative_fallback() {
        let window = WindowSnapshot {
            title: "BIN Auction View".to_string(),
            slots: vec![
                WindowSlot {
                    slot: 31,
                    name: "stained_glass_pane".to_string(),
                    display_name: " ".to_string(),
                    lore: vec![],
                    item_uuid: None,
                },
                WindowSlot {
                    slot: 33,
                    name: "gold_nugget".to_string(),
                    display_name: "Buy Item".to_string(),
                    lore: vec!["Click to buy".to_string()],
                    item_uuid: None,
                },
            ],
        };

        assert_eq!(window.auction_action_slot(31), 33);
    }

    #[test]
    fn action_slot_ignores_real_auction_item_lore() {
        let window = WindowSnapshot {
            title: "BIN Auction View".to_string(),
            slots: vec![
                WindowSlot {
                    slot: 13,
                    name: "player_head".to_string(),
                    display_name: "[Lvl 100] Blaze".to_string(),
                    lore: vec![
                        "Buy it now: 70,000,000 coins".to_string(),
                        "Seller: example".to_string(),
                        "Click to inspect this auction".to_string(),
                    ],
                    item_uuid: None,
                },
                WindowSlot {
                    slot: 22,
                    name: "gold_nugget".to_string(),
                    display_name: "Buy Item".to_string(),
                    lore: vec!["Click to buy this auction".to_string()],
                    item_uuid: None,
                },
            ],
        };

        assert_eq!(window.auction_action_slot(31), 22);
    }
}
