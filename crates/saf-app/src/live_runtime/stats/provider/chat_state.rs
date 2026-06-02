use super::super::super::support::normalized_purchase_item_name;
use super::super::super::{LiveTrackedFlipProvider, now_ms};
use super::super::chat::{
    ChatStatsUpdate, ClaimStatsUpdate, PurchaseStatsUpdate, SoldStatsUpdate,
    parse_claimed_sold_chat_message, parse_hypixel_ping_ms, parse_own_auction_collection_message,
    parse_purchase_chat_message, parse_sold_chat_message,
};
use super::{
    AUCTION_COLLECTION_CONTEXT_LIMIT, AUCTION_COLLECTION_CONTEXT_TTL_MS, LiveStatsProvider,
    PendingAuctionCollection, PendingZeroClaim,
};
use saf_core::AccountId;
use saf_core::ports::PortError;
use std::collections::VecDeque;

impl LiveStatsProvider {
    pub(in crate::live_runtime) fn record_chat_message(
        &self,
        account: &AccountId,
        text: &str,
        tracked_flips: &LiveTrackedFlipProvider,
    ) -> Result<ChatStatsUpdate, PortError> {
        if !self.accounts.contains(account) {
            return Err(PortError::Unavailable(format!(
                "no stats provider for {account}"
            )));
        }
        let now = now_ms();
        self.prune_auction_collection_contexts(account, now)?;
        let mut update = ChatStatsUpdate::default();
        if let Some(hypixel_ping) = parse_hypixel_ping_ms(text) {
            let mut ping = self
                .cofl_ping
                .lock()
                .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?;
            ping.entry(account.clone()).or_default().hypixel_ping_ms = Some(hypixel_ping);
        }
        if let Some(purchase) = parse_purchase_chat_message(text)
            && let Some(tracked) =
                tracked_flips.take_purchase(account, &purchase.item_name, purchase.price)?
        {
            let price_paid = tracked.price_paid.unwrap_or(purchase.price as f64);
            let profit = saf_core::flip::ihate_taxes(tracked.target_price) - price_paid;
            self.record_bought_profit(account, profit)?;
            let item_name = purchase.item_name;
            let weird_item_name = tracked
                .weird_item_name
                .unwrap_or_else(|| normalized_purchase_item_name(&item_name));
            update.purchase = Some(PurchaseStatsUpdate {
                auction_id: tracked.auction_id,
                item_name,
                weird_item_name,
                tag: tracked.tag,
                price: purchase.price,
                target_price: tracked.target_price,
                profit,
                finder: tracked.finder.unwrap_or_else(|| "UNKNOWN".to_string()),
                volume: tracked.volume,
                profit_percentage: tracked.profit_percentage,
                buy_kind: tracked.buy_kind.unwrap_or_else(|| "NUGGET".to_string()),
                buy_speed_ms: tracked
                    .seen_at_ms
                    .map(|seen_at| now.saturating_sub(seen_at)),
            });
        }
        if let Some(sold) = parse_sold_chat_message(text) {
            self.record_sold(account)?;
            update.sold = Some(SoldStatsUpdate {
                buyer: sold.buyer,
                item_name: sold.item_name,
                price: sold.price,
            });
        }
        if let Some(collection) = parse_own_auction_collection_message(text)
            && collection.collector.eq_ignore_ascii_case(account.as_str())
        {
            if let Some(pending) = self.take_pending_zero_claim(account, now)? {
                update.claim = Some(ClaimStatsUpdate {
                    coins: collection.coins,
                    item_name: pending.item_name,
                    buyer: pending.buyer,
                });
            } else {
                self.remember_auction_collection(account, collection.coins, now)?;
            }
        }
        if let Some(claim) = parse_claimed_sold_chat_message(text) {
            let coins = self.resolve_auction_collection(account, claim.coins, now)?;
            if coins > 0 || claim.coins > 0 {
                update.claim = Some(ClaimStatsUpdate {
                    coins,
                    item_name: claim.item_name,
                    buyer: claim.buyer,
                });
            } else {
                self.remember_pending_zero_claim(
                    account,
                    PendingZeroClaim {
                        item_name: claim.item_name,
                        buyer: claim.buyer,
                        at_ms: now,
                    },
                )?;
            }
        }
        #[cfg(feature = "api")]
        if let Some(sink) = &self.dashboard_sink {
            use crate::live_runtime::dashboard::{ClaimRecord, FlipRecord, LiveEvent, SaleRecord};
            if let Some(purchase) = &update.purchase {
                sink.emit(LiveEvent::Purchase(FlipRecord::from_update(
                    account, purchase, now,
                )));
            }
            if let Some(sold) = &update.sold {
                sink.emit(LiveEvent::Sold(SaleRecord::from_update(account, sold, now)));
            }
            if let Some(claim) = &update.claim {
                sink.emit(LiveEvent::Claim(ClaimRecord::from_update(
                    account, claim, now,
                )));
            }
        }
        Ok(update)
    }

    fn prune_auction_collection_contexts(
        &self,
        account: &AccountId,
        now: u64,
    ) -> Result<(), PortError> {
        let cutoff = now.saturating_sub(AUCTION_COLLECTION_CONTEXT_TTL_MS);
        if let Some(collections) = self
            .auction_collections
            .lock()
            .map_err(|_| PortError::Failed("auction collection lock poisoned".to_string()))?
            .get_mut(account)
        {
            while collections
                .front()
                .is_some_and(|entry| entry.at_ms < cutoff)
            {
                collections.pop_front();
            }
        }
        if let Some(claims) = self
            .pending_zero_claims
            .lock()
            .map_err(|_| PortError::Failed("pending zero-claim lock poisoned".to_string()))?
            .get_mut(account)
        {
            while claims.front().is_some_and(|entry| entry.at_ms < cutoff) {
                claims.pop_front();
            }
        }
        Ok(())
    }

    fn remember_auction_collection(
        &self,
        account: &AccountId,
        coins: u64,
        now: u64,
    ) -> Result<(), PortError> {
        if coins == 0 {
            return Ok(());
        }
        let mut collections = self
            .auction_collections
            .lock()
            .map_err(|_| PortError::Failed("auction collection lock poisoned".to_string()))?;
        let account_collections = collections.entry(account.clone()).or_default();
        account_collections.push_back(PendingAuctionCollection { coins, at_ms: now });
        while account_collections.len() > AUCTION_COLLECTION_CONTEXT_LIMIT {
            account_collections.pop_front();
        }
        Ok(())
    }

    fn resolve_auction_collection(
        &self,
        account: &AccountId,
        raw_coins: u64,
        now: u64,
    ) -> Result<u64, PortError> {
        self.prune_auction_collection_contexts(account, now)?;
        let mut collections = self
            .auction_collections
            .lock()
            .map_err(|_| PortError::Failed("auction collection lock poisoned".to_string()))?;
        let Some(account_collections) = collections.get_mut(account) else {
            return Ok(raw_coins);
        };
        if raw_coins > 0 {
            if let Some(index) = account_collections
                .iter()
                .position(|entry| entry.coins == raw_coins)
            {
                account_collections.remove(index);
            }
            return Ok(raw_coins);
        }
        Ok(account_collections
            .pop_front()
            .map(|entry| entry.coins)
            .unwrap_or(0))
    }

    fn remember_pending_zero_claim(
        &self,
        account: &AccountId,
        claim: PendingZeroClaim,
    ) -> Result<(), PortError> {
        let mut claims = self
            .pending_zero_claims
            .lock()
            .map_err(|_| PortError::Failed("pending zero-claim lock poisoned".to_string()))?;
        let account_claims = claims.entry(account.clone()).or_default();
        account_claims.push_back(claim);
        while account_claims.len() > AUCTION_COLLECTION_CONTEXT_LIMIT {
            account_claims.pop_front();
        }
        Ok(())
    }

    fn take_pending_zero_claim(
        &self,
        account: &AccountId,
        now: u64,
    ) -> Result<Option<PendingZeroClaim>, PortError> {
        self.prune_auction_collection_contexts(account, now)?;
        Ok(self
            .pending_zero_claims
            .lock()
            .map_err(|_| PortError::Failed("pending zero-claim lock poisoned".to_string()))?
            .get_mut(account)
            .and_then(VecDeque::pop_front))
    }

    fn record_bought_profit(&self, account: &AccountId, profit: f64) -> Result<(), PortError> {
        self.bought_profits
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .entry(account.clone())
            .or_default()
            .push(profit);
        Ok(())
    }

    fn record_sold(&self, account: &AccountId) -> Result<(), PortError> {
        *self
            .sold_counts
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .entry(account.clone())
            .or_default() += 1;
        Ok(())
    }
}
