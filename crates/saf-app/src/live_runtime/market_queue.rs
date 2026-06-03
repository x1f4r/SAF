use super::MarketActionMode;
use crate::state_cli::FileQueueStore;
use anyhow::Result;
use async_trait::async_trait;
#[cfg(test)]
use saf_core::ports::MinecraftClient;
use saf_core::ports::{MinecraftAction, PortError, QueueStore};
use saf_core::{AccountId, BotState, MarketInstruction, QueueEntry};
use serde::Serialize;
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunMarketAction {
    pub account: AccountId,
    pub state: BotState,
    pub priority: u8,
    pub action: Value,
}

#[derive(Clone, Debug)]
pub struct MarketActionQueueStore {
    inner: FileQueueStore,
    mode: MarketActionMode,
    dry_run_records: Arc<Mutex<Vec<DryRunMarketAction>>>,
}

impl MarketActionQueueStore {
    pub fn new(inner: FileQueueStore, mode: MarketActionMode) -> Self {
        Self {
            inner,
            mode,
            dry_run_records: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn dry_run_records(&self) -> Vec<DryRunMarketAction> {
        match self.dry_run_records.lock() {
            Ok(records) => records.clone(),
            Err(_) => {
                tracing::warn!("dry-run market action lock poisoned; reporting empty records");
                Vec::new()
            }
        }
    }

    pub fn record_dry_run_step(
        &self,
        account: &AccountId,
        entry: &QueueEntry,
        instruction: &MarketInstruction,
        reason: &str,
    ) -> Result<(), PortError> {
        let action = serde_json::json!({
            "queuedAction": entry.action.clone(),
            "instruction": instruction.clone(),
            "reason": reason
        });
        self.dry_run_records
            .lock()
            .map_err(|_| PortError::Failed("dry-run market action lock poisoned".to_string()))?
            .push(DryRunMarketAction {
                account: account.clone(),
                state: entry.state.clone(),
                priority: entry.priority,
                action,
            });
        Ok(())
    }

    pub fn record_dry_run_action(
        &self,
        account: &AccountId,
        state: BotState,
        priority: u8,
        action: Value,
    ) -> Result<(), PortError> {
        self.dry_run_records
            .lock()
            .map_err(|_| PortError::Failed("dry-run market action lock poisoned".to_string()))?
            .push(DryRunMarketAction {
                account: account.clone(),
                state,
                priority,
                action,
            });
        Ok(())
    }

    pub async fn remove_completed(
        &self,
        account: &AccountId,
        entry: &QueueEntry,
    ) -> Result<bool, PortError> {
        if !self.mode.allows_market_actions() {
            return Ok(false);
        }
        self.inner.remove_matching(account, entry).await
    }

    pub async fn bid_data_entry(
        &self,
        account: &AccountId,
        item_uuid: &str,
    ) -> Result<Option<Value>, PortError> {
        self.inner.bid_data_entry(account, item_uuid).await
    }

    pub async fn has_bid_data(&self, account: &AccountId) -> Result<bool, PortError> {
        self.inner.has_bid_data(account).await
    }

    pub async fn queue_claimed_bid_relist(
        &self,
        account: &AccountId,
        item_uuid: &str,
        action: Value,
    ) -> Result<bool, PortError> {
        if !self.mode.allows_market_actions() {
            self.dry_run_records
                .lock()
                .map_err(|_| PortError::Failed("dry-run market action lock poisoned".to_string()))?
                .push(DryRunMarketAction {
                    account: account.clone(),
                    state: BotState::Listing,
                    priority: 4,
                    action,
                });
            return Ok(true);
        }
        self.inner
            .queue_claimed_bid_relist(account, item_uuid, action)
            .await
    }
}

#[async_trait]
impl QueueStore for MarketActionQueueStore {
    async fn add(
        &self,
        account: &AccountId,
        action: Value,
        state: BotState,
        priority: u8,
    ) -> Result<bool, PortError> {
        if !self.mode.allows_market_actions() && is_market_state(&state) {
            self.dry_run_records
                .lock()
                .map_err(|_| PortError::Failed("dry-run market action lock poisoned".to_string()))?
                .push(DryRunMarketAction {
                    account: account.clone(),
                    state,
                    priority,
                    action,
                });
            return Ok(true);
        }

        self.inner.add(account, action, state, priority).await
    }

    async fn snapshot(&self, account: &AccountId) -> Result<Vec<QueueEntry>, PortError> {
        self.inner.snapshot(account).await
    }

    async fn clear(&self, account: &AccountId) -> Result<usize, PortError> {
        self.inner.clear(account).await
    }

    async fn remove_at(
        &self,
        account: &AccountId,
        index: usize,
    ) -> Result<Option<QueueEntry>, PortError> {
        self.inner.remove_at(account, index).await
    }
}

fn is_market_state(state: &BotState) -> bool {
    match state {
        BotState::Buying
        | BotState::Listing
        | BotState::ListingNoName
        | BotState::Delisting
        | BotState::Expired => true,
        BotState::Custom(name) => matches!(
            name.as_str(),
            "bank"
                | "bids"
                | "claimPurchased"
                | "externalBuying"
                | "claimSold"
                | "reconcileAuctions"
                | "sellInventory"
        ),
        _ => false,
    }
}

#[derive(Clone)]
#[cfg(test)]
pub(super) struct MarketGuardMinecraftClient {
    pub(super) inner: Arc<dyn MinecraftClient>,
    pub(super) mode: MarketActionMode,
}

#[async_trait]
#[cfg(test)]
impl MinecraftClient for MarketGuardMinecraftClient {
    async fn account(&self) -> AccountId {
        self.inner.account().await
    }

    async fn perform(&self, action: MinecraftAction) -> Result<(), PortError> {
        if !self.mode.allows_market_actions() && is_market_minecraft_action(&action) {
            return Ok(());
        }
        self.inner.perform(action).await
    }

    async fn next_event(&self) -> Result<Option<saf_core::ports::MinecraftEvent>, PortError> {
        self.inner.next_event().await
    }
}

pub(super) fn is_market_minecraft_action(action: &MinecraftAction) -> bool {
    matches!(
        action,
        MinecraftAction::OpenAuction(_)
            | MinecraftAction::ClickSlot(_)
            | MinecraftAction::SwapSlotToHotbar { .. }
            | MinecraftAction::SetHeldHotbarSlot(_)
            | MinecraftAction::ActivateHeldItem
            | MinecraftAction::TypeText(_)
    )
}
