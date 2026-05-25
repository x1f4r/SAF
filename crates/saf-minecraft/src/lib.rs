use async_trait::async_trait;
use saf_core::ids::AccountId;
use saf_core::ports::{MinecraftAction, MinecraftClient, MinecraftEvent, PortError};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct RecordedMinecraftClient {
    account: AccountId,
    actions: Arc<Mutex<Vec<MinecraftAction>>>,
    events: Arc<Mutex<VecDeque<MinecraftEvent>>>,
}

impl RecordedMinecraftClient {
    pub fn new(account: AccountId) -> Self {
        Self {
            account,
            actions: Arc::new(Mutex::new(Vec::new())),
            events: Arc::new(Mutex::new(VecDeque::new())),
        }
    }

    pub fn actions(&self) -> Vec<MinecraftAction> {
        self.actions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn push_event(&self, event: MinecraftEvent) {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push_back(event);
    }
}

#[async_trait]
impl MinecraftClient for RecordedMinecraftClient {
    async fn account(&self) -> AccountId {
        self.account.clone()
    }

    async fn perform(&self, action: MinecraftAction) -> Result<(), PortError> {
        self.actions
            .lock()
            .map_err(|_| PortError::Failed("actions lock poisoned".to_string()))?
            .push(action);
        Ok(())
    }

    async fn next_event(&self) -> Result<Option<MinecraftEvent>, PortError> {
        Ok(self
            .events
            .lock()
            .map_err(|_| PortError::Failed("events lock poisoned".to_string()))?
            .pop_front())
    }
}

#[cfg(feature = "azalea-native")]
pub mod azalea_native;
#[cfg(test)]
mod tests {
    use super::*;
    use saf_core::ids::AuctionId;

    #[tokio::test]
    async fn records_actions_for_offline_core_tests() {
        let client = RecordedMinecraftClient::new(AccountId::new("Main").unwrap());
        client
            .perform(MinecraftAction::OpenAuction(
                AuctionId::new("auction-1").unwrap(),
            ))
            .await
            .unwrap();

        assert_eq!(client.actions().len(), 1);
    }
}
