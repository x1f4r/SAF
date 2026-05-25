use super::*;

impl RuntimeSession {
    pub async fn notify(&self, notification: Notification) -> Result<(), RuntimeError> {
        if let Some(notifier) = &self.notifier {
            notifier.notify(notification).await?;
        }
        Ok(())
    }

    pub fn plan_market_queue_entry(
        &self,
        entry: &crate::QueueEntry,
        window: Option<&WindowSnapshot>,
    ) -> Option<MarketStep> {
        MarketWorkflow::from_queue_entry(entry).map(|workflow| workflow.next_step(window))
    }

    pub async fn execute_market_instruction(
        &self,
        account: &AccountId,
        instruction: &crate::MarketInstruction,
    ) -> Result<(), RuntimeError> {
        match instruction {
            crate::MarketInstruction::Noop => Ok(()),
            crate::MarketInstruction::Chat { message } => {
                self.minecraft_client(account)?
                    .perform(MinecraftAction::Chat(message.clone()))
                    .await?;
                Ok(())
            }
            crate::MarketInstruction::OpenAuction { auction_id } => {
                self.minecraft_client(account)?
                    .perform(MinecraftAction::OpenAuction(auction_id.clone()))
                    .await?;
                Ok(())
            }
            crate::MarketInstruction::ClickSlot { slot } => {
                self.minecraft_client(account)?
                    .perform(MinecraftAction::ClickSlot(*slot))
                    .await?;
                Ok(())
            }
            crate::MarketInstruction::ClickSlotThenType { slot, text } => {
                let client = self.minecraft_client(account)?;
                client.perform(MinecraftAction::ClickSlot(*slot)).await?;
                client
                    .perform(MinecraftAction::TypeText(text.clone()))
                    .await?;
                Ok(())
            }
            crate::MarketInstruction::CloseWindow => {
                self.minecraft_client(account)?
                    .perform(MinecraftAction::CloseWindow)
                    .await?;
                Ok(())
            }
            crate::MarketInstruction::TypeText { text } => {
                self.minecraft_client(account)?
                    .perform(MinecraftAction::TypeText(text.clone()))
                    .await?;
                Ok(())
            }
        }
    }
}
