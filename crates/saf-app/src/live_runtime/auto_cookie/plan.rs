use super::{COOKIE_ACTIVATION_DELAY, COOKIE_DURATION, COOKIE_PURCHASE_TIMEOUT, MAX_COOKIE_PRICE};
use saf_core::gui::WindowSnapshot;
use saf_core::ports::{InventoryItem, MinecraftAction};
use serde_json::Value;
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub(in crate::live_runtime) struct PendingAutoCookie {
    started_at: Instant,
    pub(in crate::live_runtime) phase: AutoCookiePhase,
    pub(in crate::live_runtime) previous_duration: Option<Duration>,
    pub(in crate::live_runtime) price: f64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::live_runtime) enum AutoCookiePhase {
    AwaitingBazaar,
    AwaitingAmount,
    AwaitingConfirm,
    PreparingActivation { due_at: Instant },
    AwaitingConsumeConfirm,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum AutoCookieWindowAction {
    Click { slot: usize, next: AutoCookiePhase },
    ClickAndPrepareActivation { slot: usize, due_at: Instant },
    ClickConsume { slot: usize },
}

impl PendingAutoCookie {
    pub(super) fn new(price: f64, previous_duration: Option<Duration>, now: Instant) -> Self {
        Self {
            started_at: now,
            phase: AutoCookiePhase::AwaitingBazaar,
            previous_duration,
            price,
        }
    }

    pub(super) fn timed_out(&self, now: Instant) -> bool {
        now.duration_since(self.started_at) >= COOKIE_PURCHASE_TIMEOUT
    }

    #[cfg(test)]
    pub(in crate::live_runtime) fn force_timeout_for_test(&mut self) {
        self.started_at = Instant::now()
            .checked_sub(COOKIE_PURCHASE_TIMEOUT + Duration::from_secs(1))
            .unwrap_or_else(Instant::now);
    }

    pub(super) fn plan_window(
        &self,
        window: &WindowSnapshot,
        now: Instant,
    ) -> Option<AutoCookieWindowAction> {
        match self.phase {
            AutoCookiePhase::AwaitingBazaar => {
                bazaar_cookie_buy_slot(window).map(|slot| AutoCookieWindowAction::Click {
                    slot,
                    next: AutoCookiePhase::AwaitingAmount,
                })
            }
            AutoCookiePhase::AwaitingAmount => {
                bazaar_cookie_amount_slot(window).map(|slot| AutoCookieWindowAction::Click {
                    slot,
                    next: AutoCookiePhase::AwaitingConfirm,
                })
            }
            AutoCookiePhase::AwaitingConfirm => bazaar_cookie_confirm_slot(window).map(|slot| {
                AutoCookieWindowAction::ClickAndPrepareActivation {
                    slot,
                    due_at: now + COOKIE_ACTIVATION_DELAY,
                }
            }),
            AutoCookiePhase::PreparingActivation { .. } => None,
            AutoCookiePhase::AwaitingConsumeConfirm => cookie_consume_slot(window)
                .map(|slot| AutoCookieWindowAction::ClickConsume { slot }),
        }
    }

    pub(super) fn activated_cookie_duration(&self) -> Duration {
        self.previous_duration
            .and_then(|duration| duration.checked_add(COOKIE_DURATION))
            .unwrap_or(COOKIE_DURATION)
    }
}

pub(super) fn auto_cookie_threshold(raw: &str) -> Option<Duration> {
    saf_core::time::normal_time(raw).filter(|duration| !duration.is_zero())
}

pub(super) fn should_buy_cookie(
    threshold: Option<Duration>,
    remaining: Option<Duration>,
    purse: Option<f64>,
    price: Option<f64>,
) -> AutoCookieDecision {
    let Some(threshold) = threshold else {
        return AutoCookieDecision::Disabled;
    };
    if let Some(remaining) = remaining
        && remaining > threshold
    {
        return AutoCookieDecision::EnoughTime;
    }
    let Some(purse) = purse.filter(|value| value.is_finite() && *value >= 0.0) else {
        return AutoCookieDecision::MissingPurse;
    };
    let Some(price) = price.filter(|value| value.is_finite() && *value > 0.0) else {
        return AutoCookieDecision::PriceUnavailable;
    };
    if price > MAX_COOKIE_PRICE || purse < price * 2.0 {
        return AutoCookieDecision::TooExpensive { price, purse };
    }
    AutoCookieDecision::Buy { price }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum AutoCookieDecision {
    Disabled,
    EnoughTime,
    MissingPurse,
    PriceUnavailable,
    TooExpensive { price: f64, purse: f64 },
    Buy { price: f64 },
}

pub(super) fn dry_run_auto_cookie_action(
    remaining: Option<Duration>,
    threshold: Duration,
    price: f64,
) -> Value {
    serde_json::json!({
        "kind": "autoCookie",
        "remainingMs": remaining.map(|duration| duration.as_millis() as u64),
        "thresholdMs": threshold.as_millis() as u64,
        "price": price
    })
}

pub(super) fn activation_actions_for_cookie(
    items: &[InventoryItem],
) -> Option<Vec<MinecraftAction>> {
    let cookie = items.iter().find(|item| is_booster_cookie(item))?;
    let slot = usize::from(cookie.slot?);
    let hotbar_slot = if cookie.in_hotbar && (36..=44).contains(&slot) {
        u8::try_from(slot - 36).ok()?
    } else {
        first_empty_hotbar_slot(items)?
    };
    let mut actions = Vec::new();
    if !cookie.in_hotbar || !(36..=44).contains(&slot) {
        actions.push(MinecraftAction::SwapSlotToHotbar { slot, hotbar_slot });
    }
    actions.push(MinecraftAction::SetHeldHotbarSlot(hotbar_slot));
    actions.push(MinecraftAction::ActivateHeldItem);
    Some(actions)
}

pub(super) fn full_inventory_cookie_message(text: &str) -> bool {
    let normalized = text.to_ascii_lowercase();
    normalized.contains("didn't fit in your inventory")
        && normalized.contains("item stash")
        && normalized.contains("click here")
}

fn first_empty_hotbar_slot(items: &[InventoryItem]) -> Option<u8> {
    let occupied = items
        .iter()
        .filter_map(|item| item.slot)
        .filter(|slot| (36..=44).contains(slot))
        .collect::<BTreeSet<_>>();
    (36_u8..=44)
        .find(|slot| !occupied.contains(slot))
        .map(|slot| slot - 36)
}

fn is_booster_cookie(item: &InventoryItem) -> bool {
    item.tag
        .as_deref()
        .is_some_and(|tag| tag.eq_ignore_ascii_case("BOOSTER_COOKIE"))
        || item
            .item_name
            .to_ascii_lowercase()
            .contains("booster cookie")
        || item
            .lore
            .iter()
            .any(|line| line.to_ascii_lowercase().contains("booster cookie"))
}

fn bazaar_cookie_buy_slot(window: &WindowSnapshot) -> Option<usize> {
    if !window_matches_any(window, &["bazaar", "booster cookie"]) {
        return None;
    }
    window.resolve_slot(&["buy instantly", "instant buy", "buy price"], Some(11))
}

fn bazaar_cookie_amount_slot(window: &WindowSnapshot) -> Option<usize> {
    window.resolve_slot(
        &["buy one", "buy 1", "1x", "custom amount", "buy instantly"],
        Some(10),
    )
}

fn bazaar_cookie_confirm_slot(window: &WindowSnapshot) -> Option<usize> {
    window.resolve_slot(
        &[
            "confirm",
            "buy item",
            "submit",
            "buy instantly",
            "buy only one",
            "click to buy now",
            "amount: 1x",
        ],
        Some(10),
    )
}

fn cookie_consume_slot(window: &WindowSnapshot) -> Option<usize> {
    let text = window_text(window);
    if !(text.contains("booster cookie") && (text.contains("consume") || text.contains("confirm")))
    {
        return None;
    }
    window.resolve_slot(&["consume", "booster cookie", "confirm"], Some(11))
}

fn window_matches_any(window: &WindowSnapshot, patterns: &[&str]) -> bool {
    let text = format!("{}\n{}", window.title, window_text(window)).to_ascii_lowercase();
    patterns
        .iter()
        .any(|pattern| text.contains(&pattern.to_ascii_lowercase()))
}

fn window_text(window: &WindowSnapshot) -> String {
    window
        .slots
        .iter()
        .map(|slot| slot.text())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn force_zero_remaining_decides_buy() {
        // A manual "cookie" force passes remaining = ZERO, which is never above
        // the threshold, so affordability alone decides — and it buys.
        let threshold = Duration::from_secs(3600);
        assert_eq!(
            should_buy_cookie(
                Some(threshold),
                Some(Duration::ZERO),
                Some(10_000_000.0),
                Some(1_000_000.0),
            ),
            AutoCookieDecision::Buy {
                price: 1_000_000.0,
            }
        );
    }

    #[test]
    fn fresh_cookie_skips_without_force() {
        // Without the force (plenty of remaining time), the same affordable
        // account is left alone.
        let threshold = Duration::from_secs(3600);
        assert_eq!(
            should_buy_cookie(
                Some(threshold),
                Some(Duration::from_secs(7200)),
                Some(10_000_000.0),
                Some(1_000_000.0),
            ),
            AutoCookieDecision::EnoughTime
        );
    }

    #[test]
    fn force_still_respects_affordability() {
        // "Force" means ignore remaining time, NOT ignore affordability: a purse
        // below 2x the price must not buy.
        let threshold = Duration::from_secs(3600);
        assert!(matches!(
            should_buy_cookie(
                Some(threshold),
                Some(Duration::ZERO),
                Some(1_000_000.0),
                Some(1_000_000.0),
            ),
            AutoCookieDecision::TooExpensive { .. }
        ));
    }
}
