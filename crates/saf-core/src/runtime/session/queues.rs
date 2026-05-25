use super::super::command_planning::{
    invalid_bank_amount, inventory_item_is_listable, inventory_listing_action, required_bank_amount,
};
use super::*;

impl RuntimeSession {
    pub(super) async fn queue_action(
        &self,
        account: &AccountId,
        action: serde_json::Value,
        state: BotState,
        priority: u8,
        directive: RuntimeDirective,
    ) -> Result<RuntimeOutcome, RuntimeError> {
        let changed = self
            .queue_store(account)?
            .add(account, action, state, priority)
            .await?;
        Ok(RuntimeOutcome::Queued { directive, changed })
    }

    pub(super) async fn queue_transfer_source_deposit(
        &self,
        from: &AccountId,
        to: &AccountId,
        amount: &str,
        stop_source: bool,
        directive: RuntimeDirective,
    ) -> Result<RuntimeOutcome, RuntimeError> {
        if from == to {
            return Err(RuntimeError::Invalid(
                "Source and target accounts must be different.".to_string(),
            ));
        }
        if let Some(supervisor) = &self.supervisor {
            supervisor.start(from).await?;
            self.mark_accounts_started(std::slice::from_ref(from))?;
        }
        let amount = self.transfer_amount(from, amount).await?;
        let changed = self
            .queue_store(from)?
            .add(
                from,
                serde_json::json!({
                    "amount": amount,
                    "withdraw": false,
                    "personal": false,
                    "transfer": {
                        "to": to,
                        "stopSource": stop_source
                    }
                }),
                BotState::Custom("bank".to_string()),
                5,
            )
            .await?;
        Ok(RuntimeOutcome::Queued { directive, changed })
    }

    pub(super) async fn bank_action(
        &self,
        account: &AccountId,
        request: &BankRequest,
    ) -> Result<serde_json::Value, RuntimeError> {
        let amount = self.bank_amount(account, request).await?;
        Ok(serde_json::json!({
            "amount": amount,
            "withdraw": request.withdraw,
            "personal": request.personal
        }))
    }

    async fn bank_amount(
        &self,
        account: &AccountId,
        request: &BankRequest,
    ) -> Result<serde_json::Value, RuntimeError> {
        let raw = required_bank_amount(account.as_str(), request)?;
        if raw.eq_ignore_ascii_case("all") {
            if request.withdraw {
                return Err(invalid_bank_amount(account.as_str(), raw));
            }
            let stats = self
                .stats_provider(account)
                .ok_or_else(|| {
                    RuntimeError::Port(format!(
                        "Live purse data is required to deposit all coins from {account}."
                    ))
                })?
                .stats(account)
                .await?;
            let purse = stats.purse.ok_or_else(|| {
                RuntimeError::Invalid(format!(
                    "No depositable purse value is known for {account}."
                ))
            })?;
            if !purse.is_finite() || purse <= 0.0 {
                return Err(RuntimeError::Invalid(format!(
                    "Invalid bank amount for {account}: all"
                )));
            }
            return Ok(serde_json::Value::from(purse.floor() as u64));
        }

        let amount = parse_compact_number(raw)
            .filter(|amount| amount.is_finite() && *amount > 0.0)
            .ok_or_else(|| invalid_bank_amount(account.as_str(), raw))?;
        Ok(serde_json::Value::from(amount))
    }

    async fn transfer_amount(
        &self,
        account: &AccountId,
        amount: &str,
    ) -> Result<u64, RuntimeError> {
        let amount = amount.trim();
        let value = if amount.eq_ignore_ascii_case("all") {
            let stats = self
                .stats_provider(account)
                .ok_or_else(|| {
                    RuntimeError::Port(format!(
                        "Live purse data is required to transfer all coins from {account}."
                    ))
                })?
                .stats(account)
                .await?;
            stats.purse.ok_or_else(|| {
                RuntimeError::Invalid(format!(
                    "No transferable purse value is known for {account}."
                ))
            })?
        } else {
            parse_compact_number(amount).ok_or_else(|| {
                RuntimeError::Invalid(format!("Invalid transfer amount: {amount}"))
            })?
        };
        if !value.is_finite() || value <= 0.0 {
            return Err(RuntimeError::Invalid(format!(
                "Invalid transfer amount for {account}: {amount}"
            )));
        }
        Ok(value.floor() as u64)
    }

    pub(super) async fn queue_inventory_listings(
        &self,
        account: &AccountId,
        items: &[InventoryItem],
        include_hotbar: bool,
    ) -> Result<usize, RuntimeError> {
        let mut queued = 0;
        let blacklist = self.blacklist_handle().current();
        for item in items
            .iter()
            .filter(|item| inventory_item_is_listable(item, include_hotbar))
        {
            let item_context = ItemContext {
                tag: item.tag.clone(),
                item_name: Some(item.item_name.clone()),
                weird_item_name: Some(item.item_name.clone()),
                enchantments: parse_lore_enchantments(&item.lore),
                is_purchased_inventory_relist: true,
                ..ItemContext::default()
            };
            if blacklist.relist_block_reason(&item_context).is_some() {
                continue;
            }
            let Some(action) = inventory_listing_action(item) else {
                continue;
            };
            self.queue_store(account)?
                .add(account, action, BotState::ListingNoName, 4)
                .await?;
            queued += 1;
        }
        Ok(queued)
    }

    pub(super) async fn queue_delist_all(
        &self,
        account: &AccountId,
        auctions: &[ActiveAuction],
    ) -> Result<usize, RuntimeError> {
        let mut queued = 0;
        for auction in auctions.iter().filter(|auction| {
            !auction.auction_id.trim().is_empty() && !auction.item_uuid.trim().is_empty()
        }) {
            self.queue_store(account)?
                .add(
                    account,
                    serde_json::json!({
                        "auctionID": auction.auction_id.clone(),
                        "itemUuid": auction.item_uuid.clone()
                    }),
                    BotState::Delisting,
                    3,
                )
                .await?;
            queued += 1;
        }
        Ok(queued)
    }
}
