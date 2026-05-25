use super::{
    DEFERRED_QUEUE_RETRY_DELAY, LiveRuntime, PendingCompletedQueueEntry, PendingCompletionKind,
    PendingTransferFollowup,
    auction_flow::{TransferFollowup, is_transfer_withdrawal_entry, transfer_followup},
};
use anyhow::{Context, Result};
use saf_core::ports::QueueStore;
use saf_core::{AccountId, BotState, QueueEntry, RuntimeDirective};
use std::time::Instant;

impl LiveRuntime {
    async fn advance_transfer_after_bank_deposit(
        &mut self,
        source: &AccountId,
        entry: &QueueEntry,
    ) -> Result<()> {
        let Some(transfer) = transfer_followup(entry) else {
            return Ok(());
        };
        if let Err(error) = self
            .try_advance_transfer_after_bank_deposit(source, &transfer)
            .await
        {
            tracing::warn!(
                source = %source,
                target = %transfer.target,
                error = %error,
                "failed to queue transfer follow-up; retrying later"
            );
            self.defer_transfer_followup(source, transfer);
        }
        Ok(())
    }

    async fn try_advance_transfer_after_bank_deposit(
        &self,
        source: &AccountId,
        transfer: &TransferFollowup,
    ) -> Result<()> {
        if transfer.stop_source {
            self.session
                .execute_directive(RuntimeDirective::StopAccounts {
                    account: Some(source.clone()),
                })
                .await
                .with_context(|| format!("stopping transfer source {source}"))?;
        }
        self.session
            .execute_directive(RuntimeDirective::StartAccounts {
                accounts: vec![transfer.target.clone()],
            })
            .await
            .with_context(|| format!("starting transfer target {}", transfer.target))?;
        if self
            .transfer_withdrawal_already_queued(source, transfer)
            .await?
        {
            return Ok(());
        }
        self.queue
            .add(
                &transfer.target,
                serde_json::json!({
                    "amount": transfer.amount,
                    "withdraw": true,
                    "personal": false,
                    "transfer": {
                        "from": source,
                        "complete": true
                    }
                }),
                BotState::Custom("bank".to_string()),
                5,
            )
            .await
            .map_err(anyhow::Error::from)?;
        Ok(())
    }

    async fn transfer_withdrawal_already_queued(
        &self,
        source: &AccountId,
        transfer: &TransferFollowup,
    ) -> Result<bool> {
        Ok(self
            .queue
            .snapshot(&transfer.target)
            .await
            .map_err(anyhow::Error::from)?
            .iter()
            .any(|entry| is_transfer_withdrawal_entry(entry, source, transfer)))
    }

    fn defer_transfer_followup(&mut self, source: &AccountId, transfer: TransferFollowup) {
        if self.pending_transfer_followups.iter().any(|pending| {
            pending.source == *source
                && pending.transfer.target == transfer.target
                && pending.transfer.amount == transfer.amount
                && pending.transfer.stop_source == transfer.stop_source
        }) {
            return;
        }
        self.pending_transfer_followups
            .push(PendingTransferFollowup {
                source: source.clone(),
                transfer,
                ready_at: Instant::now() + DEFERRED_QUEUE_RETRY_DELAY,
            });
    }

    pub(super) async fn drain_pending_transfer_followups(&mut self) -> Result<()> {
        let now = Instant::now();
        let mut pending = Vec::new();
        let mut ready = Vec::new();
        for followup in self.pending_transfer_followups.drain(..) {
            if followup.ready_at <= now {
                ready.push(followup);
            } else {
                pending.push(followup);
            }
        }
        self.pending_transfer_followups = pending;

        for mut followup in ready {
            if let Err(error) = self
                .try_advance_transfer_after_bank_deposit(&followup.source, &followup.transfer)
                .await
            {
                tracing::warn!(
                    source = %followup.source,
                    target = %followup.transfer.target,
                    error = %error,
                    "failed to retry transfer follow-up"
                );
                followup.ready_at = Instant::now() + DEFERRED_QUEUE_RETRY_DELAY;
                self.pending_transfer_followups.push(followup);
            }
        }
        Ok(())
    }

    pub(super) fn is_completion_pending(&self, account: &AccountId, entry: &QueueEntry) -> bool {
        self.pending_completed_entries
            .iter()
            .any(|pending| pending.account == *account && pending.entry == *entry)
    }

    pub(super) async fn complete_queue_entry_or_defer(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        kind: PendingCompletionKind,
    ) -> Result<()> {
        self.pending_missing_listing_inventory_retries
            .remove(account);
        self.pending_open_auction_retries.remove(account);
        self.finish_completed_queue_entry(account, entry, kind)
            .await?;
        match self.queue.remove_completed(account, entry).await {
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(
                    account = %account,
                    state = %entry.state.as_str(),
                    error = %error,
                    "failed to remove completed queue entry; retrying cleanup later"
                );
                self.defer_completed_queue_entry(account, entry, kind, true);
            }
        }
        Ok(())
    }

    fn defer_completed_queue_entry(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        kind: PendingCompletionKind,
        finished: bool,
    ) {
        if self.is_completion_pending(account, entry) {
            return;
        }
        self.pending_completed_entries
            .push(PendingCompletedQueueEntry {
                account: account.clone(),
                entry: entry.clone(),
                kind,
                finished,
                ready_at: Instant::now() + DEFERRED_QUEUE_RETRY_DELAY,
            });
    }

    pub(super) async fn drain_pending_completed_entries(&mut self) -> Result<()> {
        let now = Instant::now();
        let mut pending = Vec::new();
        let mut ready = Vec::new();
        for completion in self.pending_completed_entries.drain(..) {
            if completion.ready_at <= now {
                ready.push(completion);
            } else {
                pending.push(completion);
            }
        }
        self.pending_completed_entries = pending;

        for mut completion in ready {
            match self
                .queue
                .remove_completed(&completion.account, &completion.entry)
                .await
            {
                Ok(_) => {
                    let account = completion.account.clone();
                    let entry = completion.entry.clone();
                    if !completion.finished {
                        self.finish_completed_queue_entry(&account, &entry, completion.kind)
                            .await?;
                    }
                }
                Err(error) => {
                    tracing::warn!(
                        account = %completion.account,
                        state = %completion.entry.state.as_str(),
                        error = %error,
                        "failed to retry completed queue cleanup"
                    );
                    completion.ready_at = Instant::now() + DEFERRED_QUEUE_RETRY_DELAY;
                    self.pending_completed_entries.push(completion);
                }
            }
        }
        Ok(())
    }

    async fn finish_completed_queue_entry(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        kind: PendingCompletionKind,
    ) -> Result<()> {
        match kind {
            PendingCompletionKind::Generic => {
                self.register_completed_listing(account, entry);
                self.completed_queue_entries += 1;
                self.advance_transfer_after_bank_deposit(account, entry)
                    .await?;
            }
            PendingCompletionKind::Reconcile => {
                self.mark_auction_reconciled(account, Instant::now());
                self.completed_queue_entries += 1;
                self.queue_bids_followup_if_needed(account).await?;
            }
            PendingCompletionKind::CountOnly => {
                self.completed_queue_entries += 1;
            }
        }
        Ok(())
    }

    fn register_completed_listing(&self, account: &AccountId, entry: &QueueEntry) {
        if !matches!(entry.state, BotState::Listing | BotState::ListingNoName) {
            return;
        }
        if let Err(error) = self.sold_tracker.record_listing(account, &entry.action) {
            tracing::warn!(
                account = %account,
                error = %error,
                "failed to record completed listing metadata"
            );
        }
        if let Err(error) = self.stats.increment_auction_slots_used(account) {
            tracing::warn!(
                account = %account,
                error = %error,
                "failed to increment completed listing slot stats"
            );
        }
    }
}
