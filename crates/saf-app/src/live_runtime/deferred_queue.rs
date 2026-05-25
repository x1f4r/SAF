use super::auction_flow::{
    ExpiredAuctionReconcile, is_bids_queue_entry, is_bids_state, is_reconcile_queue_entry,
    is_reconcile_state, sold_reconcile_action,
};
use super::{
    DEFERRED_QUEUE_RETRY_DELAY, DeferredQueueEntry, EXPIRED_RELIST_QUEUE_DELAY, LiveRuntime,
};
use anyhow::Result;
use saf_core::ports::QueueStore;
use saf_core::{AccountId, BotState, ExpiredRelistPricing};
use serde_json::Value;
use std::time::Instant;

impl LiveRuntime {
    pub(super) async fn queue_bids_followup_if_needed(
        &mut self,
        account: &AccountId,
    ) -> Result<()> {
        if let Err(error) = self.try_queue_bids_followup_if_needed(account).await {
            tracing::warn!(
                account = %account,
                error = %error,
                "failed to queue purchased-bids follow-up; retrying later"
            );
            self.defer_bids_followup(account);
        }
        Ok(())
    }

    async fn try_queue_bids_followup_if_needed(&self, account: &AccountId) -> Result<()> {
        if !self
            .queue
            .has_bid_data(account)
            .await
            .map_err(anyhow::Error::from)?
        {
            return Ok(());
        }
        let already_queued = self
            .queue
            .snapshot(account)
            .await
            .map_err(anyhow::Error::from)?
            .iter()
            .any(is_bids_queue_entry);
        if already_queued {
            return Ok(());
        }
        self.queue
            .add(
                account,
                serde_json::json!({"reason": "reconcile-purchased-bids"}),
                BotState::Custom("bids".to_string()),
                2,
            )
            .await
            .map_err(anyhow::Error::from)?;
        Ok(())
    }

    fn defer_bids_followup(&mut self, account: &AccountId) {
        if self
            .deferred_queue_entries
            .iter()
            .any(|entry| entry.account == *account && is_bids_state(&entry.state))
        {
            return;
        }
        self.deferred_queue_entries.push(DeferredQueueEntry {
            account: account.clone(),
            action: serde_json::json!({"reason": "reconcile-purchased-bids"}),
            state: BotState::Custom("bids".to_string()),
            priority: 2,
            ready_at: Instant::now() + DEFERRED_QUEUE_RETRY_DELAY,
        });
    }

    pub(super) fn queue_expired_relist_entries(
        &mut self,
        account: &AccountId,
        expired: &[ExpiredAuctionReconcile],
    ) -> Result<()> {
        if !self.config.relist || !self.config.do_not_relist.expired_auctions {
            return Ok(());
        }
        let pricing = ExpiredRelistPricing::from_config(&self.config);
        for auction in expired {
            let Some(action) = auction.queue_action(account, &pricing) else {
                continue;
            };
            if self.deferred_queue_entries.iter().any(|entry| {
                entry.account == *account
                    && entry.state == BotState::ListingNoName
                    && entry.action == action
            }) {
                continue;
            }
            self.deferred_queue_entries.push(DeferredQueueEntry {
                account: account.clone(),
                action,
                state: BotState::ListingNoName,
                priority: 4,
                ready_at: Instant::now() + EXPIRED_RELIST_QUEUE_DELAY,
            });
        }
        Ok(())
    }

    pub(super) async fn drain_deferred_queue_entries(&mut self) -> Result<()> {
        let now = Instant::now();
        let mut pending = Vec::new();
        let mut ready = Vec::new();
        for entry in self.deferred_queue_entries.drain(..) {
            if entry.ready_at <= now {
                ready.push(entry);
            } else {
                pending.push(entry);
            }
        }
        self.deferred_queue_entries = pending;

        for mut entry in ready {
            if is_bids_state(&entry.state) {
                let has_bid_data = match self.queue.has_bid_data(&entry.account).await {
                    Ok(has_bid_data) => has_bid_data,
                    Err(error) => {
                        tracing::warn!(
                            account = %entry.account,
                            state = %entry.state.as_str(),
                            error = %error,
                            "failed to inspect bid data for deferred entry; retrying later"
                        );
                        entry.ready_at = Instant::now() + DEFERRED_QUEUE_RETRY_DELAY;
                        self.deferred_queue_entries.push(entry);
                        continue;
                    }
                };
                if !has_bid_data {
                    continue;
                }
            }
            if is_reconcile_state(&entry.state) {
                let queue = match self.queue.snapshot(&entry.account).await {
                    Ok(queue) => queue,
                    Err(error) => {
                        tracing::warn!(
                            account = %entry.account,
                            state = %entry.state.as_str(),
                            error = %error,
                            "failed to inspect queue for deferred entry; retrying later"
                        );
                        entry.ready_at = Instant::now() + DEFERRED_QUEUE_RETRY_DELAY;
                        self.deferred_queue_entries.push(entry);
                        continue;
                    }
                };
                if queue.iter().any(is_reconcile_queue_entry) {
                    continue;
                }
            }
            if is_bids_state(&entry.state) {
                let queue = match self.queue.snapshot(&entry.account).await {
                    Ok(queue) => queue,
                    Err(error) => {
                        tracing::warn!(
                            account = %entry.account,
                            state = %entry.state.as_str(),
                            error = %error,
                            "failed to inspect queue for deferred entry; retrying later"
                        );
                        entry.ready_at = Instant::now() + DEFERRED_QUEUE_RETRY_DELAY;
                        self.deferred_queue_entries.push(entry);
                        continue;
                    }
                };
                if queue.iter().any(is_bids_queue_entry) {
                    continue;
                }
            }
            let queue = match self.queue.snapshot(&entry.account).await {
                Ok(queue) => queue,
                Err(error) => {
                    tracing::warn!(
                        account = %entry.account,
                        state = %entry.state.as_str(),
                        error = %error,
                        "failed to inspect queue for duplicate deferred entry; retrying later"
                    );
                    entry.ready_at = Instant::now() + DEFERRED_QUEUE_RETRY_DELAY;
                    self.deferred_queue_entries.push(entry);
                    continue;
                }
            };
            if queue
                .iter()
                .any(|queued| queued.state == entry.state && queued.action == entry.action)
            {
                continue;
            }
            if let Err(error) = self
                .queue
                .add(
                    &entry.account,
                    entry.action.clone(),
                    entry.state.clone(),
                    entry.priority,
                )
                .await
            {
                tracing::warn!(
                    account = %entry.account,
                    state = %entry.state.as_str(),
                    error = %error,
                    "failed to persist deferred queue entry; retrying later"
                );
                entry.ready_at = Instant::now() + DEFERRED_QUEUE_RETRY_DELAY;
                self.deferred_queue_entries.push(entry);
            }
        }
        Ok(())
    }

    pub(super) async fn queue_sold_reconcile_or_defer(
        &mut self,
        account: &AccountId,
        sold: &super::stats::SoldStatsUpdate,
    ) -> Result<()> {
        let action = sold_reconcile_action(sold);
        if let Err(error) = self
            .queue
            .add(
                account,
                action.clone(),
                BotState::Custom("reconcileAuctions".to_string()),
                0,
            )
            .await
        {
            tracing::warn!(
                account = %account,
                error = %error,
                "failed to queue sold-message reconcile; retrying later"
            );
            self.defer_sold_reconcile(account, action);
        }
        Ok(())
    }

    fn defer_sold_reconcile(&mut self, account: &AccountId, action: Value) {
        let state = BotState::Custom("reconcileAuctions".to_string());
        if self.deferred_queue_entries.iter().any(|entry| {
            entry.account == *account && entry.state == state && entry.action == action
        }) {
            return;
        }
        self.deferred_queue_entries.push(DeferredQueueEntry {
            account: account.clone(),
            action,
            state,
            priority: 0,
            ready_at: Instant::now() + DEFERRED_QUEUE_RETRY_DELAY,
        });
    }
}
