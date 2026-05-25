use super::super::auction_flow::{
    ClaimablePurchasedBid, PendingClaimedBidRelist, is_bids_queue_entry, is_bids_window,
    is_claimed_bid_listing_entry, is_purchased_bid_slot, purchased_bid_listing_action,
};
use super::super::{
    DEFERRED_QUEUE_RETRY_DELAY, LiveRuntime, PendingCompletionKind, PendingMarketStep,
};
use anyhow::Result;
use saf_core::gui::WindowSnapshot;
use saf_core::ports::QueueStore;
use saf_core::{AccountId, BotState, MarketInstruction, QueueEntry};
use std::time::{Duration, Instant};
use tokio::time::sleep;

impl LiveRuntime {
    pub(in crate::live_runtime) async fn process_bids_window_once(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        window: &WindowSnapshot,
    ) -> Result<bool> {
        if !is_bids_queue_entry(entry) || !is_bids_window(window) {
            return Ok(false);
        }

        let claimable = self.claimable_purchased_bids(account, window).await?;
        let Some(claim) = claimable.first().cloned() else {
            let instruction = MarketInstruction::CloseWindow;
            if !self.should_attempt_market_step(account, entry, &instruction) {
                return Ok(true);
            }
            if self.options.market_actions.allows_market_actions() {
                if !self
                    .execute_live_market_instruction_or_retry(
                        account,
                        entry,
                        &instruction,
                        "close bids window with no claimable purchased bids",
                    )
                    .await
                {
                    return Ok(true);
                }
                self.pending_market_steps.remove(account);
                self.clear_active_window_cache(account)?;
                self.complete_queue_entry_or_defer(
                    account,
                    entry,
                    PendingCompletionKind::CountOnly,
                )
                .await?;
            } else {
                self.queue.record_dry_run_step(
                    account,
                    entry,
                    &instruction,
                    "no claimable purchased bids",
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
            return Ok(true);
        };

        let instruction = MarketInstruction::ClickSlot { slot: claim.slot };
        if !self.should_attempt_market_step(account, entry, &instruction) {
            return Ok(true);
        }

        if self.options.market_actions.allows_market_actions() {
            if !self
                .execute_live_market_instruction_or_retry(
                    account,
                    entry,
                    &instruction,
                    "claim purchased bid",
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
            if let Err(error) = self
                .queue
                .queue_claimed_bid_relist(account, &claim.item_uuid, claim.listing_action.clone())
                .await
            {
                tracing::warn!(
                    account = %account,
                    item_uuid = %claim.item_uuid,
                    error = %error,
                    "failed to persist claimed bid relist; retrying later"
                );
                self.defer_claimed_bid_relist(account, claim);
            }
            self.pending_market_steps.remove(account);
            self.clear_active_window_cache(account)?;
        } else {
            self.queue
                .record_dry_run_step(account, entry, &instruction, "claim purchased bid")?;
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

    pub(in crate::live_runtime) async fn drain_pending_claimed_bid_relists(
        &mut self,
    ) -> Result<()> {
        let now = Instant::now();
        let mut pending = Vec::new();
        let mut ready = Vec::new();
        for relist in self.pending_claimed_bid_relists.drain(..) {
            if relist.ready_at <= now {
                ready.push(relist);
            } else {
                pending.push(relist);
            }
        }
        self.pending_claimed_bid_relists = pending;

        for mut relist in ready {
            let result = self
                .queue
                .queue_claimed_bid_relist(
                    &relist.account,
                    &relist.item_uuid,
                    relist.listing_action.clone(),
                )
                .await;
            match result {
                Ok(true) => {}
                Ok(false) => {
                    if let Err(error) = self
                        .queue_missing_claimed_bid_listing(&relist.account, &relist)
                        .await
                    {
                        tracing::warn!(
                            account = %relist.account,
                            item_uuid = %relist.item_uuid,
                            error = %error,
                            "failed to recover claimed bid listing without bid data"
                        );
                        relist.ready_at = Instant::now() + DEFERRED_QUEUE_RETRY_DELAY;
                        self.pending_claimed_bid_relists.push(relist);
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        account = %relist.account,
                        item_uuid = %relist.item_uuid,
                        error = %error,
                        "failed to retry claimed bid relist"
                    );
                    relist.ready_at = Instant::now() + DEFERRED_QUEUE_RETRY_DELAY;
                    self.pending_claimed_bid_relists.push(relist);
                }
            }
        }
        Ok(())
    }

    fn defer_claimed_bid_relist(&mut self, account: &AccountId, claim: ClaimablePurchasedBid) {
        if self
            .pending_claimed_bid_relists
            .iter()
            .any(|pending| pending.account == *account && pending.item_uuid == claim.item_uuid)
        {
            return;
        }
        self.pending_claimed_bid_relists
            .push(PendingClaimedBidRelist {
                account: account.clone(),
                item_uuid: claim.item_uuid,
                listing_action: claim.listing_action,
                ready_at: Instant::now() + DEFERRED_QUEUE_RETRY_DELAY,
            });
    }

    async fn queue_missing_claimed_bid_listing(
        &self,
        account: &AccountId,
        relist: &PendingClaimedBidRelist,
    ) -> Result<()> {
        let already_queued = self
            .queue
            .snapshot(account)
            .await
            .map_err(anyhow::Error::from)?
            .iter()
            .any(|entry| is_claimed_bid_listing_entry(entry, relist));
        if already_queued {
            return Ok(());
        }
        self.queue
            .add(account, relist.listing_action.clone(), BotState::Listing, 4)
            .await
            .map_err(anyhow::Error::from)?;
        Ok(())
    }

    async fn claimable_purchased_bids(
        &self,
        account: &AccountId,
        window: &WindowSnapshot,
    ) -> Result<Vec<ClaimablePurchasedBid>> {
        let mut claimable = Vec::new();
        for slot in &window.slots {
            if window.slots.len() > 36 && slot.slot >= window.slots.len() - 36 {
                continue;
            }
            let Some(item_uuid) = &slot.item_uuid else {
                continue;
            };
            if !is_purchased_bid_slot(slot) {
                continue;
            }
            let Some(bid_data) = self
                .queue
                .bid_data_entry(account, item_uuid)
                .await
                .map_err(anyhow::Error::from)?
            else {
                continue;
            };
            let Some(listing_action) = purchased_bid_listing_action(item_uuid, &bid_data, slot)
            else {
                continue;
            };
            claimable.push(ClaimablePurchasedBid {
                slot: slot.slot,
                item_uuid: item_uuid.clone(),
                listing_action,
            });
        }
        Ok(claimable)
    }
}
