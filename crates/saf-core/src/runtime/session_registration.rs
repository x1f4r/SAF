use super::*;

impl RuntimeSession {
    pub fn new(runtime: BotRuntime) -> Self {
        let running_accounts = Mutex::new(runtime.selector.running.clone());
        Self {
            runtime,
            running_accounts,
            minecraft_clients: BTreeMap::new(),
            cofl_clients: BTreeMap::new(),
            queue_stores: BTreeMap::new(),
            saved_data_stores: BTreeMap::new(),
            blacklist_stores: BTreeMap::new(),
            stats_providers: BTreeMap::new(),
            account_schedulers: BTreeMap::new(),
            inventory_providers: BTreeMap::new(),
            active_auction_providers: BTreeMap::new(),
            auction_metadata_provider: None,
            fallback_tracked_flip_provider: None,
            fallback_connection_provider: None,
            fallback_inventory_provider: None,
            fallback_active_auction_provider: None,
            fallback_gui_diagnostics_provider: None,
            log_reader: None,
            fallback_cofl: None,
            fallback_queue_store: None,
            fallback_saved_data_store: None,
            fallback_blacklist_store: None,
            fallback_stats_provider: None,
            fallback_account_scheduler: None,
            notifier: None,
            supervisor: None,
        }
    }

    pub fn runtime(&self) -> &BotRuntime {
        &self.runtime
    }

    pub fn blacklist_handle(&self) -> BlacklistPolicyHandle {
        self.runtime.blacklist_handle()
    }

    pub fn selector(&self) -> AccountSelector {
        let mut selector = self.runtime.selector.clone();
        selector.running = self.running_accounts();
        selector
    }

    pub fn running_accounts(&self) -> Vec<String> {
        self.running_accounts
            .lock()
            .map(|accounts| accounts.clone())
            .unwrap_or_else(|_| self.runtime.selector.running.clone())
    }

    pub fn add_minecraft_client(
        &mut self,
        account: AccountId,
        client: Arc<dyn MinecraftClient>,
    ) -> &mut Self {
        self.minecraft_clients.insert(account, client);
        self
    }

    pub fn add_cofl_client(
        &mut self,
        account: AccountId,
        client: Arc<dyn CoflClient>,
    ) -> &mut Self {
        self.cofl_clients.insert(account, client);
        self
    }

    pub fn set_fallback_cofl(&mut self, client: Arc<dyn CoflClient>) -> &mut Self {
        self.fallback_cofl = Some(client);
        self
    }

    pub fn add_queue_store(&mut self, account: AccountId, store: Arc<dyn QueueStore>) -> &mut Self {
        self.queue_stores.insert(account, store);
        self
    }

    pub fn set_fallback_queue_store(&mut self, store: Arc<dyn QueueStore>) -> &mut Self {
        self.fallback_queue_store = Some(store);
        self
    }

    pub fn add_saved_data_store(
        &mut self,
        account: AccountId,
        store: Arc<dyn SavedDataStore>,
    ) -> &mut Self {
        self.saved_data_stores.insert(account, store);
        self
    }

    pub fn set_fallback_saved_data_store(&mut self, store: Arc<dyn SavedDataStore>) -> &mut Self {
        self.fallback_saved_data_store = Some(store);
        self
    }

    pub fn add_blacklist_store(
        &mut self,
        account: AccountId,
        store: Arc<dyn BlacklistStore>,
    ) -> &mut Self {
        self.blacklist_stores.insert(account, store);
        self
    }

    pub fn set_fallback_blacklist_store(&mut self, store: Arc<dyn BlacklistStore>) -> &mut Self {
        self.fallback_blacklist_store = Some(store);
        self
    }

    pub fn add_stats_provider(
        &mut self,
        account: AccountId,
        provider: Arc<dyn AccountStatsProvider>,
    ) -> &mut Self {
        self.stats_providers.insert(account, provider);
        self
    }

    pub fn set_fallback_stats_provider(
        &mut self,
        provider: Arc<dyn AccountStatsProvider>,
    ) -> &mut Self {
        self.fallback_stats_provider = Some(provider);
        self
    }

    pub fn add_account_scheduler(
        &mut self,
        account: AccountId,
        scheduler: Arc<dyn AccountScheduler>,
    ) -> &mut Self {
        self.account_schedulers.insert(account, scheduler);
        self
    }

    pub fn set_fallback_account_scheduler(
        &mut self,
        scheduler: Arc<dyn AccountScheduler>,
    ) -> &mut Self {
        self.fallback_account_scheduler = Some(scheduler);
        self
    }

    pub fn set_fallback_connection_provider(
        &mut self,
        provider: Arc<dyn AccountConnectionProvider>,
    ) -> &mut Self {
        self.fallback_connection_provider = Some(provider);
        self
    }

    pub fn set_log_reader(&mut self, reader: Arc<dyn LogReader>) -> &mut Self {
        self.log_reader = Some(reader);
        self
    }

    pub fn add_inventory_provider(
        &mut self,
        account: AccountId,
        provider: Arc<dyn InventoryProvider>,
    ) -> &mut Self {
        self.inventory_providers.insert(account, provider);
        self
    }

    pub fn set_fallback_inventory_provider(
        &mut self,
        provider: Arc<dyn InventoryProvider>,
    ) -> &mut Self {
        self.fallback_inventory_provider = Some(provider);
        self
    }

    pub fn add_active_auction_provider(
        &mut self,
        account: AccountId,
        provider: Arc<dyn ActiveAuctionProvider>,
    ) -> &mut Self {
        self.active_auction_providers.insert(account, provider);
        self
    }

    pub fn set_fallback_active_auction_provider(
        &mut self,
        provider: Arc<dyn ActiveAuctionProvider>,
    ) -> &mut Self {
        self.fallback_active_auction_provider = Some(provider);
        self
    }

    pub fn set_fallback_gui_diagnostics_provider(
        &mut self,
        provider: Arc<dyn GuiDiagnosticsProvider>,
    ) -> &mut Self {
        self.fallback_gui_diagnostics_provider = Some(provider);
        self
    }

    pub fn set_auction_metadata_provider(
        &mut self,
        provider: Arc<dyn AuctionMetadataProvider>,
    ) -> &mut Self {
        self.auction_metadata_provider = Some(provider);
        self
    }

    pub fn set_fallback_tracked_flip_provider(
        &mut self,
        provider: Arc<dyn TrackedFlipProvider>,
    ) -> &mut Self {
        self.fallback_tracked_flip_provider = Some(provider);
        self
    }

    pub fn set_notifier(&mut self, notifier: Arc<dyn Notifier>) -> &mut Self {
        self.notifier = Some(notifier);
        self
    }

    pub fn set_account_supervisor(&mut self, supervisor: Arc<dyn AccountSupervisor>) -> &mut Self {
        self.supervisor = Some(supervisor);
        self
    }
}
