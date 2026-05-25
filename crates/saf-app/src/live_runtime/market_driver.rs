use super::auction_flow::{
    PendingAuctionDraft, is_bank_queue_entry, is_open_bank_instruction, is_reconcile_queue_entry,
    pending_create_auction_draft, should_recover_pending_draft,
};
use super::bank::BankCooldownDecision;
use super::listing_safety::unsafe_listing_entry_reason;
use super::notifier::notify_operator_best_effort;
use super::support::{number_value, string_value, strip_minecraft_color_codes};
use super::{
    CLAIM_PURCHASED_OPEN_MAX_ATTEMPTS, LiveRuntime, MARKET_STEP_RETRY_INTERVAL,
    MARKET_WINDOW_SETTLE_DELAY, MISSING_LISTING_INVENTORY_MAX_ATTEMPTS,
    MISSING_LISTING_INVENTORY_RETRY_DELAY, PendingCompletionKind, PendingListingPriceMismatchRetry,
    PendingMarketStep, PendingOpenAuctionRetry,
};
use anyhow::Result;
use saf_core::gui::WindowSnapshot;
use saf_core::ports::{Notification, QueueStore};
use saf_core::{AccountId, BotState, MarketInstruction, QueueEntry};
use std::time::Instant;

impl LiveRuntime {
    pub(super) async fn process_market_queue_once(&mut self) -> Result<()> {
        for account in self.running_accounts() {
            if !self.account_market_ready(&account) {
                self.pending_market_steps.remove(&account);
                self.pending_open_auction_retries.remove(&account);
                continue;
            }
            #[cfg(feature = "live-cofl")]
            if self.pending_live_buy(&account)?.is_some() {
                self.pending_market_steps.remove(&account);
                self.pending_open_auction_retries.remove(&account);
                continue;
            }
            let queue = match self.queue.snapshot(&account).await {
                Ok(queue) => queue,
                Err(error) => {
                    tracing::warn!(
                        account = %account,
                        error = %error,
                        "failed to inspect market queue; skipping account for this poll"
                    );
                    continue;
                }
            };
            let window = self
                .active_windows
                .lock()
                .map_err(|_| anyhow::anyhow!("active window lock poisoned"))?
                .get(&account)
                .cloned();
            let entry = if let Some(entry) =
                self.pending_listing_confirmation_window_entry(&account, &queue, window.as_ref())
            {
                entry
            } else if let Some(entry) = self
                .prioritize_pending_draft_entry(&account, &queue, window.as_ref())
                .await?
            {
                entry
            } else if let Some(entry) =
                self.prioritize_reconcile_entry(&account, &queue, window.as_ref())
            {
                entry
            } else {
                let Some(entry) = queue
                    .iter()
                    .filter(|entry| !self.is_completion_pending(&account, entry))
                    .find(|entry| self.session.plan_market_queue_entry(entry, None).is_some())
                    .cloned()
                else {
                    self.pending_market_steps.remove(&account);
                    self.pending_open_auction_retries.remove(&account);
                    continue;
                };
                entry
            };
            if self.missing_listing_inventory_retry_is_pending(&account, &entry) {
                continue;
            }
            if self.listing_price_mismatch_retry_is_pending(&account, &entry) {
                continue;
            }
            if self.reject_unsafe_listing_entry(&account, &entry).await? {
                continue;
            }

            if let Some(window) = &window
                && self
                    .complete_listing_confirmation_from_window(&account, window)
                    .await?
            {
                if let Err(error) = self
                    .session
                    .execute_market_instruction(&account, &MarketInstruction::CloseWindow)
                    .await
                {
                    tracing::warn!(
                        account = %account,
                        error = %error,
                        "failed to close active listing window after listing confirmation"
                    );
                }
                self.clear_active_window_cache(&account)?;
                continue;
            }
            if self.listing_confirmation_waiting_for_signal(&account, &entry, window.as_ref()) {
                continue;
            }
            if window.is_some() && self.live_market_window_is_settling(&account)? {
                continue;
            }
            if let Some(window) = &window
                && self
                    .handle_missing_listing_inventory_once(&account, &entry, window)
                    .await?
            {
                continue;
            }
            if let Some(window) = &window
                && self
                    .process_reconcile_window_once(&account, &entry, window)
                    .await?
            {
                continue;
            }
            if let Some(window) = &window
                && self
                    .process_bids_window_once(&account, &entry, window)
                    .await?
            {
                continue;
            }
            if let Some(window) = &window
                && self
                    .process_expired_window_once(&account, &entry, window)
                    .await?
            {
                continue;
            }
            let Some(step) = self
                .session
                .plan_market_queue_entry(&entry, window.as_ref())
            else {
                self.pending_market_steps.remove(&account);
                self.pending_open_auction_retries.remove(&account);
                continue;
            };
            tracing::debug!(
                account = %account,
                state = %entry.state.as_str(),
                priority = entry.priority,
                window = window.as_ref().map(|window| window.title.as_str()).unwrap_or("none"),
                instruction = ?step.instruction,
                reason = %step.reason,
                done = step.done,
                "planned queued market step"
            );

            let live_market_actions = self.options.market_actions.allows_market_actions();
            let has_instruction = !matches!(step.instruction, MarketInstruction::Noop);
            if has_instruction || !live_market_actions {
                if live_market_actions
                    && self
                        .handle_listing_price_mismatch_step(
                            &account,
                            &entry,
                            &step.instruction,
                            &step.reason,
                        )
                        .await?
                {
                    continue;
                }
                if !self.should_attempt_market_step(&account, &entry, &step.instruction) {
                    continue;
                }
                if live_market_actions
                    && self
                        .handle_stale_claim_purchased_open_retry(
                            &account,
                            &entry,
                            &step.instruction,
                            &step.reason,
                            window.as_ref(),
                        )
                        .await?
                {
                    continue;
                }
                if self.clear_stale_transition_window_if_needed(
                    &account,
                    &entry,
                    &step.instruction,
                    step.done,
                )? {
                    continue;
                }

                if live_market_actions {
                    if !self.guard_bank_cooldown(&account, &entry, &step.instruction)? {
                        self.remember_pending_market_step(&account, &entry, &step.instruction);
                        continue;
                    }
                    if !self
                        .execute_live_market_instruction_or_retry(
                            &account,
                            &entry,
                            &step.instruction,
                            &step.reason,
                        )
                        .await
                    {
                        continue;
                    }
                } else {
                    self.queue.record_dry_run_step(
                        &account,
                        &entry,
                        &step.instruction,
                        &step.reason,
                    )?;
                    tracing::info!(
                        account = %account,
                        state = %entry.state.as_str(),
                        priority = entry.priority,
                        instruction = ?step.instruction,
                        reason = %step.reason,
                        "recorded dry-run queued market step"
                    );
                }

                self.processed_queue_steps += 1;
                if step.done
                    && live_market_actions
                    && self.is_listing_confirmation_step(&entry, &step.instruction, &step.reason)
                {
                    self.remember_pending_listing_confirmation(&account, &entry);
                    self.remember_pending_market_step(&account, &entry, &step.instruction);
                    continue;
                }
                if step.done
                    && live_market_actions
                    && self
                        .complete_claim_purchased_entry_if_needed(
                            &account,
                            &entry,
                            &step.instruction,
                            &step.reason,
                        )
                        .await?
                {
                    continue;
                }
                if !step.done || !live_market_actions {
                    self.remember_pending_market_step(&account, &entry, &step.instruction);
                }
            }

            if step.done && live_market_actions {
                self.pending_market_steps.remove(&account);
                self.complete_queue_entry_or_defer(
                    &account,
                    &entry,
                    PendingCompletionKind::Generic,
                )
                .await?;
            }
        }
        Ok(())
    }

    fn pending_listing_confirmation_window_entry(
        &self,
        account: &AccountId,
        queue: &[QueueEntry],
        window: Option<&WindowSnapshot>,
    ) -> Option<QueueEntry> {
        if !self.options.market_actions.allows_market_actions() {
            return None;
        }
        let window = window?;
        if !market_window_is_listing_confirmation(window) {
            return None;
        }
        let pending = self.pending_market_steps.get(account)?;
        if !matches!(
            pending.entry.state,
            BotState::Listing | BotState::ListingNoName
        ) {
            return None;
        }
        if self.is_completion_pending(account, &pending.entry) {
            return None;
        }
        if !queue.iter().any(|entry| entry == &pending.entry) {
            return None;
        }
        tracing::debug!(
            account = %account,
            state = %pending.entry.state.as_str(),
            priority = pending.entry.priority,
            window = %window.title,
            "continuing pending listing through confirmation window"
        );
        Some(pending.entry.clone())
    }

    async fn prioritize_pending_draft_entry(
        &mut self,
        account: &AccountId,
        queue: &[QueueEntry],
        window: Option<&WindowSnapshot>,
    ) -> Result<Option<QueueEntry>> {
        if !self.options.market_actions.allows_market_actions() {
            return Ok(None);
        }
        let Some(window) = window else {
            return Ok(None);
        };
        let Some(draft) = pending_create_auction_draft(window) else {
            return Ok(None);
        };

        if let Some(entry) = queue
            .iter()
            .filter(|entry| !self.is_completion_pending(account, entry))
            .find(|entry| pending_draft_listing_entry_matches(entry, &draft))
        {
            tracing::debug!(
                account = %account,
                state = %entry.state.as_str(),
                priority = entry.priority,
                draft_inventory = %draft.item_uuid,
                draft_item = %draft.item_name,
                draft_price = draft.list_price,
                "prioritizing queued listing that matches active pending auction draft"
            );
            return Ok(Some(entry.clone()));
        }

        if let Some(entry) = queue
            .iter()
            .filter(|entry| !self.is_completion_pending(account, entry))
            .find(|entry| is_reconcile_queue_entry(entry) && should_recover_pending_draft(entry))
        {
            tracing::debug!(
                account = %account,
                state = %entry.state.as_str(),
                priority = entry.priority,
                draft_inventory = %draft.item_uuid,
                draft_item = %draft.item_name,
                draft_price = draft.list_price,
                "prioritizing auction reconcile for active pending auction draft"
            );
            return Ok(Some(entry.clone()));
        }

        let queued = self
            .queue
            .add(
                account,
                serde_json::json!({
                    "reason": "listing-status-unclear",
                    "inventory": draft.item_uuid.as_str(),
                    "inv": draft.item_uuid.as_str(),
                    "itemName": draft.item_name.as_str(),
                    "price": draft.list_price,
                }),
                BotState::Custom("reconcileAuctions".to_string()),
                2,
            )
            .await?;
        tracing::warn!(
            account = %account,
            draft_inventory = %draft.item_uuid,
            draft_item = %draft.item_name,
            draft_price = draft.list_price,
            queued,
            "queued auction reconcile before continuing listings because a pending auction draft is active"
        );
        Ok(None)
    }

    fn prioritize_reconcile_entry(
        &self,
        account: &AccountId,
        queue: &[QueueEntry],
        window: Option<&WindowSnapshot>,
    ) -> Option<QueueEntry> {
        if !self.options.market_actions.allows_market_actions() {
            return None;
        }
        if window.is_some_and(|window| {
            market_window_is_listing_confirmation(window)
                || pending_create_auction_draft(window).is_some()
        }) {
            return None;
        }
        let entry = queue
            .iter()
            .filter(|entry| !self.is_completion_pending(account, entry))
            .find(|entry| is_reconcile_queue_entry(entry))?;
        tracing::debug!(
            account = %account,
            state = %entry.state.as_str(),
            priority = entry.priority,
            reason = ?string_value(&entry.action, &["reason"]),
            "prioritizing auction reconcile before queued market work"
        );
        Some(entry.clone())
    }

    pub(super) fn missing_listing_inventory_retry_is_pending(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
    ) -> bool {
        let Some(pending) = self.pending_missing_listing_inventory_retries.get(account) else {
            return false;
        };
        if pending.entry != *entry {
            self.pending_missing_listing_inventory_retries
                .remove(account);
            return false;
        }
        if pending.retry_at <= Instant::now() {
            return false;
        }

        tracing::debug!(
            account = %account,
            state = %entry.state.as_str(),
            priority = entry.priority,
            attempts = pending.attempts,
            retry_after_ms = pending.retry_at.saturating_duration_since(Instant::now()).as_millis(),
            "missing listing inventory retry is waiting"
        );
        true
    }

    pub(super) fn remember_pending_market_step(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        instruction: &MarketInstruction,
    ) {
        self.pending_market_steps.insert(
            account.clone(),
            PendingMarketStep {
                entry: entry.clone(),
                instruction: instruction.clone(),
                last_attempt: Instant::now(),
                opens_sold_claim_action: false,
            },
        );
    }

    pub(super) async fn execute_live_market_instruction_or_retry(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        instruction: &MarketInstruction,
        reason: &str,
    ) -> bool {
        match self
            .session
            .execute_market_instruction(account, instruction)
            .await
        {
            Ok(_) => {
                tracing::info!(
                    account = %account,
                    state = %entry.state.as_str(),
                    priority = entry.priority,
                    instruction = ?instruction,
                    reason = %reason,
                    "executed queued market step"
                );
                true
            }
            Err(error) => {
                tracing::warn!(
                    account = %account,
                    reason = %reason,
                    error = %error,
                    "queued market step failed; leaving entry queued for retry"
                );
                self.remember_pending_market_step(account, entry, instruction);
                false
            }
        }
    }

    async fn handle_stale_claim_purchased_open_retry(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        instruction: &MarketInstruction,
        reason: &str,
        window: Option<&WindowSnapshot>,
    ) -> Result<bool> {
        if !is_claim_purchased_open_step(entry, instruction, reason) {
            self.pending_open_auction_retries.remove(account);
            return Ok(false);
        }

        let Some(pending_step) = self.pending_market_steps.get(account) else {
            self.pending_open_auction_retries.remove(account);
            return Ok(false);
        };
        if pending_step.entry != *entry || pending_step.instruction != *instruction {
            self.pending_open_auction_retries.remove(account);
            return Ok(false);
        }
        if pending_step.last_attempt.elapsed() < MARKET_STEP_RETRY_INTERVAL {
            return Ok(false);
        }

        let attempts = self
            .pending_open_auction_retries
            .get(account)
            .filter(|pending| pending.entry == *entry && pending.instruction == *instruction)
            .map(|pending| pending.attempts.saturating_add(1))
            .unwrap_or(1);
        let window_title = window.map(|window| window.title.as_str()).unwrap_or("none");
        let auction_id = match instruction {
            MarketInstruction::OpenAuction { auction_id } => auction_id.as_str(),
            _ => "",
        };

        if attempts < CLAIM_PURCHASED_OPEN_MAX_ATTEMPTS {
            self.pending_open_auction_retries.insert(
                account.clone(),
                PendingOpenAuctionRetry {
                    entry: entry.clone(),
                    instruction: instruction.clone(),
                    attempts,
                },
            );
            tracing::warn!(
                account = %account,
                state = %entry.state.as_str(),
                priority = entry.priority,
                auction_id,
                attempts,
                max_attempts = CLAIM_PURCHASED_OPEN_MAX_ATTEMPTS,
                window = %window_title,
                "purchased auction open did not produce a claim window; retrying"
            );
            return Ok(false);
        }

        self.pending_open_auction_retries.remove(account);
        self.pending_market_steps.remove(account);
        tracing::error!(
            account = %account,
            state = %entry.state.as_str(),
            priority = entry.priority,
            auction_id,
            attempts,
            max_attempts = CLAIM_PURCHASED_OPEN_MAX_ATTEMPTS,
            window = %window_title,
            action = ?entry.action,
            "purchased auction open stayed unavailable after retries; removing stale claim entry"
        );
        self.complete_queue_entry_or_defer(account, entry, PendingCompletionKind::CountOnly)
            .await?;
        notify_operator_best_effort(
            self.session.as_ref(),
            Notification {
                title: "Purchased claim unavailable".to_string(),
                body: format!(
                    "`{}` could not open a queued purchased auction after {attempts} attempts. Rust removed the stale claim entry so the queue can continue.",
                    account.as_str()
                ),
                account: Some(account.clone()),
            },
        )
        .await;
        Ok(true)
    }

    async fn reject_unsafe_listing_entry(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
    ) -> Result<bool> {
        let Some(reason) = unsafe_listing_entry_reason(entry) else {
            return Ok(false);
        };

        tracing::error!(
            account = %account,
            state = %entry.state.as_str(),
            priority = entry.priority,
            action = ?entry.action,
            reason = %reason,
            "blocked unsafe listing queue entry"
        );
        let had_active_window = self
            .active_windows
            .lock()
            .map_err(|_| anyhow::anyhow!("active window lock poisoned"))?
            .contains_key(account);
        if had_active_window {
            if let Err(error) = self
                .session
                .execute_market_instruction(account, &MarketInstruction::CloseWindow)
                .await
            {
                tracing::warn!(
                    account = %account,
                    error = %error,
                    "failed to close window after blocking unsafe listing"
                );
            }
            self.clear_active_window_cache(account)?;
        }
        self.pending_market_steps.remove(account);
        self.complete_queue_entry_or_defer(account, entry, PendingCompletionKind::CountOnly)
            .await?;
        notify_operator_best_effort(
            self.session.as_ref(),
            Notification {
                title: "Unsafe listing blocked".to_string(),
                body: format!(
                    "Blocked `{}` because {reason}. The queue entry was removed so it cannot list at the unsafe price.",
                    account.as_str()
                ),
                account: Some(account.clone()),
            },
        )
        .await;
        Ok(true)
    }

    fn listing_price_mismatch_retry_is_pending(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
    ) -> bool {
        let Some(pending) = self.pending_listing_price_mismatch_retries.get(account) else {
            return false;
        };
        if pending.entry != *entry {
            self.pending_listing_price_mismatch_retries.remove(account);
            return false;
        }
        if pending.retry_at <= Instant::now() {
            return false;
        }

        tracing::debug!(
            account = %account,
            state = %entry.state.as_str(),
            priority = entry.priority,
            attempts = pending.attempts,
            retry_after_ms = pending.retry_at.saturating_duration_since(Instant::now()).as_millis(),
            "listing confirmation price mismatch retry is waiting"
        );
        true
    }

    async fn handle_listing_price_mismatch_step(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        instruction: &MarketInstruction,
        reason: &str,
    ) -> Result<bool> {
        if !is_listing_price_mismatch_step(entry, instruction, reason) {
            return Ok(false);
        }

        let attempts = self
            .pending_listing_price_mismatch_retries
            .get(account)
            .filter(|pending| pending.entry == *entry)
            .map(|pending| pending.attempts.saturating_add(1))
            .unwrap_or(1);
        let window = self
            .active_windows
            .lock()
            .map_err(|_| anyhow::anyhow!("active window lock poisoned"))?
            .get(account)
            .cloned();
        let window_title = window
            .as_ref()
            .map(|window| window.title.as_str())
            .unwrap_or("none");
        let price_context = window
            .as_ref()
            .map(listing_confirmation_price_context)
            .unwrap_or_default();
        let expected_price = number_value(&entry.action, &["price", "oldPrice", "listPrice"]);
        if let Err(error) = self
            .session
            .execute_market_instruction(account, &MarketInstruction::CloseWindow)
            .await
        {
            tracing::warn!(
                account = %account,
                error = %error,
                "failed to close listing confirmation after price mismatch"
            );
        }
        self.clear_active_window_cache(account)?;

        if attempts < MISSING_LISTING_INVENTORY_MAX_ATTEMPTS {
            self.pending_listing_price_mismatch_retries.insert(
                account.clone(),
                PendingListingPriceMismatchRetry {
                    entry: entry.clone(),
                    retry_at: Instant::now() + MISSING_LISTING_INVENTORY_RETRY_DELAY,
                    attempts,
                },
            );
            self.remember_pending_market_step(
                account,
                entry,
                &MarketInstruction::Chat {
                    message: "/ah".to_string(),
                },
            );
            tracing::warn!(
                account = %account,
                state = %entry.state.as_str(),
                priority = entry.priority,
                attempts,
                max_attempts = MISSING_LISTING_INVENTORY_MAX_ATTEMPTS,
                retry_after_ms = MISSING_LISTING_INVENTORY_RETRY_DELAY.as_millis(),
                expected_price,
                window = %window_title,
                price_context = ?price_context,
                action = ?entry.action,
                "listing confirmation price did not match queued price; closing confirmation and retrying later"
            );
            return Ok(true);
        }

        self.pending_listing_price_mismatch_retries.remove(account);
        self.pending_market_steps.remove(account);
        tracing::error!(
            account = %account,
            state = %entry.state.as_str(),
            priority = entry.priority,
            attempts,
            max_attempts = MISSING_LISTING_INVENTORY_MAX_ATTEMPTS,
            expected_price,
            window = %window_title,
            price_context = ?price_context,
            action = ?entry.action,
            "listing confirmation price remained mismatched after retries; queueing auction reconcile and clearing listing entry"
        );
        self.queue_listing_status_reconcile(account, entry).await?;
        self.complete_queue_entry_or_defer(account, entry, PendingCompletionKind::CountOnly)
            .await?;
        notify_operator_best_effort(
            self.session.as_ref(),
            Notification {
                title: "Listing price mismatch".to_string(),
                body: format!(
                    "`{}` saw a listing confirmation price mismatch after {attempts} attempts. Rust queued auction reconciliation and removed the listing entry.",
                    account.as_str()
                ),
                account: Some(account.clone()),
            },
        )
        .await;
        Ok(true)
    }

    fn guard_bank_cooldown(
        &self,
        account: &AccountId,
        entry: &QueueEntry,
        instruction: &MarketInstruction,
    ) -> Result<bool> {
        if !is_bank_queue_entry(entry) || !is_open_bank_instruction(instruction) {
            return Ok(true);
        }

        match self.bank_cooldowns.try_mark_use(account)? {
            BankCooldownDecision::Ready => Ok(true),
            BankCooldownDecision::Wait { remaining_ms } => {
                tracing::warn!(
                    account = %account,
                    remaining_ms,
                    "bank command is still on cooldown; leaving entry queued"
                );
                Ok(false)
            }
        }
    }

    pub(super) fn should_attempt_market_step(
        &self,
        account: &AccountId,
        entry: &QueueEntry,
        instruction: &MarketInstruction,
    ) -> bool {
        self.pending_market_steps
            .get(account)
            .is_none_or(|pending| {
                pending.entry != *entry
                    || pending.instruction != *instruction
                    || pending.last_attempt.elapsed() >= MARKET_STEP_RETRY_INTERVAL
            })
    }

    fn live_market_window_is_settling(&self, account: &AccountId) -> Result<bool> {
        if !self.options.market_actions.allows_market_actions() {
            return Ok(false);
        }
        let observed_at = self
            .active_window_observed_at
            .lock()
            .map_err(|_| anyhow::anyhow!("active window timestamp lock poisoned"))?
            .get(account)
            .copied();
        Ok(observed_at
            .is_some_and(|observed_at| observed_at.elapsed() < MARKET_WINDOW_SETTLE_DELAY))
    }

    pub(super) fn clear_stale_transition_window_if_needed(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        instruction: &MarketInstruction,
        done: bool,
    ) -> Result<bool> {
        if done || !self.options.market_actions.allows_market_actions() {
            return Ok(false);
        }
        if !matches!(instruction, MarketInstruction::ClickSlot { .. }) {
            return Ok(false);
        }
        let Some(last_attempt) = self.pending_market_steps.get(account).and_then(|pending| {
            (pending.entry == *entry
                && pending.instruction == *instruction
                && pending.last_attempt.elapsed() >= MARKET_STEP_RETRY_INTERVAL)
                .then_some(pending.last_attempt)
        }) else {
            return Ok(false);
        };
        let observed_at = self
            .active_window_observed_at
            .lock()
            .map_err(|_| anyhow::anyhow!("active window timestamp lock poisoned"))?
            .get(account)
            .copied();
        if !observed_at.is_some_and(|observed_at| observed_at <= last_attempt) {
            return Ok(false);
        }

        tracing::warn!(
            account = %account,
            state = %entry.state.as_str(),
            priority = entry.priority,
            instruction = ?instruction,
            "clearing stale market window after transition click produced no fresh window"
        );
        self.clear_active_window_cache(account)?;
        self.pending_market_steps.remove(account);
        Ok(true)
    }
}

fn is_listing_price_mismatch_step(
    entry: &QueueEntry,
    instruction: &MarketInstruction,
    reason: &str,
) -> bool {
    matches!(entry.state, BotState::Listing | BotState::ListingNoName)
        && matches!(instruction, MarketInstruction::CloseWindow)
        && reason.eq_ignore_ascii_case("close listing confirmation with unexpected price")
}

fn is_claim_purchased_open_step(
    entry: &QueueEntry,
    instruction: &MarketInstruction,
    reason: &str,
) -> bool {
    matches!(&entry.state, BotState::Custom(state) if state == "claimPurchased")
        && matches!(instruction, MarketInstruction::OpenAuction { .. })
        && reason.eq_ignore_ascii_case("open purchased auction")
}

fn market_window_is_listing_confirmation(window: &WindowSnapshot) -> bool {
    let title = window.title.to_ascii_lowercase();
    if title.contains("confirm") && (title.contains("auction") || title.contains("bin")) {
        return true;
    }
    let text = super::support::window_text_plain(window);
    text.contains("confirm auction") || text.contains("confirm bin")
}

fn listing_confirmation_price_context(window: &WindowSnapshot) -> Vec<String> {
    let mut context = Vec::new();
    for slot in &window.slots {
        for line in
            std::iter::once(slot.display_name.as_str()).chain(slot.lore.iter().map(String::as_str))
        {
            let clean = strip_minecraft_color_codes(line).trim().to_string();
            if clean.is_empty() {
                continue;
            }
            let lower = clean.to_ascii_lowercase();
            if lower.contains("price")
                || lower.contains("bid")
                || lower.contains("coin")
                || lower.contains("auction")
                || lower.contains("create")
            {
                context.push(format!("slot {}: {}", slot.slot, clean));
                if context.len() >= 12 {
                    return context;
                }
            }
        }
    }
    context
}

fn pending_draft_listing_entry_matches(entry: &QueueEntry, draft: &PendingAuctionDraft) -> bool {
    if !matches!(entry.state, BotState::Listing | BotState::ListingNoName) {
        return false;
    }
    if string_value(
        &entry.action,
        &["inventory", "inv", "itemUuid", "itemUUID", "auctionID"],
    )
    .is_some_and(|uuid| uuid.eq_ignore_ascii_case(draft.item_uuid.as_str()))
    {
        return true;
    }

    let weak_identity_matches = if let Some(draft_tag) = draft.tag.as_deref()
        && string_value(&entry.action, &["tag"])
            .is_some_and(|tag| tag.eq_ignore_ascii_case(draft_tag))
    {
        true
    } else {
        string_value(&entry.action, &["itemName", "weirdItemName"]).is_some_and(|item_name| {
            normalized_pending_draft_text(&item_name)
                == normalized_pending_draft_text(&draft.item_name)
        })
    };
    if !weak_identity_matches {
        return false;
    }

    number_value(&entry.action, &["price", "oldPrice", "listPrice"])
        .is_some_and(|price| price.round().max(0.0) as u64 == draft.list_price)
}

fn normalized_pending_draft_text(text: &str) -> String {
    strip_minecraft_color_codes(text)
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
