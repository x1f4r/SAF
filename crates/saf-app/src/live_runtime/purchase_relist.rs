use super::notifier::notify_operator_best_effort;
use super::support::normalized_purchase_item_name;
use super::{
    AccountId, LiveRuntime, PURCHASE_RELIST_MAX_ATTEMPTS, PURCHASE_RELIST_RETRY_DELAY,
    PendingCompletionKind, PurchaseStatsUpdate, Result,
};
use saf_core::ports::{
    InventoryItem, InventoryProvider, Notification, NotificationKind, PortError, QueueStore,
};
use saf_core::{
    BlacklistPolicy, BotState, ItemContext, ItemUuid, MarketInstruction, QueueEntry, RelistPlan,
    RelistPurchase,
};
use serde_json::Value;
use std::time::Instant;
use tokio::time::{Duration, sleep};

const PURCHASE_RELIST_QUEUE_PRIORITY: u8 = 1;
const PURCHASE_RELIST_MAX_CLAIM_ATTEMPTS: u8 = 3;
const PURCHASE_CLAIM_SETTLE_DELAY: Duration = Duration::from_millis(750);

#[derive(Clone, Debug)]
pub(super) struct PendingPurchaseRelist {
    pub(super) account: AccountId,
    pub(super) purchase: PurchaseStatsUpdate,
    pub(super) attempts: u8,
    pub(super) claim_attempts: u8,
    pub(super) ready_at: Instant,
}

impl LiveRuntime {
    pub(super) async fn queue_purchase_relist(
        &mut self,
        account: &AccountId,
        purchase: &PurchaseStatsUpdate,
    ) -> Result<()> {
        if !self.should_relist_purchase(purchase) {
            return Ok(());
        }
        match self
            .try_queue_purchase_relist(account, purchase, false, 0)
            .await
        {
            Ok(true) => return Ok(()),
            Ok(false) => {}
            Err(error) => {
                tracing::warn!(
                    account = %account,
                    auction_id = %purchase.auction_id,
                    error = %error,
                    "failed to queue purchase relist; retrying later"
                );
            }
        }
        self.defer_purchase_relist(account, purchase, 0, 0);
        Ok(())
    }

    fn defer_purchase_relist(
        &mut self,
        account: &AccountId,
        purchase: &PurchaseStatsUpdate,
        attempts: u8,
        claim_attempts: u8,
    ) {
        if self.pending_purchase_relists.iter().any(|pending| {
            pending.account == *account && pending.purchase.auction_id == purchase.auction_id
        }) {
            return;
        }
        self.pending_purchase_relists.push(PendingPurchaseRelist {
            account: account.clone(),
            purchase: purchase.clone(),
            attempts,
            claim_attempts,
            ready_at: Instant::now() + PURCHASE_RELIST_RETRY_DELAY,
        });
    }

    fn should_relist_purchase(&self, purchase: &PurchaseStatsUpdate) -> bool {
        if !self.config.relist {
            return false;
        }
        let policy =
            BlacklistPolicy::from_config(&self.config.do_not_buy, &self.config.do_not_relist);
        policy
            .relist_block_reason(&ItemContext {
                tag: purchase.tag.clone(),
                item_name: Some(purchase.item_name.clone()),
                weird_item_name: Some(purchase.weird_item_name.clone()),
                finder: Some(purchase.finder.clone()),
                profit: Some(purchase.profit),
                ..ItemContext::default()
            })
            .is_none()
    }

    async fn try_queue_purchase_relist(
        &self,
        account: &AccountId,
        purchase: &PurchaseStatsUpdate,
        final_attempt: bool,
        claim_attempts: u8,
    ) -> Result<bool> {
        let Some(auction_id) = saf_core::AuctionId::new(&purchase.auction_id) else {
            return Ok(true);
        };
        let item_uuid = match self.find_purchased_inventory_uuid(account, purchase).await {
            Ok(item_uuid) => item_uuid,
            Err(error) => {
                tracing::warn!(
                    account = %account,
                    auction_id = %purchase.auction_id,
                    error = %error,
                    "failed to inspect purchased inventory for relist; retrying or queueing explicit claim"
                );
                if !final_attempt {
                    return Ok(false);
                }
                None
            }
        };
        let Some(item_uuid) = item_uuid else {
            if final_attempt {
                return self
                    .queue_purchased_item_claim(account, purchase, claim_attempts)
                    .await;
            }
            return Ok(false);
        };
        let plan = RelistPlan::from_purchase(
            &self.config,
            RelistPurchase {
                auction_id,
                target_price: purchase.target_price,
                profit: purchase.profit,
                item_name: purchase.item_name.clone(),
                tag: purchase.tag.clone(),
                item_uuid: Some(item_uuid.clone()),
                override_price: false,
            },
        )
        .ok_or_else(|| anyhow::anyhow!("could not build relist plan for {}", purchase.item_name))?;
        let mut action = serde_json::json!({
            "profit": plan.profit,
            "finder": purchase.finder.clone(),
            "itemName": plan.item_name,
            "tag": plan.tag,
            "auctionID": plan.auction_id.to_string(),
            "price": plan.list_price,
            "time": plan.listing_hours,
            "weirdItemName": purchase.weird_item_name.clone(),
            "pricePaid": purchase.price,
            "targetPrice": purchase.target_price
        });
        action["inventory"] = Value::String(item_uuid.to_string());
        action["inv"] = Value::String(item_uuid.to_string());
        self.queue
            .add(
                account,
                action,
                BotState::Listing,
                PURCHASE_RELIST_QUEUE_PRIORITY,
            )
            .await
            .map_err(anyhow::Error::from)?;
        Ok(true)
    }

    pub(super) async fn drain_pending_purchase_relists(&mut self) -> Result<()> {
        let now = Instant::now();
        let mut waiting = Vec::new();
        let mut ready = Vec::new();
        for pending in self.pending_purchase_relists.drain(..) {
            if pending.ready_at <= now {
                ready.push(pending);
            } else {
                waiting.push(pending);
            }
        }
        self.pending_purchase_relists = waiting;

        for mut pending in ready {
            let final_attempt = pending.attempts >= PURCHASE_RELIST_MAX_ATTEMPTS;
            match self
                .try_queue_purchase_relist_with_claims(
                    &pending.account,
                    &pending.purchase,
                    final_attempt,
                    pending.claim_attempts,
                )
                .await
            {
                Ok(true) => continue,
                Ok(false) => {}
                Err(error) => {
                    tracing::warn!(
                        account = %pending.account,
                        auction_id = %pending.purchase.auction_id,
                        error = %error,
                        "failed to queue purchase relist; retrying later"
                    );
                }
            }
            pending.attempts = pending.attempts.saturating_add(1);
            pending.ready_at = Instant::now() + PURCHASE_RELIST_RETRY_DELAY;
            self.pending_purchase_relists.push(pending);
        }

        Ok(())
    }

    async fn find_purchased_inventory_uuid(
        &self,
        account: &AccountId,
        purchase: &PurchaseStatsUpdate,
    ) -> Result<Option<ItemUuid>, PortError> {
        let Some(managed) = self.managed_minecraft.get(account) else {
            return Ok(None);
        };
        let snapshot = match managed.snapshot(account).await {
            Ok(snapshot) => snapshot,
            Err(PortError::Unavailable(_)) => return Ok(None),
            Err(error) => return Err(error),
        };
        let target_tag = purchase.tag.as_ref().map(|tag| tag.to_ascii_uppercase());
        let target_name = normalized_purchase_item_name(&purchase.item_name);
        let has_unique_uuid = |item: &InventoryItem| {
            unique_purchased_inventory_uuid(item, target_tag.as_deref()).is_some()
        };
        let is_listable_slot =
            |item: &InventoryItem| item.slot.is_none_or(|slot| (9..=44).contains(&slot));
        let name_matches = |item: &InventoryItem| {
            !target_name.is_empty() && normalized_purchase_item_name(&item.item_name) == target_name
        };
        let tag_matches = |item: &InventoryItem, tag: &str| {
            item.tag
                .as_ref()
                .is_some_and(|item_tag| item_tag.eq_ignore_ascii_case(tag))
        };
        let tag_only_matches = target_tag
            .as_deref()
            .map(|tag| {
                snapshot
                    .items
                    .iter()
                    .filter(|item| {
                        is_listable_slot(item) && has_unique_uuid(item) && tag_matches(item, tag)
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let matched = snapshot
            .items
            .iter()
            .find(|item| {
                is_listable_slot(item)
                    && has_unique_uuid(item)
                    && name_matches(item)
                    && target_tag
                        .as_deref()
                        .is_none_or(|tag| tag_matches(item, tag))
            })
            .or_else(|| {
                snapshot.items.iter().find(|item| {
                    is_listable_slot(item) && has_unique_uuid(item) && name_matches(item)
                })
            });
        if matched.is_none() {
            if tag_only_matches.len() > 1 {
                tracing::warn!(
                    account = %account,
                    auction_id = %purchase.auction_id,
                    item = %purchase.item_name,
                    tag = ?purchase.tag,
                    matches = tag_only_matches.len(),
                    "ambiguous purchased inventory tag match; waiting for exact purchased item name"
                );
            } else if tag_only_matches.len() == 1 {
                tracing::warn!(
                    account = %account,
                    auction_id = %purchase.auction_id,
                    item = %purchase.item_name,
                    tag = ?purchase.tag,
                    "purchased inventory tag matched one item but name did not match yet; waiting for exact purchased item name"
                );
            }
        }
        Ok(matched.and_then(|item| unique_purchased_inventory_uuid(item, target_tag.as_deref())))
    }

    async fn try_queue_purchase_relist_with_claims(
        &self,
        account: &AccountId,
        purchase: &PurchaseStatsUpdate,
        final_attempt: bool,
        claim_attempts: u8,
    ) -> Result<bool> {
        if final_attempt && claim_attempts >= PURCHASE_RELIST_MAX_CLAIM_ATTEMPTS {
            tracing::error!(
                account = %account,
                auction_id = %purchase.auction_id,
                item = %purchase.item_name,
                claim_attempts,
                "purchased item never appeared in inventory after claim attempts; leaving it for manual recovery"
            );
            notify_operator_best_effort(
                self.session.as_ref(),
                Notification::new(
                    NotificationKind::Error,
                    "Purchased item recovery needed",
                    format!(
                        "`{}` bought `{}` but Rust still cannot see the claimed inventory item after {claim_attempts} claim attempts. It did not queue a listing with an unsafe fallback selector.",
                        account.as_str(),
                        purchase.item_name
                    ),
                    Some(account.clone()),
                )
                .with_fields(vec![("Item".to_string(), purchase.item_name.clone())])
                .with_thumbnail(crate::player_head::account_head_thumbnail_url(
                    account.as_str(),
                )),
            )
            .await;
            return Ok(true);
        }

        let queued = self
            .try_queue_purchase_relist(account, purchase, final_attempt, claim_attempts)
            .await?;
        if queued {
            return Ok(true);
        }
        Ok(false)
    }

    async fn queue_purchased_item_claim(
        &self,
        account: &AccountId,
        purchase: &PurchaseStatsUpdate,
        claim_attempts: u8,
    ) -> Result<bool> {
        let Some(auction_id) = saf_core::AuctionId::new(&purchase.auction_id) else {
            return Ok(true);
        };
        let plan = RelistPlan::from_purchase(
            &self.config,
            RelistPurchase {
                auction_id,
                target_price: purchase.target_price,
                profit: purchase.profit,
                item_name: purchase.item_name.clone(),
                tag: purchase.tag.clone(),
                item_uuid: None,
                override_price: false,
            },
        )
        .ok_or_else(|| anyhow::anyhow!("could not build relist plan for {}", purchase.item_name))?;
        let next_claim_attempts = claim_attempts.saturating_add(1);
        let action = serde_json::json!({
            "profit": plan.profit,
            "finder": purchase.finder.clone(),
            "itemName": plan.item_name,
            "tag": plan.tag,
            "auctionID": plan.auction_id.to_string(),
            "price": plan.list_price,
            "time": plan.listing_hours,
            "weirdItemName": purchase.weird_item_name.clone(),
            "pricePaid": purchase.price,
            "targetPrice": purchase.target_price,
            "claimAttempts": next_claim_attempts
        });
        let already_queued = self
            .queue
            .snapshot(account)
            .await
            .map_err(anyhow::Error::from)?
            .iter()
            .any(|entry| {
                is_claim_purchased_entry(entry)
                    && auction_id_for_entry(entry).as_deref() == Some(purchase.auction_id.as_str())
            });
        if already_queued {
            return Ok(true);
        }
        tracing::warn!(
            account = %account,
            auction_id = %purchase.auction_id,
            item = %purchase.item_name,
            claim_attempts = next_claim_attempts,
            "purchased item is not visible in inventory; queueing explicit auction claim before relist"
        );
        self.queue
            .add(
                account,
                action,
                BotState::Custom("claimPurchased".to_string()),
                PURCHASE_RELIST_QUEUE_PRIORITY,
            )
            .await
            .map_err(anyhow::Error::from)?;
        Ok(true)
    }

    pub(super) async fn complete_claim_purchased_entry_if_needed(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        instruction: &MarketInstruction,
        reason: &str,
    ) -> Result<bool> {
        if !is_claim_purchased_entry(entry) {
            return Ok(false);
        }
        if matches!(instruction, MarketInstruction::ClickSlot { .. }) {
            sleep(PURCHASE_CLAIM_SETTLE_DELAY).await;
            if let Err(error) = self
                .session
                .execute_market_instruction(account, &MarketInstruction::CloseWindow)
                .await
            {
                tracing::warn!(
                    account = %account,
                    error = %error,
                    "failed to close purchased auction claim window"
                );
            }
            self.clear_active_window_cache(account)?;
            if let Some((purchase, claim_attempts)) = purchase_from_claim_entry(entry) {
                self.defer_purchase_relist(account, &purchase, 0, claim_attempts);
            } else {
                tracing::error!(
                    account = %account,
                    action = ?entry.action,
                    "completed purchased auction claim but could not recover relist metadata"
                );
            }
        } else {
            tracing::warn!(
                account = %account,
                reason = %reason,
                action = ?entry.action,
                "purchased auction claim entry completed without a claim click"
            );
            notify_operator_best_effort(
                self.session.as_ref(),
                Notification::new(
                    NotificationKind::Error,
                    "Purchased item claim skipped",
                    format!(
                        "`{}` could not find a claim button for the purchased auction. The listing was not queued.",
                        account.as_str()
                    ),
                    Some(account.clone()),
                )
                .with_thumbnail(crate::player_head::account_head_thumbnail_url(
                    account.as_str(),
                )),
            )
            .await;
        }
        self.pending_market_steps.remove(account);
        self.complete_queue_entry_or_defer(account, entry, PendingCompletionKind::CountOnly)
            .await?;
        Ok(true)
    }
}

pub(super) fn is_claim_purchased_entry(entry: &QueueEntry) -> bool {
    matches!(&entry.state, BotState::Custom(name) if name == "claimPurchased")
}

fn auction_id_for_entry(entry: &QueueEntry) -> Option<String> {
    string_value(&entry.action, &["auctionID", "auctionId", "auction_id"])
}

fn string_value(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    })
}

fn number_value(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(|value| value.as_f64().or_else(|| value.as_str()?.parse().ok()))
            .filter(|value| value.is_finite())
    })
}

fn unique_purchased_inventory_uuid(
    item: &InventoryItem,
    target_tag: Option<&str>,
) -> Option<ItemUuid> {
    let uuid = item.uuid.as_deref()?.trim();
    if uuid.is_empty()
        || target_tag.is_some_and(|tag| uuid.eq_ignore_ascii_case(tag))
        || item
            .tag
            .as_deref()
            .is_some_and(|tag| uuid.eq_ignore_ascii_case(tag))
        || looks_like_skyblock_item_tag(uuid)
    {
        return None;
    }
    ItemUuid::new(uuid)
}

fn looks_like_skyblock_item_tag(value: &str) -> bool {
    let value = value.trim();
    let mut has_underscore = false;
    let mut has_letter = false;
    for ch in value.chars() {
        if ch == '_' {
            has_underscore = true;
            continue;
        }
        if ch.is_ascii_uppercase() {
            has_letter = true;
            continue;
        }
        if ch.is_ascii_digit() {
            continue;
        }
        return false;
    }
    has_underscore && has_letter
}

fn purchase_from_claim_entry(entry: &QueueEntry) -> Option<(PurchaseStatsUpdate, u8)> {
    if !is_claim_purchased_entry(entry) {
        return None;
    }
    let auction_id = auction_id_for_entry(entry)?;
    let item_name = string_value(&entry.action, &["itemName", "item_name"])?;
    let weird_item_name = string_value(&entry.action, &["weirdItemName", "weird_item_name"])
        .unwrap_or_else(|| normalized_purchase_item_name(&item_name));
    let price = number_value(&entry.action, &["pricePaid", "startingBid", "starting_bid"])
        .unwrap_or(0.0)
        .max(0.0)
        .round() as u64;
    let target_price = number_value(&entry.action, &["targetPrice", "target", "price"])?;
    let profit = number_value(&entry.action, &["profit"]).unwrap_or(0.0);
    let finder = string_value(&entry.action, &["finder"]).unwrap_or_else(|| "UNKNOWN".to_string());
    let claim_attempts = number_value(&entry.action, &["claimAttempts", "claim_attempts"])
        .unwrap_or(0.0)
        .max(0.0)
        .round()
        .min(u8::MAX as f64) as u8;
    Some((
        PurchaseStatsUpdate {
            auction_id,
            item_name,
            weird_item_name,
            tag: string_value(&entry.action, &["tag"]),
            price,
            target_price,
            profit,
            finder,
            volume: number_value(&entry.action, &["volume"]),
            profit_percentage: number_value(&entry.action, &["profitPercentage", "profitPerc"]),
            buy_kind: string_value(&entry.action, &["buyKind", "bed"])
                .unwrap_or_else(|| "NUGGET".to_string()),
            buy_speed_ms: None,
        },
        claim_attempts,
    ))
}
