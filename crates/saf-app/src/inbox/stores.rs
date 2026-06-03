use super::records::{
    AccountScheduleRecord, BlacklistRecord, LifecycleAction, LifecycleRecord, QueueRecord,
    SavedDataClearRecord, StatsRequestKind, StatsRequestRecord,
};
use crate::state_cli::FileQueueStore;
use async_trait::async_trait;
use saf_core::ports::{
    AccountPing, AccountScheduleRequest, AccountScheduleResult, AccountScheduler, AccountStats,
    AccountStatsProvider, AccountSupervisor, BlacklistStore, LogReader, LogSnapshot, Notification,
    Notifier, PortError, QueueStore, SavedDataStore,
};
use saf_core::{
    AccountId, BlacklistApplyResult, BlacklistRequest, BotState, QueueEntry, SavedDataClear,
};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct RecordedQueueStore {
    entries: Arc<Mutex<Vec<QueueRecord>>>,
}

impl RecordedQueueStore {
    pub fn entries(&self) -> Vec<QueueRecord> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[async_trait]
impl QueueStore for RecordedQueueStore {
    async fn add(
        &self,
        account: &AccountId,
        action: Value,
        state: BotState,
        priority: u8,
    ) -> Result<bool, PortError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| PortError::Failed("queue lock poisoned".to_string()))?;
        entries.push(QueueRecord {
            account: account.clone(),
            action,
            state,
            priority,
        });
        // Mirror FileQueueStore's stable priority sort (StateStore::add) so a
        // snapshot index shared between the file and recorded stores (used by
        // cancel_queue / remove_at) resolves to the same entry in both.
        entries.sort_by_key(|entry| entry.priority);
        Ok(true)
    }

    async fn snapshot(&self, account: &AccountId) -> Result<Vec<QueueEntry>, PortError> {
        Ok(self
            .entries
            .lock()
            .map_err(|_| PortError::Failed("queue lock poisoned".to_string()))?
            .iter()
            .filter(|entry| &entry.account == account)
            .cloned()
            .map(QueueEntry::from)
            .collect())
    }

    async fn clear(&self, account: &AccountId) -> Result<usize, PortError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| PortError::Failed("queue lock poisoned".to_string()))?;
        let before = entries.len();
        entries.retain(|entry| &entry.account != account);
        Ok(before - entries.len())
    }

    async fn remove_at(
        &self,
        account: &AccountId,
        index: usize,
    ) -> Result<Option<QueueEntry>, PortError> {
        let mut entries = self
            .entries
            .lock()
            .map_err(|_| PortError::Failed("queue lock poisoned".to_string()))?;
        let position = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| &entry.account == account)
            .nth(index)
            .map(|(position, _)| position);
        Ok(position.map(|position| QueueEntry::from(entries.remove(position))))
    }
}

#[derive(Clone, Debug)]
pub struct RecordedSavedDataStore {
    queue: Arc<RecordedQueueStore>,
    records: Arc<Mutex<Vec<SavedDataClearRecord>>>,
}

impl RecordedSavedDataStore {
    pub(super) fn new(queue: Arc<RecordedQueueStore>) -> Self {
        Self {
            queue,
            records: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn records(&self) -> Vec<SavedDataClearRecord> {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn record_clear(&self, account: &AccountId, result: SavedDataClear) -> Result<(), PortError> {
        self.records
            .lock()
            .map_err(|_| PortError::Failed("saved data lock poisoned".to_string()))?
            .push(SavedDataClearRecord {
                account: account.clone(),
                queue_removed: result.queue_removed,
                bid_data_cleared: result.bid_data_cleared,
            });
        Ok(())
    }
}

#[async_trait]
impl SavedDataStore for RecordedSavedDataStore {
    async fn clear_saved_data(&self, account: &AccountId) -> Result<SavedDataClear, PortError> {
        let result = SavedDataClear {
            queue_removed: self.queue.clear(account).await?,
            bid_data_cleared: false,
        };
        self.record_clear(account, result.clone())?;
        Ok(result)
    }
}

#[derive(Clone, Debug, Default)]
pub struct RecordedBlacklistStore {
    records: Arc<Mutex<Vec<BlacklistRecord>>>,
}

impl RecordedBlacklistStore {
    pub fn records(&self) -> Vec<BlacklistRecord> {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[async_trait]
impl BlacklistStore for RecordedBlacklistStore {
    async fn apply(
        &self,
        account: &AccountId,
        request: BlacklistRequest,
    ) -> Result<BlacklistApplyResult, PortError> {
        let summary = match &request {
            BlacklistRequest::List => "listed blacklist rules".to_string(),
            BlacklistRequest::Update(update) => format!(
                "{:?} {:?}.{:?} {}",
                update.action, update.scope, update.field, update.value
            ),
        };
        let result = BlacklistApplyResult {
            request,
            changed: true,
            summary,
        };
        self.records
            .lock()
            .map_err(|_| PortError::Failed("blacklist lock poisoned".to_string()))?
            .push(BlacklistRecord {
                account: account.clone(),
                request: result.request.clone(),
                changed: result.changed,
                summary: result.summary.clone(),
            });
        Ok(result)
    }
}

#[derive(Clone, Debug, Default)]
pub struct RecordedStatsProvider {
    records: Arc<Mutex<Vec<StatsRequestRecord>>>,
}

impl RecordedStatsProvider {
    pub fn records(&self) -> Vec<StatsRequestRecord> {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn record(&self, account: &AccountId, kind: StatsRequestKind) -> Result<(), PortError> {
        self.records
            .lock()
            .map_err(|_| PortError::Failed("stats lock poisoned".to_string()))?
            .push(StatsRequestRecord {
                account: account.clone(),
                kind,
            });
        Ok(())
    }
}

#[async_trait]
impl AccountStatsProvider for RecordedStatsProvider {
    async fn stats(&self, account: &AccountId) -> Result<AccountStats, PortError> {
        self.record(account, StatsRequestKind::Stats)?;
        Ok(AccountStats::default())
    }

    async fn ping(&self, account: &AccountId) -> Result<AccountPing, PortError> {
        self.record(account, StatsRequestKind::Ping)?;
        Ok(AccountPing::default())
    }
}

#[derive(Clone, Debug, Default)]
pub struct RecordedAccountScheduler {
    records: Arc<Mutex<Vec<AccountScheduleRecord>>>,
}

impl RecordedAccountScheduler {
    pub fn records(&self) -> Vec<AccountScheduleRecord> {
        self.records
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[async_trait]
impl AccountScheduler for RecordedAccountScheduler {
    async fn schedule(
        &self,
        request: AccountScheduleRequest,
    ) -> Result<AccountScheduleResult, PortError> {
        self.records
            .lock()
            .map_err(|_| PortError::Failed("scheduler lock poisoned".to_string()))?
            .push(AccountScheduleRecord {
                request: request.clone(),
            });
        Ok(AccountScheduleResult {
            account: request.account,
            action: request.action,
            delay_ms: request.delay_ms,
        })
    }
}

#[derive(Clone, Debug, Default)]
pub struct RecordedAccountSupervisor {
    actions: Arc<Mutex<Vec<LifecycleRecord>>>,
}

impl RecordedAccountSupervisor {
    pub fn actions(&self) -> Vec<LifecycleRecord> {
        self.actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[async_trait]
impl AccountSupervisor for RecordedAccountSupervisor {
    async fn start(&self, account: &AccountId) -> Result<(), PortError> {
        self.actions
            .lock()
            .map_err(|_| PortError::Failed("lifecycle lock poisoned".to_string()))?
            .push(LifecycleRecord {
                action: LifecycleAction::Start,
                account: Some(account.clone()),
            });
        Ok(())
    }

    async fn stop(&self, account: Option<AccountId>) -> Result<(), PortError> {
        self.actions
            .lock()
            .map_err(|_| PortError::Failed("lifecycle lock poisoned".to_string()))?
            .push(LifecycleRecord {
                action: LifecycleAction::Stop,
                account,
            });
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct RecordedNotifier {
    notifications: Arc<Mutex<Vec<Notification>>>,
}

impl RecordedNotifier {
    pub fn notifications(&self) -> Vec<Notification> {
        self.notifications
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[async_trait]
impl Notifier for RecordedNotifier {
    async fn notify(&self, notification: Notification) -> Result<(), PortError> {
        self.notifications
            .lock()
            .map_err(|_| PortError::Failed("notification lock poisoned".to_string()))?
            .push(notification);
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct FileLogReader {
    path: PathBuf,
}

impl FileLogReader {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait]
impl LogReader for FileLogReader {
    async fn latest(&self, lines: usize) -> Result<LogSnapshot, PortError> {
        match tokio::fs::read_to_string(&self.path).await {
            Ok(raw) => {
                let mut entries = raw
                    .lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                let start = entries.len().saturating_sub(lines);
                entries.drain(0..start);
                Ok(LogSnapshot {
                    path: self.path.display().to_string(),
                    exists: true,
                    lines: entries,
                })
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(LogSnapshot {
                path: self.path.display().to_string(),
                exists: false,
                lines: Vec::new(),
            }),
            Err(error) => Err(PortError::Failed(error.to_string())),
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct TeeQueueStore {
    recorded: Arc<RecordedQueueStore>,
    file: Arc<FileQueueStore>,
}

impl TeeQueueStore {
    pub(super) fn new(recorded: Arc<RecordedQueueStore>, file: Arc<FileQueueStore>) -> Self {
        Self { recorded, file }
    }
}

#[async_trait]
impl QueueStore for TeeQueueStore {
    async fn add(
        &self,
        account: &AccountId,
        action: Value,
        state: BotState,
        priority: u8,
    ) -> Result<bool, PortError> {
        let recorded_changed = self
            .recorded
            .add(account, action.clone(), state.clone(), priority)
            .await?;
        let file_changed = self.file.add(account, action, state, priority).await?;
        Ok(recorded_changed || file_changed)
    }

    async fn snapshot(&self, account: &AccountId) -> Result<Vec<QueueEntry>, PortError> {
        self.file.snapshot(account).await
    }

    async fn clear(&self, account: &AccountId) -> Result<usize, PortError> {
        let recorded = self.recorded.clear(account).await?;
        let file = self.file.clear(account).await?;
        Ok(recorded.max(file))
    }

    async fn remove_at(
        &self,
        account: &AccountId,
        index: usize,
    ) -> Result<Option<QueueEntry>, PortError> {
        self.recorded.remove_at(account, index).await?;
        self.file.remove_at(account, index).await
    }
}

#[derive(Clone, Debug)]
pub(super) struct TeeSavedDataStore {
    recorded: Arc<RecordedSavedDataStore>,
    file: Arc<FileQueueStore>,
}

impl TeeSavedDataStore {
    pub(super) fn new(recorded: Arc<RecordedSavedDataStore>, file: Arc<FileQueueStore>) -> Self {
        Self { recorded, file }
    }
}

#[async_trait]
impl SavedDataStore for TeeSavedDataStore {
    async fn clear_saved_data(&self, account: &AccountId) -> Result<SavedDataClear, PortError> {
        let recorded_queue_removed = self.recorded.queue.clear(account).await?;
        let file = self.file.clear_saved_data(account).await?;
        let result = SavedDataClear {
            queue_removed: recorded_queue_removed.max(file.queue_removed),
            bid_data_cleared: file.bid_data_cleared,
        };
        self.recorded.record_clear(account, result.clone())?;
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn acct(name: &str) -> AccountId {
        AccountId::new(name).unwrap()
    }

    /// The recorded store must order a snapshot by priority exactly like the
    /// file store, so a `cancel_queue` index the UI took against the file
    /// snapshot removes the SAME entry from the recorded log.
    #[tokio::test]
    async fn recorded_store_orders_and_removes_by_priority_like_the_file_store() {
        let store = RecordedQueueStore::default();
        let a = acct("MainAccount");
        // Insert out of priority order (5, 3, 4).
        for (tag, prio) in [("p5", 5u8), ("p3", 3), ("p4", 4)] {
            store
                .add(&a, json!({ "tag": tag }), BotState::Buying, prio)
                .await
                .unwrap();
        }
        // Snapshot is priority-sorted (3, 4, 5), matching FileQueueStore.
        let snap = store.snapshot(&a).await.unwrap();
        assert_eq!(
            snap.iter().map(|e| e.priority).collect::<Vec<_>>(),
            vec![3, 4, 5]
        );
        // Removing index 1 removes the priority-4 entry the UI saw there.
        let removed = store.remove_at(&a, 1).await.unwrap().unwrap();
        assert_eq!(removed.priority, 4);
        let after = store.snapshot(&a).await.unwrap();
        assert_eq!(
            after.iter().map(|e| e.priority).collect::<Vec<_>>(),
            vec![3, 5]
        );
    }
}
