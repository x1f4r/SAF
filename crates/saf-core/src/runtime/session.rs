use super::*;

mod accounts;
mod directives;
mod effects;
mod providers;
mod queues;

impl RuntimeSession {
    pub async fn process_local_command(
        &self,
        command: LocalCommand,
    ) -> Result<RuntimeOutcome, RuntimeError> {
        let directive = self.runtime_with_running().plan_local_command(command)?;
        self.execute_directive(directive).await
    }

    pub async fn process_flip(
        &self,
        account: &AccountId,
        flip: FlipEvent,
    ) -> Result<RuntimeOutcome, RuntimeError> {
        let client = self.minecraft_client(account)?;
        self.runtime.process_flip(client.as_ref(), flip).await
    }
}
