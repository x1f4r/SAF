use super::auction_flow::{
    PendingAuctionDraft, is_bank_queue_entry, is_open_bank_instruction, is_reconcile_queue_entry,
    pending_create_auction_draft, should_recover_pending_draft,
};
use super::bank::BankCooldownDecision;
use super::listing_safety::unsafe_listing_entry_reason;
use super::notifier::notify_operator_best_effort;
use super::support::{number_value, string_value, strip_minecraft_color_codes};
use super::{
    AUCTION_MANAGEMENT_UNAVAILABLE_BACKOFF, CLAIM_PURCHASED_OPEN_MAX_ATTEMPTS, LiveRuntime,
    MARKET_STEP_NO_PROGRESS_MAX_STRIKES,
    MARKET_STEP_RETRY_INTERVAL, MARKET_WINDOW_SETTLE_DELAY, MISSING_LISTING_INVENTORY_MAX_ATTEMPTS,
    MISSING_LISTING_INVENTORY_RETRY_DELAY, PendingCompletionKind, PendingListingPriceMismatchRetry,
    PendingMarketStep, PendingOpenAuctionRetry, PendingUnaffordableListingRetry,
    StaleTransitionStrikes, UNAFFORDABLE_LISTING_RETRY_DELAY,
};
use anyhow::Result;
use saf_core::gui::WindowSnapshot;
use saf_core::ports::{Notification, NotificationKind, QueueStore};
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
                    // Skip any listing that is parked under a retry-hold (low purse,
                    // item-not-yet-visible, price mismatch). Otherwise a single
                    // stuck/stale listing at the head of the queue starves all the
                    // sibling work behind it — claimPurchased, claimSold, expired,
                    // bank — for the whole hold window.
                    .filter(|entry| !self.listing_entry_is_retry_held(&account, entry))
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
            if self.unaffordable_listing_retry_is_pending(&account, &entry) {
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
                    .defer_unaffordable_listing_once(&account, &entry, window)
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
            if let Some(window) = &window {
                maybe_dump_market_window(&account, &entry, window);
            }
            let Some(step) = self
                .session
                .plan_market_queue_entry(&entry, window.as_ref())
            else {
                self.pending_market_steps.remove(&account);
                self.pending_open_auction_retries.remove(&account);
                continue;
            };
            // If a reconcile found the AH menu has no "Manage Auctions" button
            // (0 auctions to manage), note it so slot-pressure reconciles back off
            // and stop churning the GUI under the listing flow.
            if is_reconcile_queue_entry(&entry)
                && step.reason == "auction management unavailable"
            {
                self.auction_management_unavailable_until.insert(
                    account.clone(),
                    Instant::now() + AUCTION_MANAGEMENT_UNAVAILABLE_BACKOFF,
                );
            }
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
                if self
                    .clear_stale_transition_window_if_needed(
                        &account,
                        &entry,
                        &step.instruction,
                        step.done,
                    )
                    .await?
                {
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
                // This entry reached a terminal success, so it is healthy: drop
                // any strikes counted against *it* (not against other entries).
                self.clear_stale_transition_strikes_for(&account, &entry);
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
            // If the drafted item can't be listed right now (its fee exceeds the
            // purse, so it's on an affordability hold), don't keep re-selecting it
            // — that would let the stuck draft monopolize the create window. Yield
            // so a different, affordable listing can take the window instead (its
            // item-select click swaps the held item back out of the slot).
            if self.listing_held_for_affordability(account, entry) {
                tracing::debug!(
                    account = %account,
                    draft_item = %draft.item_name,
                    draft_price = draft.list_price,
                    "yielding the create window: the drafted item is on an affordability hold"
                );
                return Ok(None);
            }
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
            // A different entry is being worked this poll — that's fine, but do
            // NOT drop the hold. Removing it here let a sibling entry (a claim, a
            // reconcile) silently reset a stuck listing's retry counter every
            // cycle, so it never reached its attempt cap and never got cleaned up
            // — leaving a stale listing stuck forever and starving the queue.
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

    /// True while a listing is being held off because the account could not
    /// afford its auction creation fee. Lets the hold expire (so coins freed by
    /// other sales let it proceed) and drops the hold if the queue moved on.
    /// Read-only check (for entry selection) of whether `entry` is a listing
    /// currently parked under an active affordability hold. Mirrors
    /// `unaffordable_listing_retry_is_pending` but never mutates, so it can run
    /// inside the selection filter to skip the held listing and let sibling
    /// entries proceed instead of starving them.
    /// Read-only check (for entry selection) of whether a listing entry is
    /// currently parked under *any* retry-hold, so it can be skipped in favor of
    /// sibling work instead of starving it. Covers the affordability hold (per
    /// item) and the missing-inventory / price-mismatch retries (per account,
    /// entry-matched).
    fn listing_entry_is_retry_held(&self, account: &AccountId, entry: &QueueEntry) -> bool {
        if self.listing_held_for_affordability(account, entry) {
            return true;
        }
        let now = Instant::now();
        if self
            .pending_missing_listing_inventory_retries
            .get(account)
            .is_some_and(|pending| pending.entry == *entry && pending.retry_at > now)
        {
            return true;
        }
        if self
            .pending_listing_price_mismatch_retries
            .get(account)
            .is_some_and(|pending| pending.entry == *entry && pending.retry_at > now)
        {
            return true;
        }
        false
    }

    fn listing_held_for_affordability(&self, account: &AccountId, entry: &QueueEntry) -> bool {
        if !matches!(entry.state, BotState::Listing | BotState::ListingNoName) {
            return false;
        }
        let key = listing_affordability_key(entry);
        self.pending_unaffordable_listing_retries
            .get(account)
            .and_then(|by_item| by_item.get(&key))
            .is_some_and(|pending| pending.retry_at > Instant::now())
    }

    fn unaffordable_listing_retry_is_pending(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
    ) -> bool {
        // Hold this specific listing until its cooldown passes; once it expires,
        // let one re-check through (the gate re-evaluates affordability and either
        // proceeds or re-arms the hold without re-alerting). Other items are not
        // affected — affordability is judged per item.
        self.listing_held_for_affordability(account, entry)
    }

    /// Refuses to attempt an auction listing the account cannot pay the creation
    /// fee for. Clicking "Create Auction" with too few coins silently fails and
    /// leaves the window open, which previously drove an endless submit-retry
    /// loop (server spam / ban risk). Instead we close the window, hold the
    /// listing for [`UNAFFORDABLE_LISTING_RETRY_DELAY`], and alert the operator
    /// once so they can add coins or sell something. Returns `true` if it took
    /// over this poll for the account.
    pub(super) async fn defer_unaffordable_listing_once(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        window: &WindowSnapshot,
    ) -> Result<bool> {
        if !self.options.market_actions.allows_market_actions()
            || !matches!(entry.state, BotState::Listing | BotState::ListingNoName)
            || !is_create_auction_window(window)
        {
            return Ok(false);
        }
        // Only judge affordability once the listing is in BIN mode, so the fee we
        // read is the BIN fee we'd actually pay. In regular-auction mode the
        // planner still has to flip the "Switch to BIN" toggle first.
        if !window.title.to_ascii_lowercase().contains("bin") {
            return Ok(false);
        }
        // Only judge the item actually sitting in the create slot. If the window
        // still holds a *different* item's draft (e.g. a stuck unaffordable item
        // jamming the slot), don't gate this entry by that item's fee — let the
        // listing flow swap the correct item into the slot first.
        if let Some(draft) = pending_create_auction_draft(window) {
            let entry_uuid =
                string_value(&entry.action, &["inventory", "inv", "itemUuid", "itemUUID"]);
            if entry_uuid
                .is_some_and(|uuid| !uuid.eq_ignore_ascii_case(draft.item_uuid.as_str()))
            {
                return Ok(false);
            }
        }
        let Some(fee) = parse_auction_creation_fee(window) else {
            return Ok(false);
        };
        let Some(purse) = self.stats.current_purse(account) else {
            return Ok(false);
        };
        let key = listing_affordability_key(entry);
        if purse >= fee {
            // Affordable again — clear this item's hold and proceed normally.
            if let Some(by_item) = self.pending_unaffordable_listing_retries.get_mut(account) {
                by_item.remove(&key);
            }
            return Ok(false);
        }

        let now = Instant::now();
        let by_item = self
            .pending_unaffordable_listing_retries
            .entry(account.clone())
            .or_default();
        // Drop holds whose cooldown lapsed long ago (the item almost certainly
        // listed or sold) so the per-account map can't grow without bound.
        by_item.retain(|_, pending| pending.retry_at + UNAFFORDABLE_LISTING_RETRY_DELAY > now);
        let already_notified = by_item.get(&key).is_some_and(|pending| pending.notified);
        by_item.insert(
            key,
            PendingUnaffordableListingRetry {
                retry_at: now + UNAFFORDABLE_LISTING_RETRY_DELAY,
                notified: true,
            },
        );
        tracing::warn!(
            account = %account,
            state = %entry.state.as_str(),
            priority = entry.priority,
            purse,
            creation_fee = fee,
            retry_after_ms = UNAFFORDABLE_LISTING_RETRY_DELAY.as_millis(),
            "skipping listing: account cannot afford the auction creation fee"
        );
        if let Err(error) = self
            .session
            .execute_market_instruction(account, &MarketInstruction::CloseWindow)
            .await
        {
            tracing::warn!(
                account = %account,
                error = %error,
                "failed to close create-auction window after unaffordable listing fee"
            );
        }
        self.clear_active_window_cache(account)?;
        self.pending_market_steps.remove(account);
        if !already_notified {
            let item = listing_entry_item_label(entry);
            notify_operator_best_effort(
                self.session.as_ref(),
                Notification::new(
                    NotificationKind::Blocked,
                    "Listing blocked: low purse",
                    format!(
                        "`{}` can't list {} — the auction creation fee is {} coins but the purse is only {} coins. Add coins or let other auctions sell, then it will list automatically.",
                        account.as_str(),
                        item,
                        format_coins(fee),
                        format_coins(purse),
                    ),
                    Some(account.clone()),
                )
                .with_thumbnail(crate::player_head::account_head_thumbnail_url(
                    account.as_str(),
                )),
            )
            .await;
        }
        Ok(true)
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
            Notification::new(
                NotificationKind::Error,
                "Purchased claim unavailable",
                format!(
                    "`{}` could not open a queued purchased auction after {attempts} attempts. Rust removed the stale claim entry so the queue can continue.",
                    account.as_str()
                ),
                Some(account.clone()),
            )
            .with_thumbnail(crate::player_head::account_head_thumbnail_url(
                account.as_str(),
            )),
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
            Notification::new(
                NotificationKind::Blocked,
                "Unsafe listing blocked",
                format!(
                    "Blocked `{}` because {reason}. The queue entry was removed so it cannot list at the unsafe price.",
                    account.as_str()
                ),
                Some(account.clone()),
            )
            .with_fields(vec![("Reason".to_string(), reason.clone())])
            .with_thumbnail(crate::player_head::account_head_thumbnail_url(
                account.as_str(),
            )),
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
            Notification::new(
                NotificationKind::Blocked,
                "Listing price mismatch",
                format!(
                    "`{}` saw a listing confirmation price mismatch after {attempts} attempts. Rust queued auction reconciliation and removed the listing entry.",
                    account.as_str()
                ),
                Some(account.clone()),
            )
            .with_thumbnail(crate::player_head::account_head_thumbnail_url(
                account.as_str(),
            )),
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
        let Some(observed_at) = observed_at else {
            return Ok(false);
        };
        // Use the jittered settle sampled when this window was observed; fall
        // back to the fixed delay if the sample is missing or stale (its key no
        // longer matches the current observation).
        let settle = self
            .market_settle_jitter
            .lock()
            .map_err(|_| anyhow::anyhow!("market settle jitter lock poisoned"))?
            .get(account)
            .filter(|(sampled_for, _)| *sampled_for == observed_at)
            .map(|(_, delay)| *delay)
            .unwrap_or(MARKET_WINDOW_SETTLE_DELAY);
        Ok(observed_at.elapsed() < settle)
    }

    pub(super) async fn clear_stale_transition_window_if_needed(
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

        let strikes = self.register_stale_transition_strike(account, entry);
        tracing::warn!(
            account = %account,
            state = %entry.state.as_str(),
            priority = entry.priority,
            instruction = ?instruction,
            strikes,
            max_strikes = MARKET_STEP_NO_PROGRESS_MAX_STRIKES,
            "clearing stale market window after transition click produced no fresh window"
        );
        self.clear_active_window_cache(account)?;
        self.pending_market_steps.remove(account);
        if strikes >= MARKET_STEP_NO_PROGRESS_MAX_STRIKES {
            self.abort_stuck_market_step(account, entry).await?;
        }
        Ok(true)
    }

    /// Records one no-progress strike for the step currently being driven for
    /// `account`. Strikes accumulate while the *same* queue entry keeps failing
    /// to advance; a different entry resets the counter. Successful completion of
    /// any entry clears it via [`clear_stale_transition_strikes`]. Returns the
    /// running strike count.
    fn register_stale_transition_strike(&mut self, account: &AccountId, entry: &QueueEntry) -> u8 {
        let next = match self.stale_transition_strikes.get(account) {
            Some(existing) if existing.entry == *entry => existing.count.saturating_add(1),
            _ => 1,
        };
        self.stale_transition_strikes.insert(
            account.clone(),
            StaleTransitionStrikes {
                entry: entry.clone(),
                count: next,
            },
        );
        next
    }

    /// Clears the no-progress strike counter for `account`, but only when the
    /// entry that just completed is the *same* one the strikes were counted
    /// against. This matters because unrelated bookkeeping entries (e.g. the
    /// `reconcileAuctions` open/close that runs every cycle) complete constantly
    /// while a different entry is stuck — wiping strikes on any completion would
    /// keep resetting the stuck entry to 1 and the kill-switch would never trip.
    pub(super) fn clear_stale_transition_strikes_for(
        &mut self,
        account: &AccountId,
        completed: &QueueEntry,
    ) {
        if self
            .stale_transition_strikes
            .get(account)
            .is_some_and(|strikes| strikes.entry == *completed)
        {
            self.stale_transition_strikes.remove(account);
        }
    }

    /// Kill-switch: abandons a market step that has produced
    /// [`MARKET_STEP_NO_PROGRESS_MAX_STRIKES`] consecutive no-progress clicks.
    /// For listings this queues a status reconcile (so a half-created auction is
    /// re-detected) before dropping the entry; everything else is simply dropped.
    /// Either way the operator is alerted and the spammy retry loop stops.
    async fn abort_stuck_market_step(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
    ) -> Result<()> {
        self.stale_transition_strikes.remove(account);
        let is_listing = matches!(entry.state, BotState::Listing | BotState::ListingNoName);
        tracing::error!(
            account = %account,
            state = %entry.state.as_str(),
            priority = entry.priority,
            action = ?entry.action,
            max_strikes = MARKET_STEP_NO_PROGRESS_MAX_STRIKES,
            "market step made no progress after repeated retries; aborting it to stop server spam"
        );
        if is_listing {
            self.queue_listing_status_reconcile(account, entry).await?;
        }
        if let Err(error) = self
            .session
            .execute_market_instruction(account, &MarketInstruction::CloseWindow)
            .await
        {
            tracing::warn!(
                account = %account,
                error = %error,
                "failed to close window while aborting a stuck market step"
            );
        }
        self.clear_active_window_cache(account)?;
        self.pending_market_steps.remove(account);
        self.complete_queue_entry_or_defer(account, entry, PendingCompletionKind::CountOnly)
            .await?;
        let detail = if is_listing {
            format!(
                "`{}` could not finish creating an auction after {} attempts (the create-auction window never advanced — the item may be unauctionable or the menu changed). Rust queued a status reconcile and dropped the listing so it stops retrying.",
                account.as_str(),
                MARKET_STEP_NO_PROGRESS_MAX_STRIKES
            )
        } else {
            format!(
                "`{}` made no progress on a `{}` market step after {} attempts and dropped it to stop spamming the server.",
                account.as_str(),
                entry.state.as_str(),
                MARKET_STEP_NO_PROGRESS_MAX_STRIKES
            )
        };
        notify_operator_best_effort(
            self.session.as_ref(),
            Notification::new(
                NotificationKind::Blocked,
                "Market step aborted",
                detail,
                Some(account.clone()),
            )
            .with_thumbnail(crate::player_head::account_head_thumbnail_url(
                account.as_str(),
            )),
        )
        .await;
        Ok(())
    }
}

fn is_create_auction_window(window: &WindowSnapshot) -> bool {
    let title = window.title.to_ascii_lowercase();
    title.contains("create") && title.contains("auction")
}

/// Extracts the auction creation fee (in coins) the create-auction window shows
/// before you submit. Prefers the explicit "Creation fee:" line (the total the
/// player pays) and falls back to the auction-house "Extra fee:" cut. Returns
/// `None` when no fee line is present (e.g. the listing isn't ready to submit).
fn parse_auction_creation_fee(window: &WindowSnapshot) -> Option<f64> {
    let lines = || {
        window.slots.iter().flat_map(|slot| {
            std::iter::once(slot.display_name.as_str()).chain(slot.lore.iter().map(String::as_str))
        })
    };
    lines()
        .filter(|line| line.to_ascii_lowercase().contains("creation fee"))
        .find_map(parse_trailing_coin_amount)
        .or_else(|| {
            lines()
                .filter(|line| {
                    let lower = line.to_ascii_lowercase();
                    lower.contains("extra fee") && lower.contains("coin")
                })
                .find_map(parse_trailing_coin_amount)
        })
}

/// Parses the coin amount that immediately precedes the word "coins" in a lore
/// line such as `Creation fee: 4,711,200 coins`, ignoring thousands separators.
fn parse_trailing_coin_amount(line: &str) -> Option<f64> {
    let lower = line.to_ascii_lowercase();
    let head = lower.split("coin").next()?;
    let digits: String = head
        .chars()
        .rev()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit() || *c == ',' || *c == '+')
        .filter(char::is_ascii_digit)
        .collect();
    if digits.is_empty() {
        return None;
    }
    digits.chars().rev().collect::<String>().parse::<f64>().ok()
}

fn format_coins(value: f64) -> String {
    saf_core::numbers::add_commas_to_number(value.round())
}

/// A stable per-listing key for affordability holds, so each item is judged
/// independently. Prefers the inventory UUID (stable across reconcile cycles),
/// then the auction id, then the item name.
fn listing_affordability_key(entry: &QueueEntry) -> String {
    string_value(&entry.action, &["inventory", "inv", "itemUuid", "itemUUID"])
        .or_else(|| string_value(&entry.action, &["auctionID", "auctionId", "auction_id"]))
        .or_else(|| string_value(&entry.action, &["itemName", "weirdItemName"]))
        .unwrap_or_else(|| "listing".to_string())
}

/// A short, human-readable label for the item a listing entry is for, used in
/// operator alerts. Prefers the item name, then the SkyBlock tag, then the UUID.
fn listing_entry_item_label(entry: &QueueEntry) -> String {
    string_value(&entry.action, &["itemName", "weirdItemName", "item_name"])
        .map(|name| strip_minecraft_color_codes(&name))
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .or_else(|| string_value(&entry.action, &["tag"]))
        .or_else(|| string_value(&entry.action, &["inventory", "inv", "itemUuid", "itemUUID"]))
        .unwrap_or_else(|| "the queued item".to_string())
}

/// Diagnostic-only: when `SAF_DEBUG_DUMP_WINDOWS` is set, log the full contents
/// of an auction/listing GUI window (title + every slot's index, item name,
/// display name, and lore). This is how we capture the real Create-Auction
/// layout from the live server to fix the price/duration slot mapping. It is a
/// no-op unless the env var is present, so it is safe to leave compiled in.
fn maybe_dump_market_window(account: &AccountId, entry: &QueueEntry, window: &WindowSnapshot) {
    if std::env::var_os("SAF_DEBUG_DUMP_WINDOWS").is_none() {
        return;
    }
    let title = window.title.to_ascii_lowercase();
    let listing_related = matches!(entry.state, BotState::Listing | BotState::ListingNoName)
        || title.contains("auction")
        || title.contains("create")
        || title.contains("duration")
        || title.contains("price");
    if !listing_related {
        return;
    }
    tracing::info!(
        account = %account,
        state = %entry.state.as_str(),
        window_title = %window.title,
        slot_count = window.slots.len(),
        "DEBUG window dump: header"
    );
    for slot in &window.slots {
        tracing::info!(
            account = %account,
            slot = slot.slot,
            item = %slot.name,
            display_name = %strip_minecraft_color_codes(&slot.display_name),
            has_uuid = slot.item_uuid.is_some(),
            lore = %slot
                .lore
                .iter()
                .map(|line| strip_minecraft_color_codes(line))
                .collect::<Vec<_>>()
                .join(" | "),
            "DEBUG window dump: slot"
        );
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
