#[cfg(feature = "live-cofl")]
use super::super::cofl::LiveCoflClient;
use super::super::minecraft::ManagedMinecraftClient;
use async_trait::async_trait;
use saf_core::AccountId;
use saf_core::ports::{AccountSupervisor, PortError};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(in crate::live_runtime) struct LiveAccountSupervisor {
    configured: BTreeSet<AccountId>,
    running: Arc<Mutex<BTreeSet<AccountId>>>,
    minecraft: BTreeMap<AccountId, Arc<ManagedMinecraftClient>>,
    #[cfg(feature = "live-cofl")]
    cofl: BTreeMap<AccountId, Arc<LiveCoflClient>>,
    /// Shared "panic stop" flag. A Stop All (`stop(None)`) arms it; any explicit
    /// start disarms it. The poll loop and background tasks read the same flag.
    halted: Arc<AtomicBool>,
}

impl LiveAccountSupervisor {
    #[cfg(not(feature = "live-cofl"))]
    pub(in crate::live_runtime) fn new(
        accounts: Vec<AccountId>,
        running: Vec<AccountId>,
        minecraft: BTreeMap<AccountId, Arc<ManagedMinecraftClient>>,
        halted: Arc<AtomicBool>,
    ) -> Self {
        let configured = accounts.into_iter().collect::<BTreeSet<_>>();
        let running = running
            .into_iter()
            .filter(|account| configured.contains(account))
            .collect();
        Self {
            running: Arc::new(Mutex::new(running)),
            configured,
            minecraft,
            halted,
        }
    }

    #[cfg(feature = "live-cofl")]
    pub(in crate::live_runtime) fn new(
        accounts: Vec<AccountId>,
        running: Vec<AccountId>,
        minecraft: BTreeMap<AccountId, Arc<ManagedMinecraftClient>>,
        cofl: BTreeMap<AccountId, Arc<LiveCoflClient>>,
        halted: Arc<AtomicBool>,
    ) -> Self {
        let configured = accounts.into_iter().collect::<BTreeSet<_>>();
        let running = running
            .into_iter()
            .filter(|account| configured.contains(account))
            .collect();
        Self {
            running: Arc::new(Mutex::new(running)),
            configured,
            minecraft,
            cofl,
            halted,
        }
    }
}

#[async_trait]
impl AccountSupervisor for LiveAccountSupervisor {
    async fn start(&self, account: &AccountId) -> Result<(), PortError> {
        if !self.configured.contains(account) {
            return Err(PortError::Unavailable(format!(
                "{account} is not configured for this runtime"
            )));
        }
        // An explicit operator start re-arms the runtime after a panic stop.
        self.halted.store(false, Ordering::SeqCst);
        tracing::info!(account = %account, "starting account supervisor target");
        if let Some(client) = self.minecraft.get(account) {
            client.force_connect().await?;
        }
        #[cfg(feature = "live-cofl")]
        if let Some(client) = self.cofl.get(account) {
            client.resume_after_stop().await;
        }
        self.running
            .lock()
            .map_err(|_| PortError::Failed("account supervisor lock poisoned".to_string()))?
            .insert(account.clone());
        tracing::info!(account = %account, "account supervisor target is running");
        Ok(())
    }

    async fn stop(&self, account: Option<AccountId>) -> Result<(), PortError> {
        // Stop All is the operator panic stop: latch the halt so no background
        // task (auto-rotate, scheduler, deferred drains) can restart accounts
        // until an explicit start. A single-account stop leaves others running.
        if account.is_none() {
            self.halted.store(true, Ordering::SeqCst);
            tracing::warn!(
                "Stop All engaged: halting all flipping and account restarts until an explicit start"
            );
        }
        let targets = if let Some(account) = account {
            vec![account]
        } else {
            self.configured.iter().cloned().collect::<Vec<_>>()
        };
        {
            let mut running = self
                .running
                .lock()
                .map_err(|_| PortError::Failed("account supervisor lock poisoned".to_string()))?;
            for account in &targets {
                running.remove(account);
            }
        }
        for account in targets {
            if let Some(client) = self.minecraft.get(&account)
                && let Err(error) = client.disconnect().await
            {
                tracing::warn!(account = %account, error = %error, "failed to disconnect Minecraft client during stop");
            }
            #[cfg(feature = "live-cofl")]
            if let Some(client) = self.cofl.get(&account) {
                client.disconnect_for_stop().await;
            }
        }
        Ok(())
    }
}
