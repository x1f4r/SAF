use super::process_inbox;
use super::records::{
    AccountScheduleRecord, BlacklistRecord, CoflCommandRecord, InboxProcessingReport,
    MinecraftActionRecord, QueueRecord, SavedDataClearRecord, StatsRequestRecord,
};
use super::stores::{
    FileLogReader, RecordedAccountScheduler, RecordedAccountSupervisor, RecordedBlacklistStore,
    RecordedNotifier, RecordedQueueStore, RecordedSavedDataStore, RecordedStatsProvider,
    TeeQueueStore, TeeSavedDataStore,
};
use crate::state_cli::FileQueueStore;
use saf_cofl::RecordedCoflClient;
use saf_core::ports::{
    AccountConnectionProvider, ActiveAuctionProvider, AuctionMetadataProvider, InventoryProvider,
    LogReader, Notification, TrackedFlipProvider,
};
use saf_core::{AccountId, BotRuntime, RuntimeSession, SafConfig};
use saf_minecraft::RecordedMinecraftClient;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

pub struct RecordedRuntimeSession {
    runtime: RuntimeSession,
    cofl: Arc<RecordedCoflClient>,
    queue: Arc<RecordedQueueStore>,
    saved_data: Arc<RecordedSavedDataStore>,
    blacklist: Arc<RecordedBlacklistStore>,
    stats: Arc<RecordedStatsProvider>,
    scheduler: Arc<RecordedAccountScheduler>,
    supervisor: Arc<RecordedAccountSupervisor>,
    notifier: Arc<RecordedNotifier>,
    minecraft: BTreeMap<AccountId, Arc<RecordedMinecraftClient>>,
}

impl RecordedRuntimeSession {
    pub fn from_config(config: &SafConfig, running: Vec<String>) -> Self {
        Self::from_config_with_queue(config, running, None)
    }

    pub fn from_config_with_file_queue(
        config: &SafConfig,
        running: Vec<String>,
        base_dir: impl Into<PathBuf>,
    ) -> Self {
        Self::from_config_with_queue(config, running, Some(FileQueueStore::new(base_dir)))
    }

    fn from_config_with_queue(
        config: &SafConfig,
        running: Vec<String>,
        file_queue: Option<FileQueueStore>,
    ) -> Self {
        let account_ids = runtime_accounts(config, running);
        let runtime = BotRuntime::from_config(
            config,
            account_ids
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        );
        let mut session = RuntimeSession::new(runtime);
        let minecraft = account_ids
            .iter()
            .map(|account| {
                let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
                session.add_minecraft_client(account.clone(), client.clone());
                (account.clone(), client)
            })
            .collect::<BTreeMap<_, _>>();
        let cofl = Arc::new(RecordedCoflClient::default());
        let queue = Arc::new(RecordedQueueStore::default());
        let saved_data = Arc::new(RecordedSavedDataStore::new(queue.clone()));
        let blacklist = Arc::new(RecordedBlacklistStore::default());
        let stats = Arc::new(RecordedStatsProvider::default());
        let scheduler = Arc::new(RecordedAccountScheduler::default());
        let supervisor = Arc::new(RecordedAccountSupervisor::default());
        let notifier = Arc::new(RecordedNotifier::default());
        session.set_fallback_cofl(cofl.clone());
        session.set_log_reader(Arc::new(FileLogReader::new("logs/latest.log")));
        if let Some(file_queue) = file_queue {
            let file_queue = Arc::new(file_queue);
            session.set_fallback_queue_store(Arc::new(TeeQueueStore::new(
                queue.clone(),
                file_queue.clone(),
            )));
            session.set_fallback_saved_data_store(Arc::new(TeeSavedDataStore::new(
                saved_data.clone(),
                file_queue,
            )));
        } else {
            session.set_fallback_queue_store(queue.clone());
            session.set_fallback_saved_data_store(saved_data.clone());
        }
        session.set_account_supervisor(supervisor.clone());
        session.set_notifier(notifier.clone());
        session.set_fallback_blacklist_store(blacklist.clone());
        session.set_fallback_stats_provider(stats.clone());
        session.set_fallback_account_scheduler(scheduler.clone());

        Self {
            runtime: session,
            cofl,
            queue,
            saved_data,
            blacklist,
            stats,
            scheduler,
            supervisor,
            notifier,
            minecraft,
        }
    }

    pub async fn process_inbox_report(&self, raw: &str) -> InboxProcessingReport {
        InboxProcessingReport {
            results: process_inbox(&self.runtime, raw).await,
            cofl_commands: self.cofl_commands(),
            queue_entries: self.queue_entries(),
            saved_data_clears: self.saved_data_clears(),
            blacklist_requests: self.blacklist_requests(),
            stats_requests: self.stats_requests(),
            scheduled_accounts: self.scheduled_accounts(),
            lifecycle_actions: self.lifecycle_actions(),
            notifications: self.notifications(),
            minecraft_actions: self.minecraft_actions(),
        }
    }

    pub fn runtime(&self) -> &RuntimeSession {
        &self.runtime
    }

    pub fn set_auction_metadata_provider(
        &mut self,
        provider: Arc<dyn AuctionMetadataProvider>,
    ) -> &mut Self {
        self.runtime.set_auction_metadata_provider(provider);
        self
    }

    pub fn set_tracked_flip_provider(
        &mut self,
        provider: Arc<dyn TrackedFlipProvider>,
    ) -> &mut Self {
        self.runtime.set_fallback_tracked_flip_provider(provider);
        self
    }

    pub fn set_connection_provider(
        &mut self,
        provider: Arc<dyn AccountConnectionProvider>,
    ) -> &mut Self {
        self.runtime.set_fallback_connection_provider(provider);
        self
    }

    pub fn set_log_reader(&mut self, reader: Arc<dyn LogReader>) -> &mut Self {
        self.runtime.set_log_reader(reader);
        self
    }

    pub fn set_inventory_provider(&mut self, provider: Arc<dyn InventoryProvider>) -> &mut Self {
        self.runtime.set_fallback_inventory_provider(provider);
        self
    }

    pub fn set_active_auction_provider(
        &mut self,
        provider: Arc<dyn ActiveAuctionProvider>,
    ) -> &mut Self {
        self.runtime.set_fallback_active_auction_provider(provider);
        self
    }

    pub fn cofl_commands(&self) -> Vec<CoflCommandRecord> {
        self.cofl
            .commands()
            .into_iter()
            .map(|(account, command)| CoflCommandRecord { account, command })
            .collect()
    }

    pub fn minecraft_actions(&self) -> Vec<MinecraftActionRecord> {
        self.minecraft
            .iter()
            .map(|(account, client)| MinecraftActionRecord {
                account: account.clone(),
                actions: client.actions(),
            })
            .collect()
    }

    pub fn queue_entries(&self) -> Vec<QueueRecord> {
        self.queue.entries()
    }

    pub fn saved_data_clears(&self) -> Vec<SavedDataClearRecord> {
        self.saved_data.records()
    }

    pub fn blacklist_requests(&self) -> Vec<BlacklistRecord> {
        self.blacklist.records()
    }

    pub fn stats_requests(&self) -> Vec<StatsRequestRecord> {
        self.stats.records()
    }

    pub fn scheduled_accounts(&self) -> Vec<AccountScheduleRecord> {
        self.scheduler.records()
    }

    pub fn lifecycle_actions(&self) -> Vec<super::records::LifecycleRecord> {
        self.supervisor.actions()
    }

    pub fn notifications(&self) -> Vec<Notification> {
        self.notifier.notifications()
    }
}

fn runtime_accounts(config: &SafConfig, running: Vec<String>) -> Vec<AccountId> {
    let accounts = if running.is_empty() {
        config.startup_igns()
    } else {
        running
    };
    accounts.into_iter().filter_map(AccountId::new).collect()
}
