use super::tracked::tracked_flip_from_bid_data;
use async_trait::async_trait;
use saf_core::ports::{PortError, QueueStore, SavedDataStore, TrackedFlip, TrackedFlipProvider};
use saf_core::{AccountId, BotState, QueueEntry, SavedDataClear, StateStore};
use serde_json::Value;
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct FileQueueStore {
    base_dir: PathBuf,
}

impl FileQueueStore {
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }

    pub async fn remove_matching(
        &self,
        account: &AccountId,
        expected: &QueueEntry,
    ) -> Result<bool, PortError> {
        let mut store = StateStore::open(&self.base_dir, account.as_str())
            .map_err(|error| PortError::Failed(error.to_string()))?;
        let removed = store.remove_first_matching(|entry| entry == expected);
        if removed.is_some() {
            store
                .save_all()
                .map_err(|error| PortError::Failed(error.to_string()))?;
        }
        Ok(removed.is_some())
    }

    pub async fn bid_data_entry(
        &self,
        account: &AccountId,
        item_uuid: &str,
    ) -> Result<Option<Value>, PortError> {
        StateStore::open(&self.base_dir, account.as_str())
            .map(|store| store.bid_data_entry(item_uuid).cloned())
            .map_err(|error| PortError::Failed(error.to_string()))
    }

    pub async fn has_bid_data(&self, account: &AccountId) -> Result<bool, PortError> {
        StateStore::open(&self.base_dir, account.as_str())
            .map(|store| {
                store
                    .bid_data()
                    .as_object()
                    .is_some_and(|entries| !entries.is_empty())
            })
            .map_err(|error| PortError::Failed(error.to_string()))
    }

    pub async fn queue_claimed_bid_relist(
        &self,
        account: &AccountId,
        item_uuid: &str,
        action: Value,
    ) -> Result<bool, PortError> {
        let mut store = StateStore::open(&self.base_dir, account.as_str())
            .map_err(|error| PortError::Failed(error.to_string()))?;
        if store.remove_bid_data_entry(item_uuid).is_none() {
            return Ok(false);
        }
        store.add(action, BotState::Listing, 4);
        store
            .save_all()
            .map_err(|error| PortError::Failed(error.to_string()))?;
        Ok(true)
    }
}

#[async_trait]
impl QueueStore for FileQueueStore {
    async fn add(
        &self,
        account: &AccountId,
        action: Value,
        state: BotState,
        priority: u8,
    ) -> Result<bool, PortError> {
        let mut store = StateStore::open(&self.base_dir, account.as_str())
            .map_err(|error| PortError::Failed(error.to_string()))?;
        let changed = store.add(action, state, priority);
        store
            .save_all()
            .map_err(|error| PortError::Failed(error.to_string()))?;
        Ok(changed)
    }

    async fn snapshot(&self, account: &AccountId) -> Result<Vec<QueueEntry>, PortError> {
        StateStore::open(&self.base_dir, account.as_str())
            .map(|store| store.queue().to_vec())
            .map_err(|error| PortError::Failed(error.to_string()))
    }

    async fn clear(&self, account: &AccountId) -> Result<usize, PortError> {
        let mut store = StateStore::open(&self.base_dir, account.as_str())
            .map_err(|error| PortError::Failed(error.to_string()))?;
        let removed = store.clear_queue();
        store
            .save_all()
            .map_err(|error| PortError::Failed(error.to_string()))?;
        Ok(removed)
    }
}

#[async_trait]
impl SavedDataStore for FileQueueStore {
    async fn clear_saved_data(&self, account: &AccountId) -> Result<SavedDataClear, PortError> {
        let mut store = StateStore::open(&self.base_dir, account.as_str())
            .map_err(|error| PortError::Failed(error.to_string()))?;
        let cleared = store.clear_saved_data();
        store
            .save_all()
            .map_err(|error| PortError::Failed(error.to_string()))?;
        Ok(cleared)
    }
}

#[async_trait]
impl TrackedFlipProvider for FileQueueStore {
    async fn lookup(
        &self,
        account: &AccountId,
        auction_id: &str,
    ) -> Result<Option<TrackedFlip>, PortError> {
        let store = StateStore::open(&self.base_dir, account.as_str())
            .map_err(|error| PortError::Failed(error.to_string()))?;
        Ok(tracked_flip_from_bid_data(store.bid_data(), auction_id))
    }
}
