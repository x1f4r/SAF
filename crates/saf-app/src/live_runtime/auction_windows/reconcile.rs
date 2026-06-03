use super::super::auction_flow::{
    PendingAuctionDraft, confirm_auction_slot, create_auction_slot, expired_claim_from_window,
    is_confirm_auction_window, is_create_auction_window, is_expired_queue_entry,
    is_reconcile_queue_entry, pending_create_auction_draft, reconcile_auction_window,
    should_recover_pending_draft, sold_claim_action_slot, submit_auction_slot,
};
use super::super::stats::{auction_slot_stats_from_window, auction_slot_stats_full};
use super::super::support::{number_value, string_value, window_text_plain};
use super::super::windows::is_manage_auctions_window;
use super::super::{
    LiveRuntime, MARKET_STEP_RETRY_INTERVAL, PendingCompletionKind, PendingMarketStep,
};
use anyhow::Result;
use saf_core::gui::WindowSnapshot;
use saf_core::ports::QueueStore;
use saf_core::relist::listing_hours_for_price;
use saf_core::{AccountId, BotState, MarketInstruction, QueueEntry};
use std::time::{Duration, Instant};
use tokio::time::sleep;

impl LiveRuntime {
    pub(in crate::live_runtime) async fn process_reconcile_window_once(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        window: &WindowSnapshot,
    ) -> Result<bool> {
        if !is_reconcile_queue_entry(entry) {
            return Ok(false);
        }

        if is_create_auction_window(window) {
            return self
                .process_pending_draft_create_window(account, entry, window)
                .await;
        }

        if is_confirm_auction_window(window) {
            return self
                .process_pending_draft_confirm_window(account, entry, window)
                .await;
        }

        if let Some(slot) = sold_claim_action_slot(window)
            && self.pending_sold_claim_action_is_expected(account, entry)
        {
            return self
                .process_sold_claim_action_window(account, entry, slot)
                .await;
        }

        if self.pending_sold_claim_action_is_expected(account, entry) {
            return self
                .wait_for_sold_claim_action_window(account, entry, window)
                .await;
        }

        if is_auction_view_window(window) {
            return self
                .close_unrelated_reconcile_window(account, entry, window)
                .await;
        }

        if !is_manage_auctions_window(window) {
            return Ok(false);
        }
        tracing::debug!(
            account = %account,
            state = %entry.state.as_str(),
            priority = entry.priority,
            window = %window.title,
            "processing auction reconciliation window"
        );
        if let Err(error) = self.stats.record_window_snapshot(account, window) {
            tracing::warn!(
                account = %account,
                error = %error,
                "failed to record reconcile window stats; continuing with window snapshot"
            );
        }

        let Some(reconcile) =
            reconcile_auction_window(account, window, self.config.angry_coop_prevention)
        else {
            let auction_slots_full = match self.stats.auction_slots_full(account) {
                Ok(full) => full,
                Err(error) => {
                    tracing::warn!(
                        account = %account,
                        error = %error,
                        "failed to inspect cached auction slot stats; falling back to window snapshot"
                    );
                    auction_slot_stats_from_window(window).is_some_and(auction_slot_stats_full)
                }
            };
            if should_recover_pending_draft(entry)
                && !auction_slots_full
                && let Some(slot) = create_auction_slot(window)
            {
                let instruction = MarketInstruction::ClickSlot { slot };
                if !self.should_attempt_market_step(account, entry, &instruction) {
                    return Ok(true);
                }
                if self.options.market_actions.allows_market_actions() {
                    if !self
                        .execute_live_market_instruction_or_retry(
                            account,
                            entry,
                            &instruction,
                            "open pending auction draft",
                        )
                        .await
                    {
                        return Ok(true);
                    }
                } else {
                    self.queue.record_dry_run_step(
                        account,
                        entry,
                        &instruction,
                        "open pending auction draft",
                    )?;
                }
                self.pending_market_steps.insert(
                    account.clone(),
                    PendingMarketStep {
                        entry: entry.clone(),
                        instruction,
                        last_attempt: Instant::now(),
                        opens_sold_claim_action: false,
                    },
                );
                self.processed_queue_steps += 1;
                return Ok(true);
            }
            if self.options.market_actions.allows_market_actions()
                && self.queue.has_bid_data(account).await?
            {
                let instruction = MarketInstruction::CloseWindow;
                if !self
                    .execute_live_market_instruction_or_retry(
                        account,
                        entry,
                        &instruction,
                        "close reconcile window before purchased bids follow-up",
                    )
                    .await
                {
                    return Ok(true);
                }
                self.complete_reconcile_entry_and_queue_bids(account, entry)
                    .await?;
                self.processed_queue_steps += 1;
                return Ok(true);
            }
            tracing::info!(
                account = %account,
                state = %entry.state.as_str(),
                priority = entry.priority,
                "auction reconciliation found no claimable or recoverable auction slots"
            );
            return Ok(false);
        };
        if reconcile.sold_count == 0
            && !reconcile.claimed_expired.is_empty()
            && self
                .all_expired_relist_entries_already_queued(account, &reconcile.claimed_expired)
                .await?
        {
            tracing::info!(
                account = %account,
                state = %entry.state.as_str(),
                priority = entry.priority,
                expired_count = reconcile.expired_count,
                "skipping expired auction claim because matching relist work is already queued"
            );
            self.complete_reconcile_entry_and_queue_bids(account, entry)
                .await?;
            self.processed_queue_steps += 1;
            return Ok(true);
        }
        let instruction = MarketInstruction::ClickSlot {
            slot: reconcile.claim_slot,
        };
        if !self.should_attempt_market_step(account, entry, &instruction) {
            return Ok(true);
        }

        let reason = reconcile.reason();
        let live_market_actions = self.options.market_actions.allows_market_actions();
        if live_market_actions {
            if !self
                .execute_live_market_instruction_or_retry(account, entry, &instruction, &reason)
                .await
            {
                return Ok(true);
            }
            if reconcile.sold_count > 0 && !reconcile.claim_all {
                self.pending_market_steps.insert(
                    account.clone(),
                    PendingMarketStep {
                        entry: entry.clone(),
                        instruction,
                        last_attempt: Instant::now(),
                        opens_sold_claim_action: true,
                    },
                );
                self.processed_queue_steps += 1;
                return Ok(true);
            }
            sleep(Duration::from_millis(1500)).await;
            let _ = self
                .session
                .execute_market_instruction(account, &MarketInstruction::CloseWindow)
                .await;
            self.queue_expired_relist_entries(account, &reconcile.claimed_expired)?;
            self.pending_market_steps.remove(account);
            self.clear_active_window_cache(account)?;
            self.complete_queue_entry_or_defer(account, entry, PendingCompletionKind::Reconcile)
                .await?;
        } else {
            self.queue
                .record_dry_run_step(account, entry, &instruction, &reason)?;
            self.pending_market_steps.insert(
                account.clone(),
                PendingMarketStep {
                    entry: entry.clone(),
                    instruction,
                    last_attempt: Instant::now(),
                    opens_sold_claim_action: false,
                },
            );
        }

        self.processed_queue_steps += 1;
        Ok(true)
    }

    async fn process_sold_claim_action_window(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        slot: usize,
    ) -> Result<bool> {
        let instruction = MarketInstruction::ClickSlot { slot };
        if !self.should_attempt_market_step(account, entry, &instruction) {
            return Ok(true);
        }
        let live_market_actions = self.options.market_actions.allows_market_actions();
        if live_market_actions {
            if !self
                .execute_live_market_instruction_or_retry(
                    account,
                    entry,
                    &instruction,
                    "claim sold auction action",
                )
                .await
            {
                return Ok(true);
            }
            sleep(Duration::from_millis(1500)).await;
            let _ = self
                .session
                .execute_market_instruction(account, &MarketInstruction::CloseWindow)
                .await;
            self.pending_market_steps.remove(account);
            self.clear_active_window_cache(account)?;
            self.complete_queue_entry_or_defer(account, entry, PendingCompletionKind::Reconcile)
                .await?;
        } else {
            self.queue.record_dry_run_step(
                account,
                entry,
                &instruction,
                "claim sold auction action",
            )?;
            self.pending_market_steps.insert(
                account.clone(),
                PendingMarketStep {
                    entry: entry.clone(),
                    instruction,
                    last_attempt: Instant::now(),
                    opens_sold_claim_action: false,
                },
            );
        }
        self.processed_queue_steps += 1;
        Ok(true)
    }

    async fn wait_for_sold_claim_action_window(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        window: &WindowSnapshot,
    ) -> Result<bool> {
        let Some(last_attempt) = self.pending_market_steps.get(account).and_then(|pending| {
            self.pending_sold_claim_entries_match(&pending.entry, entry)
                .then_some(pending.last_attempt)
        }) else {
            return Ok(false);
        };
        if last_attempt.elapsed() < MARKET_STEP_RETRY_INTERVAL {
            return Ok(true);
        }

        tracing::warn!(
            account = %account,
            state = %entry.state.as_str(),
            priority = entry.priority,
            window = %window.title,
            "sold auction claim did not reach a claim action window; closing and retrying reconciliation"
        );
        if self.options.market_actions.allows_market_actions() {
            if let Err(error) = self
                .session
                .execute_market_instruction(account, &MarketInstruction::CloseWindow)
                .await
            {
                tracing::warn!(
                    account = %account,
                    error = %error,
                    "failed to close stale sold auction claim window"
                );
            }
        }
        self.pending_market_steps.remove(account);
        self.clear_active_window_cache(account)?;
        self.processed_queue_steps += 1;
        Ok(true)
    }

    async fn close_unrelated_reconcile_window(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        window: &WindowSnapshot,
    ) -> Result<bool> {
        let instruction = MarketInstruction::CloseWindow;
        if !self.should_attempt_market_step(account, entry, &instruction) {
            return Ok(true);
        }
        let reason = format!("close unrelated {} before reconcile", window.title);
        if self.options.market_actions.allows_market_actions() {
            if !self
                .execute_live_market_instruction_or_retry(account, entry, &instruction, &reason)
                .await
            {
                return Ok(true);
            }
            self.pending_market_steps.remove(account);
            self.clear_active_window_cache(account)?;
        } else {
            self.queue
                .record_dry_run_step(account, entry, &instruction, &reason)?;
        }
        self.processed_queue_steps += 1;
        Ok(true)
    }

    fn pending_sold_claim_action_is_expected(
        &self,
        account: &AccountId,
        entry: &QueueEntry,
    ) -> bool {
        self.pending_market_steps
            .get(account)
            .is_some_and(|pending| {
                self.pending_sold_claim_entries_match(&pending.entry, entry)
                    && pending.opens_sold_claim_action
                    && matches!(pending.instruction, MarketInstruction::ClickSlot { .. })
            })
    }

    fn pending_sold_claim_entries_match(&self, pending: &QueueEntry, current: &QueueEntry) -> bool {
        pending == current
            || (is_reconcile_queue_entry(pending) && is_reconcile_queue_entry(current))
    }

    fn pending_draft_submit_is_expected(&self, account: &AccountId, entry: &QueueEntry) -> bool {
        self.pending_market_steps
            .get(account)
            .is_some_and(|pending| {
                pending.entry == *entry
                    && matches!(
                        pending.instruction,
                        MarketInstruction::ClickSlot { slot: 29 }
                    )
            })
    }

    async fn process_pending_draft_create_window(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        window: &WindowSnapshot,
    ) -> Result<bool> {
        let Some(draft) = pending_create_auction_draft(window) else {
            return self
                .finish_reconcile_window(
                    account,
                    entry,
                    MarketInstruction::CloseWindow,
                    "no pending auction draft",
                )
                .await;
        };
        let Some(expected) = expected_pending_draft_listing(entry) else {
            if self
                .recover_untracked_pending_draft(account, entry, &draft)
                .await?
            {
                return Ok(true);
            }
            tracing::warn!(
                account = %account,
                state = %entry.state.as_str(),
                priority = entry.priority,
                draft_item = %draft.item_name,
                draft_price = draft.list_price,
                action = ?entry.action,
                "closing pending auction draft because reconcile entry has no expected listing price"
            );
            return self
                .finish_reconcile_window(
                    account,
                    entry,
                    MarketInstruction::CloseWindow,
                    "close pending auction draft without expected listing price",
                )
                .await;
        };
        if expected
            .inventory_uuid
            .as_deref()
            .is_some_and(|uuid| !uuid.eq_ignore_ascii_case(draft.item_uuid.as_str()))
        {
            tracing::warn!(
                account = %account,
                state = %entry.state.as_str(),
                priority = entry.priority,
                expected_inventory = ?expected.inventory_uuid,
                draft_inventory = %draft.item_uuid,
                draft_item = %draft.item_name,
                draft_price = draft.list_price,
                "closing pending auction draft because selected item does not match reconcile entry"
            );
            return self
                .finish_reconcile_window(
                    account,
                    entry,
                    MarketInstruction::CloseWindow,
                    "close pending auction draft with unexpected item",
                )
                .await;
        }
        if expected.list_price != draft.list_price {
            tracing::warn!(
                account = %account,
                state = %entry.state.as_str(),
                priority = entry.priority,
                expected_price = expected.list_price,
                draft_price = draft.list_price,
                draft_item = %draft.item_name,
                "closing pending auction draft because draft price does not match reconcile entry"
            );
            return self
                .finish_reconcile_window(
                    account,
                    entry,
                    MarketInstruction::CloseWindow,
                    "close pending auction draft with unexpected price",
                )
                .await;
        }
        let instruction = MarketInstruction::ClickSlot {
            slot: submit_auction_slot(window),
        };
        if !self.should_attempt_market_step(account, entry, &instruction) {
            return Ok(true);
        }
        if self
            .clear_stale_transition_window_if_needed(account, entry, &instruction, false)
            .await?
        {
            tracing::warn!(
                account = %account,
                state = %entry.state.as_str(),
                priority = entry.priority,
                "pending auction draft did not advance after submit; closing draft recovery and completing reconcile entry"
            );
            if let Err(error) = self
                .session
                .execute_market_instruction(account, &MarketInstruction::CloseWindow)
                .await
            {
                tracing::warn!(
                    account = %account,
                    error = %error,
                    "failed to close stale pending auction draft"
                );
            }
            self.clear_active_window_cache(account)?;
            self.complete_reconcile_entry_and_queue_bids(account, entry)
                .await?;
            self.processed_queue_steps += 1;
            return Ok(true);
        }

        if self.options.market_actions.allows_market_actions() {
            let reason = format!(
                "submit pending auction draft for {} at {}",
                draft.item_name, draft.list_price
            );
            if !self
                .execute_live_market_instruction_or_retry(account, entry, &instruction, &reason)
                .await
            {
                return Ok(true);
            }
        } else {
            self.queue.record_dry_run_step(
                account,
                entry,
                &instruction,
                "submit pending auction draft",
            )?;
        }
        self.pending_market_steps.insert(
            account.clone(),
            PendingMarketStep {
                entry: entry.clone(),
                instruction,
                last_attempt: Instant::now(),
                opens_sold_claim_action: false,
            },
        );
        self.processed_queue_steps += 1;
        Ok(true)
    }

    async fn recover_untracked_pending_draft(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        draft: &PendingAuctionDraft,
    ) -> Result<bool> {
        if !self.options.market_actions.allows_market_actions()
            || !self.config.relist
            || !should_recover_untracked_pending_draft(entry)
        {
            return Ok(false);
        }

        let item_uuid = draft.item_uuid.to_string();
        let queue = self
            .queue
            .snapshot(account)
            .await
            .map_err(anyhow::Error::from)?;
        let already_queued = queue.iter().any(|queued| {
            matches!(queued.state, BotState::Listing | BotState::ListingNoName)
                && string_value(
                    &queued.action,
                    &["inventory", "inv", "itemUuid", "itemUUID", "auctionID"],
                )
                .is_some_and(|queued_uuid| queued_uuid.eq_ignore_ascii_case(&item_uuid))
        });

        if !already_queued {
            let listing_hours = listing_hours_for_price(&self.config, draft.list_price as f64);
            self.queue
                .add(
                    account,
                    serde_json::json!({
                        "reason": "pending-draft-recovery",
                        "inventory": item_uuid.clone(),
                        "inv": item_uuid.clone(),
                        "itemUuid": item_uuid,
                        "itemName": draft.item_name.clone(),
                        "weirdItemName": draft.item_name.clone(),
                        "tag": draft.tag.clone(),
                        "price": draft.list_price,
                        "time": listing_hours,
                        "pricePaid": 0,
                        "targetPrice": draft.list_price,
                    }),
                    BotState::Listing,
                    1,
                )
                .await
                .map_err(anyhow::Error::from)?;
        }

        tracing::warn!(
            account = %account,
            state = %entry.state.as_str(),
            priority = entry.priority,
            draft_inventory = %draft.item_uuid,
            draft_item = %draft.item_name,
            draft_price = draft.list_price,
            already_queued,
            "recovered untracked pending auction draft as an explicit listing entry"
        );
        self.pending_market_steps.remove(account);
        self.complete_queue_entry_or_defer(account, entry, PendingCompletionKind::Reconcile)
            .await?;
        self.processed_queue_steps += 1;
        Ok(true)
    }

    async fn process_pending_draft_confirm_window(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        window: &WindowSnapshot,
    ) -> Result<bool> {
        if !self.pending_draft_submit_is_expected(account, entry) {
            tracing::warn!(
                account = %account,
                state = %entry.state.as_str(),
                priority = entry.priority,
                action = ?entry.action,
                "closing pending auction confirmation because no tracked draft submit preceded it"
            );
            return self
                .finish_reconcile_window(
                    account,
                    entry,
                    MarketInstruction::CloseWindow,
                    "close untracked pending auction confirmation",
                )
                .await;
        }
        if let Some(expected) = expected_pending_draft_listing(entry)
            && !window_contains_price(window, expected.list_price)
        {
            tracing::warn!(
                account = %account,
                state = %entry.state.as_str(),
                priority = entry.priority,
                expected_price = expected.list_price,
                "closing pending auction confirmation because confirmation price is missing or unexpected"
            );
            return self
                .finish_reconcile_window(
                    account,
                    entry,
                    MarketInstruction::CloseWindow,
                    "close pending auction confirmation with unexpected price",
                )
                .await;
        }
        let instruction = MarketInstruction::ClickSlot {
            slot: confirm_auction_slot(window),
        };
        if !self.should_attempt_market_step(account, entry, &instruction) {
            return Ok(true);
        }

        if self.options.market_actions.allows_market_actions() {
            if !self
                .execute_live_market_instruction_or_retry(
                    account,
                    entry,
                    &instruction,
                    "confirm pending auction draft",
                )
                .await
            {
                return Ok(true);
            }
            sleep(Duration::from_millis(1500)).await;
            let _ = self
                .session
                .execute_market_instruction(account, &MarketInstruction::CloseWindow)
                .await;
            if let Err(error) = self.stats.increment_auction_slots_used(account) {
                tracing::warn!(
                    account = %account,
                    error = %error,
                    "failed to increment pending draft slot stats"
                );
            }
            self.complete_reconcile_entry_and_queue_bids(account, entry)
                .await?;
        } else {
            self.queue.record_dry_run_step(
                account,
                entry,
                &instruction,
                "confirm pending auction draft",
            )?;
            self.pending_market_steps.insert(
                account.clone(),
                PendingMarketStep {
                    entry: entry.clone(),
                    instruction,
                    last_attempt: Instant::now(),
                    opens_sold_claim_action: false,
                },
            );
        }
        self.processed_queue_steps += 1;
        Ok(true)
    }

    async fn finish_reconcile_window(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        instruction: MarketInstruction,
        reason: &str,
    ) -> Result<bool> {
        if !self.should_attempt_market_step(account, entry, &instruction) {
            return Ok(true);
        }
        if self.options.market_actions.allows_market_actions() {
            if !self
                .execute_live_market_instruction_or_retry(account, entry, &instruction, reason)
                .await
            {
                return Ok(true);
            }
            self.complete_reconcile_entry_and_queue_bids(account, entry)
                .await?;
        } else {
            self.queue
                .record_dry_run_step(account, entry, &instruction, reason)?;
            self.pending_market_steps.insert(
                account.clone(),
                PendingMarketStep {
                    entry: entry.clone(),
                    instruction,
                    last_attempt: Instant::now(),
                    opens_sold_claim_action: false,
                },
            );
        }
        self.processed_queue_steps += 1;
        Ok(true)
    }

    pub(in crate::live_runtime) async fn process_expired_window_once(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        window: &WindowSnapshot,
    ) -> Result<bool> {
        if !is_expired_queue_entry(entry) || !is_manage_auctions_window(window) {
            return Ok(false);
        }
        let Some((slot, expired)) = expired_claim_from_window(entry, window) else {
            return Ok(false);
        };
        let instruction = MarketInstruction::ClickSlot { slot };
        if !self.should_attempt_market_step(account, entry, &instruction) {
            return Ok(true);
        }

        if self.options.market_actions.allows_market_actions() {
            if !self
                .execute_live_market_instruction_or_retry(
                    account,
                    entry,
                    &instruction,
                    "claim expired auction",
                )
                .await
            {
                return Ok(true);
            }
            sleep(Duration::from_millis(1500)).await;
            let _ = self
                .session
                .execute_market_instruction(account, &MarketInstruction::CloseWindow)
                .await;
            if let Some(expired) = expired {
                self.queue_expired_relist_entries(account, &[expired])?;
            }
            self.pending_market_steps.remove(account);
            self.clear_active_window_cache(account)?;
            self.complete_queue_entry_or_defer(account, entry, PendingCompletionKind::CountOnly)
                .await?;
        } else {
            self.queue.record_dry_run_step(
                account,
                entry,
                &instruction,
                "claim expired auction",
            )?;
            self.pending_market_steps.insert(
                account.clone(),
                PendingMarketStep {
                    entry: entry.clone(),
                    instruction,
                    last_attempt: Instant::now(),
                    opens_sold_claim_action: false,
                },
            );
        }

        self.processed_queue_steps += 1;
        Ok(true)
    }

    async fn complete_reconcile_entry_and_queue_bids(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
    ) -> Result<()> {
        self.pending_market_steps.remove(account);
        self.clear_active_window_cache(account)?;
        self.complete_queue_entry_or_defer(account, entry, PendingCompletionKind::Reconcile)
            .await
    }

    async fn all_expired_relist_entries_already_queued(
        &self,
        account: &AccountId,
        expired: &[super::super::auction_flow::ExpiredAuctionReconcile],
    ) -> Result<bool> {
        if !self.config.relist || !self.config.do_not_relist.expired_auctions {
            return Ok(false);
        }
        let pricing = saf_core::ExpiredRelistPricing::from_config(&self.config);
        let expected = expired
            .iter()
            .filter_map(|auction| auction.queue_action(account, &pricing))
            .collect::<Vec<_>>();
        if expected.is_empty() {
            return Ok(false);
        }
        let queue = self
            .queue
            .snapshot(account)
            .await
            .map_err(anyhow::Error::from)?;
        Ok(expected.iter().all(|action| {
            self.deferred_queue_entries.iter().any(|entry| {
                entry.account == *account
                    && entry.state == saf_core::BotState::ListingNoName
                    && entry.action == *action
            }) || queue.iter().any(|entry| {
                entry.state == saf_core::BotState::ListingNoName && entry.action == *action
            })
        }))
    }
}

fn is_auction_view_window(window: &WindowSnapshot) -> bool {
    let title = window.title.to_ascii_lowercase();
    title.contains("auction view") || title.contains("view auction")
}

#[derive(Debug)]
struct ExpectedPendingDraftListing {
    list_price: u64,
    inventory_uuid: Option<String>,
}

fn expected_pending_draft_listing(entry: &QueueEntry) -> Option<ExpectedPendingDraftListing> {
    let list_price = number_value(&entry.action, &["price", "oldPrice", "listPrice"])?
        .round()
        .max(0.0) as u64;
    (list_price >= 500).then(|| ExpectedPendingDraftListing {
        list_price,
        inventory_uuid: string_value(&entry.action, &["inventory", "inv", "itemUuid", "itemUUID"]),
    })
}

fn should_recover_untracked_pending_draft(entry: &QueueEntry) -> bool {
    matches!(
        string_value(&entry.action, &["reason"]).as_deref(),
        None | Some("")
            | Some("manual")
            | Some("manual-discord")
            | Some("listing-status-unclear")
            | Some("regular-poll")
            | Some("slot-pressure")
            | Some("startup")
    )
}

fn window_contains_price(window: &WindowSnapshot, price: u64) -> bool {
    let expected = saf_core::numbers::add_commas_to_number(price as f64).to_ascii_lowercase();
    let text = window_text_plain(window);
    text.contains(&expected) || text.contains(&price.to_string())
}
