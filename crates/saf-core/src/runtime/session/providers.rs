use super::*;

impl RuntimeSession {
    pub(super) fn minecraft_client(
        &self,
        account: &AccountId,
    ) -> Result<&Arc<dyn MinecraftClient>, RuntimeError> {
        self.minecraft_clients.get(account).ok_or_else(|| {
            RuntimeError::Port(format!("No Minecraft client registered for {account}."))
        })
    }

    pub(super) fn cofl_client(&self, account: &AccountId) -> Option<&Arc<dyn CoflClient>> {
        self.cofl_clients
            .get(account)
            .or(self.fallback_cofl.as_ref())
    }

    pub(super) fn queue_store(
        &self,
        account: &AccountId,
    ) -> Result<&Arc<dyn QueueStore>, RuntimeError> {
        self.queue_stores
            .get(account)
            .or(self.fallback_queue_store.as_ref())
            .ok_or_else(|| RuntimeError::Port(format!("No queue store registered for {account}.")))
    }

    pub(super) fn saved_data_store(
        &self,
        account: &AccountId,
    ) -> Result<&Arc<dyn SavedDataStore>, RuntimeError> {
        self.saved_data_stores
            .get(account)
            .or(self.fallback_saved_data_store.as_ref())
            .ok_or_else(|| {
                RuntimeError::Port(format!("No saved data store registered for {account}."))
            })
    }

    pub(super) fn blacklist_store(&self, account: &AccountId) -> Option<&Arc<dyn BlacklistStore>> {
        self.blacklist_stores
            .get(account)
            .or(self.fallback_blacklist_store.as_ref())
    }

    pub(super) fn stats_provider(
        &self,
        account: &AccountId,
    ) -> Option<&Arc<dyn AccountStatsProvider>> {
        self.stats_providers
            .get(account)
            .or(self.fallback_stats_provider.as_ref())
    }

    pub(super) fn account_scheduler(
        &self,
        account: &AccountId,
    ) -> Option<&Arc<dyn AccountScheduler>> {
        self.account_schedulers
            .get(account)
            .or(self.fallback_account_scheduler.as_ref())
    }

    pub(super) fn tracked_flip_provider(
        &self,
        _account: &AccountId,
    ) -> Option<&Arc<dyn TrackedFlipProvider>> {
        self.fallback_tracked_flip_provider.as_ref()
    }

    pub(super) fn connection_provider(
        &self,
        _account: &AccountId,
    ) -> Option<&Arc<dyn AccountConnectionProvider>> {
        self.fallback_connection_provider.as_ref()
    }

    pub(super) fn inventory_provider(
        &self,
        account: &AccountId,
    ) -> Option<&Arc<dyn InventoryProvider>> {
        self.inventory_providers
            .get(account)
            .or(self.fallback_inventory_provider.as_ref())
    }

    pub(super) fn active_auction_provider(
        &self,
        account: &AccountId,
    ) -> Option<&Arc<dyn ActiveAuctionProvider>> {
        self.active_auction_providers
            .get(account)
            .or(self.fallback_active_auction_provider.as_ref())
    }

    pub(super) fn gui_diagnostics_provider(
        &self,
        _account: &AccountId,
    ) -> Option<&Arc<dyn GuiDiagnosticsProvider>> {
        self.fallback_gui_diagnostics_provider.as_ref()
    }

    pub(super) async fn request_ping_samples(&self, account: &AccountId) {
        if let Some(cofl) = self
            .cofl_clients
            .get(account)
            .or(self.fallback_cofl.as_ref())
        {
            let _ = cofl.send_command(account, "/cofl ping").await;
            let _ = cofl.send_command(account, "/cofl delay").await;
        }
        if let Some(minecraft) = self.minecraft_clients.get(account) {
            let _ = minecraft
                .perform(MinecraftAction::Chat("/social pingwars".to_string()))
                .await;
        }
    }
}
