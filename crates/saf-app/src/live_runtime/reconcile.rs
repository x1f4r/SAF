use super::LiveRuntime;
use super::auction_flow::is_reconcile_queue_entry;
use anyhow::Result;
use saf_core::ports::QueueStore;
use saf_core::{AccountId, BotState};
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub(super) struct LiveAuctionReconcilePoller {
    poll_interval: Duration,
    idle_interval: Option<Duration>,
    pub(super) states: BTreeMap<AccountId, AuctionReconcilePollState>,
}

#[derive(Clone, Debug)]
pub(super) struct AuctionReconcilePollState {
    pub(super) next_poll_at: Instant,
    pub(super) last_reconcile_at: Instant,
}

impl LiveAuctionReconcilePoller {
    pub(super) fn new(
        accounts: &[AccountId],
        poll_interval: Duration,
        idle_interval: Option<Duration>,
    ) -> Self {
        let now = Instant::now();
        Self {
            poll_interval,
            idle_interval,
            states: accounts
                .iter()
                .map(|account| {
                    (
                        account.clone(),
                        AuctionReconcilePollState {
                            next_poll_at: now + poll_interval,
                            last_reconcile_at: now,
                        },
                    )
                })
                .collect(),
        }
    }

    pub(super) fn take_due(&mut self, account: &AccountId, now: Instant) -> bool {
        let state =
            self.states
                .entry(account.clone())
                .or_insert_with(|| AuctionReconcilePollState {
                    next_poll_at: now + self.poll_interval,
                    last_reconcile_at: now,
                });
        if state.next_poll_at > now {
            return false;
        }
        state.next_poll_at = now + self.poll_interval;
        true
    }

    pub(super) fn needs_idle_reconcile(&self, account: &AccountId, now: Instant) -> bool {
        let Some(idle_interval) = self.idle_interval else {
            return false;
        };
        self.states
            .get(account)
            .is_some_and(|state| now.duration_since(state.last_reconcile_at) >= idle_interval)
    }

    pub(super) fn mark_reconciled(&mut self, account: &AccountId, now: Instant) {
        let state =
            self.states
                .entry(account.clone())
                .or_insert_with(|| AuctionReconcilePollState {
                    next_poll_at: now + self.poll_interval,
                    last_reconcile_at: now,
                });
        state.last_reconcile_at = now;
    }
}

impl LiveRuntime {
    pub(super) async fn queue_reconciliation_polls_once(&mut self) -> Result<()> {
        if !self.config.use_cookie || !self.config.relist {
            return Ok(());
        }
        let now = Instant::now();
        let ready_accounts = self
            .running_accounts()
            .into_iter()
            .filter(|account| self.account_market_ready(account))
            .collect::<Vec<_>>();
        let Some(poller) = self.auction_reconcile_poller.as_mut() else {
            return Ok(());
        };
        let due_accounts = ready_accounts
            .into_iter()
            .filter(|account| poller.take_due(account, now))
            .collect::<Vec<_>>();

        for account in due_accounts {
            let queue = match self.queue.snapshot(&account).await {
                Ok(queue) => queue,
                Err(error) => {
                    tracing::warn!(
                        account = %account,
                        error = %error,
                        "failed to inspect queue for reconciliation poll"
                    );
                    continue;
                }
            };
            if queue.iter().any(is_reconcile_queue_entry) {
                continue;
            }

            let has_pending_listing = queue
                .iter()
                .any(|entry| matches!(entry.state, BotState::Listing | BotState::ListingNoName));
            let auction_slots_full = match self.stats.auction_slots_full(&account) {
                Ok(full) => full,
                Err(error) => {
                    tracing::warn!(
                        account = %account,
                        error = %error,
                        "failed to inspect auction slot stats for reconciliation poll"
                    );
                    continue;
                }
            };
            let needs_idle_reconcile = self
                .auction_reconcile_poller
                .as_ref()
                .is_some_and(|poller| poller.needs_idle_reconcile(&account, now));
            let reason = if has_pending_listing || auction_slots_full {
                Some("slot-pressure")
            } else if needs_idle_reconcile {
                Some("regular-poll")
            } else {
                None
            };
            let Some(reason) = reason else {
                continue;
            };

            match self
                .queue
                .add(
                    &account,
                    serde_json::json!({ "reason": reason }),
                    BotState::Custom("reconcileAuctions".to_string()),
                    reconciliation_priority(reason),
                )
                .await
            {
                Ok(_) => self.mark_auction_reconciled(&account, now),
                Err(error) => {
                    tracing::warn!(
                        account = %account,
                        reason = %reason,
                        error = %error,
                        "failed to queue reconciliation poll"
                    );
                }
            }
        }
        Ok(())
    }

    pub(super) fn mark_auction_reconciled(&mut self, account: &AccountId, now: Instant) {
        if let Some(poller) = self.auction_reconcile_poller.as_mut() {
            poller.mark_reconciled(account, now);
        }
    }
}

fn reconciliation_priority(reason: &str) -> u8 {
    match reason {
        "slot-pressure" => 0,
        _ => 2,
    }
}
