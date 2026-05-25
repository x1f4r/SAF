use super::super::{LiveRuntime, MARKET_STEP_RETRY_INTERVAL, now_ms};
use anyhow::Result;
use saf_core::gui::{WindowSlot, WindowSnapshot, is_auction_action_slot};
use saf_core::ports::{MinecraftAction, Notification};
use saf_core::{
    AccountId, AuctionId, MarketInstruction, MarketWorkflow, flip::ihate_taxes,
    relist::parse_old_price_from_lore_line,
};
use std::time::{Duration, Instant};

const BED_SPAM_WINDOW_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_TIMED_BED_CLICKS: u8 = 5;
const TIMED_BED_CLICK_DELAY: Duration = Duration::from_millis(3);
const TIMED_BED_CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
const BUY_ACTION_RETRY_DELAY: Duration = Duration::from_millis(55);
const BUY_ACTION_CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);
const LIVE_BUY_PRICE_TOLERANCE_RATIO: f64 = 1.05;
const LIVE_BUY_PRICE_TOLERANCE_COINS: f64 = 100_000.0;
const LIVE_BUY_MISSING_PRICE_WAIT_TIMEOUT: Duration = Duration::from_millis(650);

#[derive(Clone, Debug)]
pub(in crate::live_runtime) struct PendingLiveBuy {
    pub(in crate::live_runtime) auction_id: AuctionId,
    pub(in crate::live_runtime) expected_price: f64,
    pub(in crate::live_runtime) target_price: f64,
    pub(in crate::live_runtime) created_at: Instant,
    pub(in crate::live_runtime) created_at_ms: u64,
    pub(in crate::live_runtime) purchase_at_ms: Option<u64>,
    pub(in crate::live_runtime) click_at: Option<Instant>,
    pub(in crate::live_runtime) bed_spam: bool,
    pub(in crate::live_runtime) bed_click_delay: Duration,
    pub(in crate::live_runtime) bed_spam_until: Option<Instant>,
    pub(in crate::live_runtime) timed_bed_clicks: u8,
    pub(in crate::live_runtime) timed_bed_cleanup_at: Option<Instant>,
    pub(in crate::live_runtime) action_clicks: u8,
    pub(in crate::live_runtime) action_clicked_at: Option<Instant>,
    pub(in crate::live_runtime) last_click: Option<Instant>,
    pub(in crate::live_runtime) last_attempt: Option<Instant>,
}

impl LiveRuntime {
    pub(in crate::live_runtime) fn pending_live_buy(
        &self,
        account: &AccountId,
    ) -> Result<Option<PendingLiveBuy>> {
        Ok(self
            .pending_live_buys
            .lock()
            .map_err(|_| anyhow::anyhow!("pending live-buy lock poisoned"))?
            .get(account)
            .cloned())
    }

    fn mark_pending_live_buy_attempt(&self, account: &AccountId) -> Result<()> {
        if let Some(pending) = self
            .pending_live_buys
            .lock()
            .map_err(|_| anyhow::anyhow!("pending live-buy lock poisoned"))?
            .get_mut(account)
        {
            pending.last_attempt = Some(Instant::now());
        }
        Ok(())
    }

    fn mark_pending_live_bed_click(&self, account: &AccountId) -> Result<()> {
        let now = Instant::now();
        if let Some(pending) = self
            .pending_live_buys
            .lock()
            .map_err(|_| anyhow::anyhow!("pending live-buy lock poisoned"))?
            .get_mut(account)
        {
            pending.last_click = Some(now);
            pending.last_attempt = Some(now);
            if pending.bed_spam {
                pending
                    .bed_spam_until
                    .get_or_insert(now + BED_SPAM_WINDOW_TIMEOUT);
            } else {
                pending.timed_bed_clicks = pending.timed_bed_clicks.saturating_add(1);
                if pending.timed_bed_clicks >= MAX_TIMED_BED_CLICKS {
                    pending
                        .timed_bed_cleanup_at
                        .get_or_insert(now + TIMED_BED_CLEANUP_TIMEOUT);
                }
            }
        }
        Ok(())
    }

    fn mark_pending_live_action_click(&self, account: &AccountId) -> Result<()> {
        let now = Instant::now();
        if let Some(pending) = self
            .pending_live_buys
            .lock()
            .map_err(|_| anyhow::anyhow!("pending live-buy lock poisoned"))?
            .get_mut(account)
        {
            pending.action_clicks = pending.action_clicks.saturating_add(1);
            pending.action_clicked_at.get_or_insert(now);
            pending.last_click = Some(now);
            pending.last_attempt = Some(now);
        }
        Ok(())
    }

    pub(in crate::live_runtime) fn remove_pending_live_buy(
        &self,
        account: &AccountId,
    ) -> Result<()> {
        self.pending_live_buys
            .lock()
            .map_err(|_| anyhow::anyhow!("pending live-buy lock poisoned"))?
            .remove(account);
        Ok(())
    }

    async fn close_pending_live_buy_window(
        &mut self,
        account: &AccountId,
        action: &str,
    ) -> Result<()> {
        if let Some(client) = self.minecraft_clients.get(account)
            && let Err(error) = client.perform(MinecraftAction::CloseWindow).await
        {
            self.handle_minecraft_action_failure(account, action, error.into())
                .await?;
        }
        self.clear_active_window_cache(account)?;
        self.remove_pending_live_buy(account)?;
        Ok(())
    }

    pub(in crate::live_runtime) async fn process_pending_live_buys_once(&mut self) -> Result<()> {
        let running = self.running_account_set();
        let accounts = self
            .pending_live_buys
            .lock()
            .map_err(|_| anyhow::anyhow!("pending live-buy lock poisoned"))?
            .keys()
            .cloned()
            .collect::<Vec<_>>();

        for account in accounts {
            if !running.contains(&account) {
                self.remove_pending_live_buy(&account)?;
                continue;
            }
            if !self.account_market_ready(&account) {
                continue;
            }
            let Some(pending) = self.pending_live_buy(&account)? else {
                continue;
            };
            let (window, received_at) = {
                let window = self
                    .active_windows
                    .lock()
                    .map_err(|_| anyhow::anyhow!("active window lock poisoned"))?
                    .get(&account)
                    .cloned();
                let received_at = self
                    .active_window_received_at
                    .lock()
                    .map_err(|_| anyhow::anyhow!("active window received timestamp lock poisoned"))?
                    .get(&account)
                    .copied();
                (window, received_at)
            };
            let window = if received_at.is_some_and(|received_at| received_at > pending.created_at)
            {
                window
            } else {
                None
            };
            if let Some(window) = &window
                && !is_live_buy_window(window)
            {
                let now_ms = now_ms();
                tracing::warn!(
                    account = %account,
                    auction_id = %pending.auction_id,
                    window_title = %window.title,
                    pending_age_ms = elapsed_millis(pending.created_at),
                    purchase_at_ms = ?pending.purchase_at_ms,
                    purchase_lag_ms = ?pending
                        .purchase_at_ms
                        .map(|purchase_at_ms| signed_millis_delta(now_ms, purchase_at_ms)),
                    "closing unrelated window before retrying pending live Cofl buy"
                );
                if let Err(error) = self
                    .session
                    .execute_market_instruction(&account, &MarketInstruction::CloseWindow)
                    .await
                {
                    tracing::warn!(
                        account = %account,
                        auction_id = %pending.auction_id,
                        error = %error,
                        "failed to close unrelated window before pending live Cofl buy retry"
                    );
                }
                self.clear_active_window_cache(&account)?;
                self.mark_pending_live_buy_attempt(&account)?;
                continue;
            }
            if window.is_none()
                && pending
                    .action_clicked_at
                    .is_some_and(|clicked_at| clicked_at.elapsed() >= BUY_ACTION_CLEANUP_TIMEOUT)
            {
                tracing::info!(
                    account = %account,
                    auction_id = %pending.auction_id,
                    pending_age_ms = elapsed_millis(pending.created_at),
                    action_clicks = pending.action_clicks,
                    "live Cofl buy action produced no follow-up window before cleanup; removing pending buy"
                );
                self.remove_pending_live_buy(&account)?;
                continue;
            }
            if window.is_none()
                && pending
                    .last_attempt
                    .is_some_and(|attempt| attempt.elapsed() < MARKET_STEP_RETRY_INTERVAL)
            {
                continue;
            }
            let step = MarketWorkflow::BuyNow {
                auction_id: pending.auction_id.clone(),
            }
            .next_step(window.as_ref());
            let click_slot = match step.instruction {
                MarketInstruction::ClickSlot { slot } => Some(slot),
                _ => None,
            };
            let bed_action = window
                .as_ref()
                .zip(click_slot)
                .is_some_and(|(window, slot)| is_bed_action_slot(window, slot));
            if click_slot.is_some() {
                let now = Instant::now();
                if pending.click_at.is_some_and(|click_at| click_at > now) {
                    continue;
                }
                if pending.action_clicks == 0
                    && !window.as_ref().is_some_and(is_confirm_purchase_window)
                    && let Some(window) = &window
                    && let Err(reason) = validate_live_buy_window_price(&pending, window)
                {
                    if matches!(reason, LiveBuyPriceValidationError::MissingVisiblePrice)
                        && pending.created_at.elapsed() < LIVE_BUY_MISSING_PRICE_WAIT_TIMEOUT
                    {
                        tracing::debug!(
                            account = %account,
                            auction_id = %pending.auction_id,
                            expected_price = pending.expected_price,
                            target_price = pending.target_price,
                            window_title = %window.title,
                            price_candidates = ?visible_live_buy_price_candidates(window),
                            pending_age_ms = elapsed_millis(pending.created_at),
                            wait_timeout_ms = LIVE_BUY_MISSING_PRICE_WAIT_TIMEOUT.as_millis(),
                            "waiting for live Cofl buy window to expose a visible price"
                        );
                        continue;
                    }
                    let reason = reason.to_string();
                    tracing::warn!(
                        account = %account,
                        auction_id = %pending.auction_id,
                        expected_price = pending.expected_price,
                        target_price = pending.target_price,
                        window_title = %window.title,
                        price_candidates = ?visible_live_buy_price_candidates(window),
                        reason = %reason,
                        "blocked live Cofl buy because the visible auction price was unsafe"
                    );
                    if let Err(error) = self
                        .session
                        .notify(Notification {
                            title: "Blocked Cofl Buy".to_string(),
                            body: format!(
                                "`{}` was not bought because the auction window price was unsafe: {reason}",
                                pending.auction_id
                            ),
                            account: Some(account.clone()),
                        })
                        .await
                    {
                        tracing::warn!(
                            account = %account,
                            auction_id = %pending.auction_id,
                            error = %error,
                            "failed to notify operator about blocked live Cofl buy"
                        );
                    }
                    self.close_pending_live_buy_window(
                        &account,
                        "blocked unsafe live Cofl buy price",
                    )
                    .await?;
                    continue;
                }
                if bed_action
                    && pending.bed_spam
                    && pending.bed_spam_until.is_some_and(|until| until <= now)
                {
                    self.close_pending_live_buy_window(&account, "bed spam window cleanup")
                        .await?;
                    continue;
                }
                if bed_action
                    && pending.bed_spam
                    && pending.last_click.is_some_and(|last_click| {
                        now.duration_since(last_click) < pending.bed_click_delay
                    })
                {
                    continue;
                }
                if bed_action && !pending.bed_spam {
                    if let Some(cleanup_at) = pending.timed_bed_cleanup_at {
                        if cleanup_at <= now {
                            self.close_pending_live_buy_window(
                                &account,
                                "timed bed window cleanup",
                            )
                            .await?;
                        }
                        continue;
                    }
                    if pending.last_click.is_some_and(|last_click| {
                        now.duration_since(last_click) < TIMED_BED_CLICK_DELAY
                    }) {
                        continue;
                    }
                }
                if !bed_action && let Some(action_clicked_at) = pending.action_clicked_at {
                    if action_clicked_at.elapsed() >= BUY_ACTION_CLEANUP_TIMEOUT {
                        tracing::info!(
                            account = %account,
                            auction_id = %pending.auction_id,
                            window_title = window.as_ref().map(|window| window.title.as_str()).unwrap_or("none"),
                            pending_age_ms = elapsed_millis(pending.created_at),
                            action_clicks = pending.action_clicks,
                            "live Cofl buy action did not reach confirm purchase before cleanup; closing window"
                        );
                        self.close_pending_live_buy_window(
                            &account,
                            "live buy action window cleanup",
                        )
                        .await?;
                        continue;
                    }
                    if !window.as_ref().is_some_and(is_confirm_purchase_window) {
                        continue;
                    }
                    if pending.last_click.is_some_and(|last_click| {
                        now.duration_since(last_click) < BUY_ACTION_RETRY_DELAY
                    }) {
                        continue;
                    }
                }
            }
            if matches!(step.instruction, MarketInstruction::Noop) {
                self.remove_pending_live_buy(&account)?;
                continue;
            }

            match self
                .session
                .execute_market_instruction(&account, &step.instruction)
                .await
            {
                Ok(_) => {
                    let now_ms = now_ms();
                    tracing::info!(
                        account = %account,
                        auction_id = %pending.auction_id,
                        instruction = ?step.instruction,
                        reason = %step.reason,
                        window_title = window.as_ref().map(|window| window.title.as_str()).unwrap_or("none"),
                        click_slot = ?click_slot,
                        bed_action = bed_action,
                        bed_spam = pending.bed_spam,
                        pending_age_ms = elapsed_millis(pending.created_at),
                        pending_wall_age_ms = now_ms.saturating_sub(pending.created_at_ms),
                        purchase_at_ms = ?pending.purchase_at_ms,
                        purchase_lag_ms = ?pending
                            .purchase_at_ms
                            .map(|purchase_at_ms| signed_millis_delta(now_ms, purchase_at_ms)),
                        timed_bed_clicks = pending.timed_bed_clicks,
                        action_clicks = pending.action_clicks,
                        done = step.done,
                        "executed pending live Cofl buy step"
                    );
                    self.processed_queue_steps += 1;
                    if step.done && matches!(step.instruction, MarketInstruction::CloseWindow) {
                        self.remove_pending_live_buy(&account)?;
                    } else if step.done && bed_action {
                        self.mark_pending_live_bed_click(&account)?;
                    } else if step.done && click_slot.is_some() {
                        self.mark_pending_live_action_click(&account)?;
                    } else if step.done {
                        self.remove_pending_live_buy(&account)?;
                    } else {
                        self.mark_pending_live_buy_attempt(&account)?;
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        account = %account,
                        reason = %step.reason,
                        error = %error,
                        "live Cofl buy step failed; retrying while auction window is pending"
                    );
                    self.mark_pending_live_buy_attempt(&account)?;
                }
            }
        }
        Ok(())
    }
}

pub(super) fn timed_bed_click_at(
    purchase_at_ms: Option<u64>,
    click_offset: Duration,
) -> Option<Instant> {
    let purchase_at_ms = purchase_at_ms?;
    let offset_ms = click_offset.as_millis().min(u128::from(u64::MAX)) as u64;
    let click_at_ms = purchase_at_ms.saturating_sub(offset_ms);
    let now = now_ms();
    (click_at_ms > now).then(|| Instant::now() + Duration::from_millis(click_at_ms - now))
}

fn elapsed_millis(instant: Instant) -> u64 {
    instant.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn signed_millis_delta(now_ms: u64, reference_ms: u64) -> i64 {
    if now_ms >= reference_ms {
        now_ms.saturating_sub(reference_ms).min(i64::MAX as u64) as i64
    } else {
        -(reference_ms.saturating_sub(now_ms).min(i64::MAX as u64) as i64)
    }
}

fn is_bed_action_slot(window: &WindowSnapshot, slot: usize) -> bool {
    window
        .slots
        .iter()
        .find(|candidate| candidate.slot == slot)
        .is_some_and(|candidate| candidate.name == "bed" || candidate.name.ends_with("_bed"))
}

fn is_live_buy_window(window: &WindowSnapshot) -> bool {
    let title = window.title.to_ascii_lowercase();
    if title.contains("auction view") || title.contains("view auction") {
        return true;
    }
    if is_confirm_purchase_window(window) {
        return true;
    }
    let text = std::iter::once(window.title.as_str())
        .chain(window.slots.iter().flat_map(|slot| {
            std::iter::once(slot.display_name.as_str()).chain(slot.lore.iter().map(String::as_str))
        }))
        .collect::<Vec<_>>()
        .join("\n")
        .to_ascii_lowercase();
    text.contains("buy item right now")
        || text.contains("click to buy")
        || text.contains("already bought")
        || text.contains("too late")
        || text.contains("not enough coins")
        || text.contains("auction ended")
}

fn is_confirm_purchase_window(window: &WindowSnapshot) -> bool {
    window
        .title
        .to_ascii_lowercase()
        .contains("confirm purchase")
}

fn validate_live_buy_window_price(
    pending: &PendingLiveBuy,
    window: &WindowSnapshot,
) -> Result<(), LiveBuyPriceValidationError> {
    let Some(visible_price) = visible_live_buy_price(window) else {
        return Err(LiveBuyPriceValidationError::MissingVisiblePrice);
    };
    let max_expected_price = max_expected_live_buy_price(pending.expected_price);
    if visible_price > max_expected_price {
        return Err(LiveBuyPriceValidationError::Unsafe(format!(
            "visible price {visible_price:.0} exceeds expected Cofl price {:.0} plus tolerance ({max_expected_price:.0})",
            pending.expected_price
        )));
    }
    let post_tax_target = ihate_taxes(pending.target_price);
    if !post_tax_target.is_finite() || post_tax_target <= visible_price {
        return Err(LiveBuyPriceValidationError::Unsafe(format!(
            "visible price {visible_price:.0} is not profitable against target {:.0}",
            pending.target_price
        )));
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum LiveBuyPriceValidationError {
    MissingVisiblePrice,
    Unsafe(String),
}

impl std::fmt::Display for LiveBuyPriceValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingVisiblePrice => {
                formatter.write_str("no visible buy price was found in the auction window")
            }
            Self::Unsafe(reason) => formatter.write_str(reason),
        }
    }
}

fn max_expected_live_buy_price(expected_price: f64) -> f64 {
    if !expected_price.is_finite() || expected_price <= 0.0 {
        return 0.0;
    }
    (expected_price * LIVE_BUY_PRICE_TOLERANCE_RATIO)
        .max(expected_price + LIVE_BUY_PRICE_TOLERANCE_COINS)
}

fn visible_live_buy_price(window: &WindowSnapshot) -> Option<f64> {
    if let Some(price) = window.slots.iter().find_map(|slot| {
        std::iter::once(slot.display_name.as_str())
            .chain(slot.lore.iter().map(String::as_str))
            .find_map(visible_live_buy_price_line)
    }) {
        return Some(price);
    }

    window
        .slots
        .iter()
        .filter(|slot| is_live_buy_action_slot(slot))
        .flat_map(|slot| {
            std::iter::once(slot.display_name.as_str()).chain(slot.lore.iter().map(String::as_str))
        })
        .find_map(standalone_live_buy_action_price_line)
}

fn visible_live_buy_price_line(line: &str) -> Option<f64> {
    let lower = line.to_ascii_lowercase();
    let has_price_marker = lower.contains("buy it now")
        || lower.contains("starting bid")
        || (lower.contains("price") && lower.contains("coins"));
    if !has_price_marker {
        return None;
    }
    let value = line.split_once(':').map(|(_, value)| value).unwrap_or(line);
    parse_old_price_from_lore_line(value)
}

fn standalone_live_buy_action_price_line(line: &str) -> Option<f64> {
    let lower = line.to_ascii_lowercase();
    if !lower.contains("coins") || !line.chars().any(|ch| ch.is_ascii_digit()) {
        return None;
    }
    parse_old_price_from_lore_line(line)
}

fn is_live_buy_action_slot(slot: &WindowSlot) -> bool {
    let name = slot.name.to_ascii_lowercase();
    let display_name = slot.display_name.to_ascii_lowercase();
    is_auction_action_slot(slot)
        || matches!(
            name.as_str(),
            "gold_nugget" | "gold_ingot" | "gold_block" | "poisonous_potato" | "potato" | "feather"
        )
        || name == "bed"
        || name.ends_with("_bed")
        || display_name.contains("buy item")
        || display_name.contains("buy it now")
        || display_name.contains("click to buy")
        || display_name.contains("purchase")
}

fn visible_live_buy_price_candidates(window: &WindowSnapshot) -> Vec<String> {
    window
        .slots
        .iter()
        .filter(|slot| is_live_buy_action_slot(slot))
        .flat_map(|slot| {
            std::iter::once(slot.display_name.as_str()).chain(slot.lore.iter().map(String::as_str))
        })
        .filter(|line| line.chars().any(|ch| ch.is_ascii_digit()))
        .take(8)
        .map(ToString::to_string)
        .collect()
}
