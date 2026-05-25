mod queue_store;
mod tracked;

pub use queue_store::FileQueueStore;

use saf_core::{BotState, QueueEntry, StateStore};
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateSnapshot {
    pub bid_data: Value,
    pub queue: Vec<QueueEntry>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateMutationReport {
    pub changed: bool,
    pub saved: bool,
    pub removed: Option<QueueEntry>,
    pub snapshot: StateSnapshot,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StateClearReport {
    pub removed: usize,
    pub saved: bool,
    pub snapshot: StateSnapshot,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedDataClearReport {
    pub queue_removed: usize,
    pub bid_data_cleared: bool,
    pub saved: bool,
    pub snapshot: StateSnapshot,
}

pub fn snapshot(
    base_dir: impl AsRef<Path>,
    account_uuid: &str,
) -> Result<StateSnapshot, saf_core::state::StateStoreError> {
    let store = StateStore::open(base_dir, account_uuid)?;
    Ok(snapshot_from_store(&store))
}

pub fn add_queue_entry(
    base_dir: impl AsRef<Path>,
    account_uuid: &str,
    action: Value,
    state: BotState,
    priority: u8,
    save: bool,
) -> Result<StateMutationReport, saf_core::state::StateStoreError> {
    let mut store = StateStore::open(base_dir, account_uuid)?;
    let changed = store.add(action, state, priority);
    if save {
        store.save_all()?;
    }
    Ok(StateMutationReport {
        changed,
        saved: save,
        removed: None,
        snapshot: snapshot_from_store(&store),
    })
}

pub fn remove_next(
    base_dir: impl AsRef<Path>,
    account_uuid: &str,
    save: bool,
) -> Result<StateMutationReport, saf_core::state::StateStoreError> {
    let mut store = StateStore::open(base_dir, account_uuid)?;
    let removed = store.remove_next();
    if save {
        store.save()?;
    }
    Ok(StateMutationReport {
        changed: removed.is_some(),
        saved: save,
        removed,
        snapshot: snapshot_from_store(&store),
    })
}

pub fn clear_queue(
    base_dir: impl AsRef<Path>,
    account_uuid: &str,
    save: bool,
) -> Result<StateClearReport, saf_core::state::StateStoreError> {
    let mut store = StateStore::open(base_dir, account_uuid)?;
    let removed = store.clear_queue();
    if save {
        store.save_all()?;
    }
    Ok(StateClearReport {
        removed,
        saved: save,
        snapshot: snapshot_from_store(&store),
    })
}

pub fn clear_saved_data(
    base_dir: impl AsRef<Path>,
    account_uuid: &str,
    save: bool,
) -> Result<SavedDataClearReport, saf_core::state::StateStoreError> {
    let mut store = StateStore::open(base_dir, account_uuid)?;
    let cleared = store.clear_saved_data();
    if save {
        store.save_all()?;
    }
    Ok(SavedDataClearReport {
        queue_removed: cleared.queue_removed,
        bid_data_cleared: cleared.bid_data_cleared,
        saved: save,
        snapshot: snapshot_from_store(&store),
    })
}

fn snapshot_from_store(store: &StateStore) -> StateSnapshot {
    StateSnapshot {
        bid_data: store.bid_data().clone(),
        queue: store.queue().to_vec(),
    }
}

#[cfg(test)]
mod tests;
