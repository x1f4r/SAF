use super::super::market_queue::is_market_minecraft_action;
use super::super::{MarketActionMode, native_minecraft_enabled};
use super::inventory_pricing::InventoryPriceLookup;
use super::native::connect_minecraft_client;
use anyhow::Result;
use async_trait::async_trait;
use saf_core::ports::{
    InventoryProvider, InventorySnapshot, MinecraftAction, MinecraftClient, MinecraftEvent,
    PortError,
};
use saf_core::{AccountId, RuntimeSession};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as AsyncMutex;

const MAX_RECONNECT_DELAY: Duration = Duration::from_secs(120);
const MAX_RECONNECT_FAILURE_SHIFT: u32 = 10;
const MAX_RECONNECT_DELAY_ENV: &str = "SAF_MINECRAFT_MAX_RECONNECT_DELAY_MS";

#[derive(Clone)]
pub(in crate::live_runtime) struct ManagedMinecraftClients {
    pub(in crate::live_runtime) clients: BTreeMap<AccountId, Arc<dyn MinecraftClient>>,
    pub(in crate::live_runtime) handles: BTreeMap<AccountId, Arc<ManagedMinecraftClient>>,
}

pub(in crate::live_runtime) async fn add_minecraft_clients(
    session: &mut RuntimeSession,
    accounts: &[AccountId],
    startup_accounts: &[AccountId],
    mode: MarketActionMode,
    price_lookup: Option<Arc<dyn InventoryPriceLookup>>,
) -> Result<ManagedMinecraftClients> {
    let mut clients = BTreeMap::new();
    let mut handles = BTreeMap::new();
    let startup_accounts = startup_accounts.iter().collect::<BTreeSet<_>>();
    for account in accounts {
        let managed = if startup_accounts.contains(account) {
            Arc::new(
                ManagedMinecraftClient::connect(account.clone(), mode, price_lookup.clone())
                    .await?,
            )
        } else {
            Arc::new(ManagedMinecraftClient::stopped(
                account.clone(),
                mode,
                price_lookup.clone(),
            ))
        };
        if native_minecraft_enabled() || managed.has_inventory_provider().await {
            session.add_inventory_provider(account.clone(), managed.clone());
        }
        let client: Arc<dyn MinecraftClient> = managed.clone();
        session.add_minecraft_client(account.clone(), client.clone());
        clients.insert(account.clone(), client);
        handles.insert(account.clone(), managed);
    }
    Ok(ManagedMinecraftClients { clients, handles })
}

pub(in crate::live_runtime) struct LiveMinecraftClientBundle {
    pub(in crate::live_runtime) minecraft: Arc<dyn MinecraftClient>,
    pub(in crate::live_runtime) inventory_provider: Option<Arc<dyn InventoryProvider>>,
}

#[derive(Clone)]
pub(in crate::live_runtime) struct ManagedMinecraftClient {
    pub(in crate::live_runtime) account: AccountId,
    pub(in crate::live_runtime) mode: MarketActionMode,
    pub(in crate::live_runtime) inner: Arc<AsyncMutex<Option<LiveMinecraftClientBundle>>>,
    pub(in crate::live_runtime) reconnect_at: Arc<AsyncMutex<Option<Instant>>>,
    pub(in crate::live_runtime) reconnect_delay: Duration,
    pub(in crate::live_runtime) reconnect_failures: Arc<AsyncMutex<u32>>,
    pub(in crate::live_runtime) price_lookup: Option<Arc<dyn InventoryPriceLookup>>,
    /// Set when the account was intentionally stopped by an operator (per-account
    /// stop, Stop All, or shutdown). A stopped client refuses to reconnect on
    /// `perform`/`next_event`, so an in-flight task (e.g. auto-cookie) can never
    /// silently bring a stopped account back online. Cleared by `force_connect`.
    pub(in crate::live_runtime) stopped: Arc<AtomicBool>,
}

impl ManagedMinecraftClient {
    pub(in crate::live_runtime) async fn connect(
        account: AccountId,
        mode: MarketActionMode,
        price_lookup: Option<Arc<dyn InventoryPriceLookup>>,
    ) -> Result<Self> {
        tracing::info!(account = %account, "connecting Minecraft account");
        let started_at = Instant::now();
        let bundle = connect_minecraft_client(&account).await?;
        tracing::info!(
            account = %account,
            elapsed_ms = started_at.elapsed().as_millis(),
            "connected Minecraft account"
        );
        Ok(Self {
            account,
            mode,
            inner: Arc::new(AsyncMutex::new(Some(bundle))),
            reconnect_at: Arc::new(AsyncMutex::new(None)),
            reconnect_delay: Duration::from_secs(5),
            reconnect_failures: Arc::new(AsyncMutex::new(0)),
            price_lookup,
            stopped: Arc::new(AtomicBool::new(false)),
        })
    }

    pub(in crate::live_runtime) fn stopped(
        account: AccountId,
        mode: MarketActionMode,
        price_lookup: Option<Arc<dyn InventoryPriceLookup>>,
    ) -> Self {
        Self {
            account,
            mode,
            inner: Arc::new(AsyncMutex::new(None)),
            reconnect_at: Arc::new(AsyncMutex::new(None)),
            reconnect_delay: Duration::from_secs(5),
            reconnect_failures: Arc::new(AsyncMutex::new(0)),
            price_lookup,
            stopped: Arc::new(AtomicBool::new(true)),
        }
    }

    #[cfg(test)]
    pub(in crate::live_runtime) fn from_bundle(
        account: AccountId,
        mode: MarketActionMode,
        bundle: LiveMinecraftClientBundle,
        price_lookup: Option<Arc<dyn InventoryPriceLookup>>,
    ) -> Self {
        Self {
            account,
            mode,
            inner: Arc::new(AsyncMutex::new(Some(bundle))),
            reconnect_at: Arc::new(AsyncMutex::new(None)),
            reconnect_delay: Duration::from_secs(5),
            reconnect_failures: Arc::new(AsyncMutex::new(0)),
            price_lookup,
            stopped: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(in crate::live_runtime) async fn connect_if_needed(&self) -> Result<(), PortError> {
        self.connect_if_needed_with_policy(false).await
    }

    pub(in crate::live_runtime) async fn force_connect(&self) -> Result<(), PortError> {
        self.stopped.store(false, Ordering::SeqCst);
        *self.reconnect_at.lock().await = None;
        self.reset_reconnect_backoff().await;
        self.connect_if_needed_with_policy(true).await
    }

    async fn connect_if_needed_with_policy(&self, force: bool) -> Result<(), PortError> {
        {
            let inner = self.inner.lock().await;
            if inner.is_some() {
                return Ok(());
            }
        }
        if !force && self.stopped.load(Ordering::SeqCst) {
            return Err(PortError::Unavailable(format!(
                "minecraft client for {} was intentionally stopped",
                self.account
            )));
        }
        if !force {
            let now = Instant::now();
            let reconnect_at = self.reconnect_at.lock().await;
            if let Some(at) = *reconnect_at
                && at > now
            {
                return Err(PortError::Unavailable(format!(
                    "minecraft reconnect for {} is scheduled in {}ms",
                    self.account,
                    at.saturating_duration_since(now).as_millis()
                )));
            }
        }
        tracing::info!(account = %self.account, "connecting stopped Minecraft account");
        let started_at = Instant::now();
        let bundle = connect_minecraft_client(&self.account)
            .await
            .map_err(|error| PortError::Failed(error.to_string()))?;
        let mut inner = self.inner.lock().await;
        if inner.is_none() {
            *inner = Some(bundle);
        }
        *self.reconnect_at.lock().await = None;
        tracing::info!(
            account = %self.account,
            elapsed_ms = started_at.elapsed().as_millis(),
            "connected stopped Minecraft account"
        );
        Ok(())
    }

    pub(in crate::live_runtime) async fn disconnect(&self) -> Result<(), PortError> {
        self.stopped.store(true, Ordering::SeqCst);
        let bundle = self.inner.lock().await.take();
        *self.reconnect_at.lock().await = None;
        self.reset_reconnect_backoff().await;
        if let Some(bundle) = bundle {
            bundle
                .minecraft
                .perform(MinecraftAction::Disconnect)
                .await?;
        }
        Ok(())
    }

    pub(in crate::live_runtime) async fn mark_runtime_disconnected(&self) {
        self.inner.lock().await.take();
        let delay = self.next_reconnect_delay().await;
        *self.reconnect_at.lock().await = Some(Instant::now() + delay);
        tracing::info!(
            account = %self.account,
            reconnect_in_ms = delay.as_millis(),
            "scheduled Minecraft reconnect"
        );
    }

    pub(in crate::live_runtime) async fn has_active_runtime(&self) -> bool {
        self.inner.lock().await.is_some()
    }

    async fn reconnect_for_poll(&self) -> Result<bool, PortError> {
        {
            let inner = self.inner.lock().await;
            if inner.is_some() {
                return Ok(true);
            }
        }
        if self.stopped.load(Ordering::SeqCst) {
            return Ok(false);
        }
        let now = Instant::now();
        {
            let reconnect_at = self.reconnect_at.lock().await;
            if reconnect_at.is_some_and(|at| at > now) {
                return Ok(false);
            }
        }

        match connect_minecraft_client(&self.account).await {
            Ok(bundle) => {
                let mut inner = self.inner.lock().await;
                if inner.is_none() {
                    *inner = Some(bundle);
                }
                *self.reconnect_at.lock().await = None;
                tracing::info!(account = %self.account, "reconnected Minecraft account");
                Ok(true)
            }
            Err(error) => {
                let delay = self.next_reconnect_delay().await;
                *self.reconnect_at.lock().await = Some(now + delay);
                tracing::warn!(
                    account = %self.account,
                    error = %error,
                    reconnect_in_ms = delay.as_millis(),
                    "Minecraft reconnect failed; retry scheduled"
                );
                Ok(false)
            }
        }
    }

    async fn current_minecraft(&self) -> Result<Arc<dyn MinecraftClient>, PortError> {
        self.connect_if_needed().await?;
        self.inner
            .lock()
            .await
            .as_ref()
            .map(|bundle| bundle.minecraft.clone())
            .ok_or_else(|| {
                PortError::Unavailable(format!("minecraft client for {} is stopped", self.account))
            })
    }

    async fn current_inventory_provider(&self) -> Result<Arc<dyn InventoryProvider>, PortError> {
        self.connect_if_needed().await?;
        self.inner
            .lock()
            .await
            .as_ref()
            .and_then(|bundle| bundle.inventory_provider.clone())
            .ok_or_else(|| {
                PortError::Unavailable(format!(
                    "inventory provider is not available for {}",
                    self.account
                ))
            })
    }

    async fn enrich_inventory_prices(
        &self,
        snapshot: &mut InventorySnapshot,
    ) -> Result<(), PortError> {
        let Some(price_lookup) = &self.price_lookup else {
            return Ok(());
        };
        for item in &mut snapshot.items {
            if item.price.is_none() {
                match price_lookup.lookup(item).await {
                    Ok(price) => item.price = price,
                    Err(error) => {
                        tracing::warn!(
                            account = %self.account,
                            item = %item.item_name,
                            error = %error,
                            "inventory price lookup failed; leaving price unknown"
                        );
                    }
                }
            }
        }
        Ok(())
    }

    async fn has_inventory_provider(&self) -> bool {
        self.inner
            .lock()
            .await
            .as_ref()
            .and_then(|bundle| bundle.inventory_provider.as_ref())
            .is_some()
    }

    async fn next_reconnect_delay(&self) -> Duration {
        let mut failures = self.reconnect_failures.lock().await;
        let multiplier = 1_u32 << (*failures).min(MAX_RECONNECT_FAILURE_SHIFT);
        *failures = failures.saturating_add(1);
        self.reconnect_delay
            .saturating_mul(multiplier)
            .min(configured_max_reconnect_delay())
    }

    async fn reset_reconnect_backoff(&self) {
        *self.reconnect_failures.lock().await = 0;
    }

    #[cfg(test)]
    pub(in crate::live_runtime) async fn is_connected(&self) -> bool {
        self.inner.lock().await.is_some()
    }

    #[cfg(all(test, feature = "live-cofl"))]
    pub(in crate::live_runtime) async fn replace_minecraft_for_test(
        &self,
        minecraft: Arc<dyn MinecraftClient>,
    ) {
        *self.inner.lock().await = Some(LiveMinecraftClientBundle {
            minecraft,
            inventory_provider: None,
        });
        *self.reconnect_at.lock().await = None;
        self.reset_reconnect_backoff().await;
    }
}

fn configured_max_reconnect_delay() -> Duration {
    std::env::var(MAX_RECONNECT_DELAY_ENV)
        .ok()
        .and_then(|value| parse_reconnect_delay_ms(&value))
        .unwrap_or(MAX_RECONNECT_DELAY)
}

fn parse_reconnect_delay_ms(value: &str) -> Option<Duration> {
    let millis = value.trim().parse::<u64>().ok()?;
    if millis == 0 {
        return None;
    }
    Some(Duration::from_millis(millis))
}

#[async_trait]
impl MinecraftClient for ManagedMinecraftClient {
    async fn account(&self) -> AccountId {
        self.account.clone()
    }

    async fn perform(&self, action: MinecraftAction) -> Result<(), PortError> {
        if !self.mode.allows_market_actions() && is_market_minecraft_action(&action) {
            return Ok(());
        }
        self.current_minecraft().await?.perform(action).await
    }

    async fn next_event(&self) -> Result<Option<MinecraftEvent>, PortError> {
        if !self.reconnect_for_poll().await? {
            return Ok(None);
        }
        let client = self
            .inner
            .lock()
            .await
            .as_ref()
            .map(|bundle| bundle.minecraft.clone());
        match client {
            Some(client) => {
                let event = client.next_event().await?;
                if matches!(
                    event,
                    Some(MinecraftEvent::Kicked { .. } | MinecraftEvent::Disconnected { .. })
                ) {
                    self.mark_runtime_disconnected().await;
                } else if event.is_some() {
                    self.reset_reconnect_backoff().await;
                }
                Ok(event)
            }
            None => Ok(None),
        }
    }
}

#[async_trait]
impl InventoryProvider for ManagedMinecraftClient {
    async fn snapshot(&self, account: &AccountId) -> Result<InventorySnapshot, PortError> {
        if account != &self.account {
            return Err(PortError::Unavailable(format!(
                "minecraft client is configured for {}, not {account}",
                self.account
            )));
        }
        let mut snapshot = self
            .current_inventory_provider()
            .await?
            .snapshot(account)
            .await?;
        self.enrich_inventory_prices(&mut snapshot).await?;
        Ok(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    struct RecordingClient {
        account: AccountId,
        performed: AtomicUsize,
    }

    #[async_trait]
    impl MinecraftClient for RecordingClient {
        async fn account(&self) -> AccountId {
            self.account.clone()
        }

        async fn perform(&self, action: MinecraftAction) -> Result<(), PortError> {
            if !matches!(action, MinecraftAction::Disconnect) {
                self.performed.fetch_add(1, Ordering::SeqCst);
            }
            Ok(())
        }

        async fn next_event(&self) -> Result<Option<MinecraftEvent>, PortError> {
            Ok(None)
        }
    }

    #[tokio::test]
    async fn stopped_client_does_not_reconnect_on_perform() {
        let account = AccountId::new("Main").expect("account");
        let mock = Arc::new(RecordingClient {
            account: account.clone(),
            performed: AtomicUsize::new(0),
        });
        let managed = ManagedMinecraftClient::from_bundle(
            account.clone(),
            MarketActionMode::Live,
            LiveMinecraftClientBundle {
                minecraft: mock.clone(),
                inventory_provider: None,
            },
            None,
        );

        // While connected, a non-market action is forwarded to the live client.
        managed
            .perform(MinecraftAction::Chat("hi".to_string()))
            .await
            .expect("connected perform");
        assert_eq!(mock.performed.load(Ordering::SeqCst), 1);
        assert!(managed.has_active_runtime().await);

        // Operator stop disconnects and arms the inert guard.
        managed.disconnect().await.expect("disconnect");
        assert!(!managed.has_active_runtime().await);

        // An in-flight task (e.g. auto-cookie) must NOT silently reconnect the
        // stopped account: perform fails fast instead of dialing Hypixel again.
        let result = managed
            .perform(MinecraftAction::Chat("again".to_string()))
            .await;
        assert!(result.is_err(), "stopped client must refuse to act");
        assert_eq!(
            mock.performed.load(Ordering::SeqCst),
            1,
            "no further actions are forwarded after a stop"
        );
        assert!(
            !managed.has_active_runtime().await,
            "stopped client stayed disconnected"
        );

        // Clearing the flag (as force_connect does on an explicit start) lifts the guard.
        managed.stopped.store(false, Ordering::SeqCst);
        assert!(!managed.stopped.load(Ordering::SeqCst));
    }

    #[test]
    fn reconnect_delay_env_parser_rejects_empty_zero_and_invalid_values() {
        assert_eq!(
            parse_reconnect_delay_ms("900000"),
            Some(Duration::from_secs(900))
        );
        assert_eq!(
            parse_reconnect_delay_ms(" 15000 "),
            Some(Duration::from_secs(15))
        );
        assert_eq!(parse_reconnect_delay_ms("0"), None);
        assert_eq!(parse_reconnect_delay_ms(""), None);
        assert_eq!(parse_reconnect_delay_ms("later"), None);
    }
}
