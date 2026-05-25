#[cfg(feature = "live-cofl")]
use super::super::cofl::LiveCoflClient;
use super::super::minecraft::ManagedMinecraftClient;
use async_trait::async_trait;
use saf_core::AccountId;
use saf_core::ports::{AccountSupervisor, PortError};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(in crate::live_runtime) struct LiveAccountSupervisor {
    configured: BTreeSet<AccountId>,
    running: Arc<Mutex<BTreeSet<AccountId>>>,
    minecraft: BTreeMap<AccountId, Arc<ManagedMinecraftClient>>,
    #[cfg(feature = "live-cofl")]
    cofl: BTreeMap<AccountId, Arc<LiveCoflClient>>,
}

impl LiveAccountSupervisor {
    #[cfg(not(feature = "live-cofl"))]
    pub(in crate::live_runtime) fn new(
        accounts: Vec<AccountId>,
        running: Vec<AccountId>,
        minecraft: BTreeMap<AccountId, Arc<ManagedMinecraftClient>>,
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
        }
    }

    #[cfg(feature = "live-cofl")]
    pub(in crate::live_runtime) fn new(
        accounts: Vec<AccountId>,
        running: Vec<AccountId>,
        minecraft: BTreeMap<AccountId, Arc<ManagedMinecraftClient>>,
        cofl: BTreeMap<AccountId, Arc<LiveCoflClient>>,
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
