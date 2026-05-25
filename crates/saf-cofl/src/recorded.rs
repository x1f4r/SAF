use async_trait::async_trait;
use saf_core::ids::AccountId;
use saf_core::ports::{CoflClient, PortError};
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default)]
pub struct RecordedCoflClient {
    commands: Arc<Mutex<Vec<(AccountId, String)>>>,
}

impl RecordedCoflClient {
    pub fn commands(&self) -> Vec<(AccountId, String)> {
        self.commands
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }
}

#[async_trait]
impl CoflClient for RecordedCoflClient {
    async fn send_command(&self, account: &AccountId, command: &str) -> Result<(), PortError> {
        self.commands
            .lock()
            .map_err(|_| PortError::Failed("commands lock poisoned".to_string()))?
            .push((account.clone(), command.to_string()));
        Ok(())
    }
}
