use super::auction_flow::is_reconcile_queue_entry;
use super::notifier::notify_operator_best_effort;
use super::support::{number_value, slot_text_plain, string_value};
use super::{
    AccountId, LiveRuntime, MISSING_LISTING_INVENTORY_MAX_ATTEMPTS,
    MISSING_LISTING_INVENTORY_RETRY_DELAY, PendingCompletionKind,
    PendingMissingListingInventoryRetry,
};
use anyhow::Result;
use saf_core::gui::WindowSnapshot;
use saf_core::ports::{Notification, NotificationKind, QueueStore};
use saf_core::{BotState, MarketInstruction, QueueEntry};
use std::time::Instant;

impl LiveRuntime {
    pub(super) async fn handle_missing_listing_inventory_once(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        window: &WindowSnapshot,
    ) -> Result<bool> {
        if !self.options.market_actions.allows_market_actions()
            || !missing_listing_inventory_uuid(entry, window)
        {
            return Ok(false);
        }

        // Foolproof cleanup: if the item is genuinely not in the live inventory
        // (already listed/sold, or it never arrived), the listing is stale. Drop
        // it at once rather than looping on a retry counter — that counter keys on
        // full-entry equality, which can vary between polls, so a stale listing
        // could otherwise stay stuck forever and starve everything behind it.
        let stale_item = explicit_listing_inventory_uuid(entry).filter(|uuid| looks_like_item_uuid(uuid));
        if let Some(item_uuid) = stale_item
            && self
                .listing_item_absent_from_live_inventory(account, &item_uuid)
                .await
        {
            self.pending_missing_listing_inventory_retries
                .remove(account);
            tracing::warn!(
                account = %account,
                inventory = %item_uuid,
                action = ?entry.action,
                "listing item is not in the live inventory (already listed/sold/gone); dropping the stale listing entry"
            );
            if let Err(error) = self
                .session
                .execute_market_instruction(account, &MarketInstruction::CloseWindow)
                .await
            {
                tracing::warn!(
                    account = %account,
                    error = %error,
                    "failed to close window while dropping a stale listing entry"
                );
            }
            self.clear_active_window_cache(account)?;
            self.pending_market_steps.remove(account);
            self.complete_queue_entry_or_defer(account, entry, PendingCompletionKind::CountOnly)
                .await?;
            return Ok(true);
        }

        let attempts = self
            .pending_missing_listing_inventory_retries
            .get(account)
            .filter(|pending| pending.entry == *entry)
            .map(|pending| pending.attempts.saturating_add(1))
            .unwrap_or(1);

        if is_expired_relist_entry(entry) {
            self.remember_missing_listing_inventory_retry(account, entry, attempts);
            tracing::warn!(
                account = %account,
                inventory = ?explicit_listing_inventory_uuid(entry),
                attempts,
                retry_after_ms = MISSING_LISTING_INVENTORY_RETRY_DELAY.as_millis(),
                "expired relist item is not visible in the create-auction inventory; retrying listing without queueing another reconcile"
            );
            if let Err(error) = self
                .session
                .execute_market_instruction(account, &MarketInstruction::CloseWindow)
                .await
            {
                tracing::warn!(
                    account = %account,
                    error = %error,
                    "failed to close create-auction window after expired relist item was missing"
                );
            }
            self.clear_active_window_cache(account)?;
            self.remember_pending_market_step(
                account,
                entry,
                &MarketInstruction::Chat {
                    message: "/ah".to_string(),
                },
            );
            return Ok(true);
        }

        if attempts < MISSING_LISTING_INVENTORY_MAX_ATTEMPTS {
            self.remember_missing_listing_inventory_retry(account, entry, attempts);
            tracing::warn!(
                account = %account,
                inventory = ?explicit_listing_inventory_uuid(entry),
                attempts,
                max_attempts = MISSING_LISTING_INVENTORY_MAX_ATTEMPTS,
                retry_after_ms = MISSING_LISTING_INVENTORY_RETRY_DELAY.as_millis(),
                "listing item is not visible in the create-auction inventory; retrying before reconciling status"
            );
            if let Err(error) = self
                .session
                .execute_market_instruction(account, &MarketInstruction::CloseWindow)
                .await
            {
                tracing::warn!(
                    account = %account,
                    error = %error,
                    "failed to close create-auction window after listing item was missing"
                );
            }
            self.clear_active_window_cache(account)?;
            self.remember_pending_market_step(
                account,
                entry,
                &MarketInstruction::Chat {
                    message: "/ah".to_string(),
                },
            );
            return Ok(true);
        }

        self.pending_missing_listing_inventory_retries
            .remove(account);
        tracing::error!(
            account = %account,
            inventory = ?explicit_listing_inventory_uuid(entry),
            attempts,
            max_attempts = MISSING_LISTING_INVENTORY_MAX_ATTEMPTS,
            action = ?entry.action,
            "listing item remained missing after retries; queueing auction reconcile and clearing listing entry"
        );
        self.queue_listing_status_reconcile(account, entry).await?;
        if let Err(error) = self
            .session
            .execute_market_instruction(account, &MarketInstruction::CloseWindow)
            .await
        {
            tracing::warn!(
                account = %account,
                error = %error,
                "failed to close stale listing window after inventory item disappeared"
            );
        }
        self.clear_active_window_cache(account)?;
        self.pending_market_steps.remove(account);
        self.complete_queue_entry_or_defer(account, entry, PendingCompletionKind::CountOnly)
            .await?;
        notify_operator_best_effort(
            self.session.as_ref(),
            Notification::new(
                NotificationKind::Blocked,
                "Listing item missing",
                format!(
                    "`{}` could not find the queued listing item after {attempts} attempts. Rust queued auction reconciliation and removed the listing entry.",
                    account.as_str()
                ),
                Some(account.clone()),
            )
            .with_thumbnail(crate::player_head::account_head_thumbnail_url(
                account.as_str(),
            )),
        )
        .await;
        Ok(true)
    }

    /// Reads the live (passively-synced) inventory and reports whether `item_uuid`
    /// is absent from it. Returns `false` if the inventory can't be read, so a
    /// transient read failure never causes a real listing to be dropped.
    async fn listing_item_absent_from_live_inventory(
        &self,
        account: &AccountId,
        item_uuid: &str,
    ) -> bool {
        match self.session.inventory_snapshot(account).await {
            Ok(snapshot) => !snapshot.items.iter().any(|item| {
                item.uuid
                    .as_deref()
                    .is_some_and(|uuid| uuid.eq_ignore_ascii_case(item_uuid))
            }),
            Err(error) => {
                tracing::debug!(
                    account = %account,
                    error = %error,
                    "could not read live inventory to validate a listing item; will retry instead of dropping"
                );
                false
            }
        }
    }

    fn remember_missing_listing_inventory_retry(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        attempts: u8,
    ) {
        self.pending_missing_listing_inventory_retries.insert(
            account.clone(),
            PendingMissingListingInventoryRetry {
                entry: entry.clone(),
                retry_at: Instant::now() + MISSING_LISTING_INVENTORY_RETRY_DELAY,
                attempts,
            },
        );
    }

    pub(super) async fn queue_listing_status_reconcile(
        &self,
        account: &AccountId,
        entry: &QueueEntry,
    ) -> Result<()> {
        let queue = self.queue.snapshot(account).await?;
        if queue.iter().any(is_reconcile_queue_entry) {
            return Ok(());
        }
        let mut action = serde_json::json!({ "reason": "listing-status-unclear" });
        if let Some(object) = action.as_object_mut() {
            if let Some(inventory) =
                string_value(&entry.action, &["inventory", "inv", "itemUuid", "itemUUID"])
            {
                object.insert("inventory".to_string(), serde_json::json!(inventory));
            }
            if let Some(auction_id) =
                string_value(&entry.action, &["auctionID", "auctionId", "auction_id"])
            {
                object.insert("auctionID".to_string(), serde_json::json!(auction_id));
            }
            if let Some(item_name) = string_value(&entry.action, &["itemName", "weirdItemName"]) {
                object.insert("itemName".to_string(), serde_json::json!(item_name));
            }
            if let Some(price) = number_value(&entry.action, &["price", "oldPrice", "listPrice"]) {
                object.insert("price".to_string(), serde_json::json!(price));
            }
        }
        self.queue
            .add(
                account,
                action,
                BotState::Custom("reconcileAuctions".to_string()),
                2,
            )
            .await?;
        Ok(())
    }
}

/// Cheap check that a value looks like a SkyBlock item UUID (dashed, ~36 chars),
/// so we only run the live-inventory "is it gone?" drop for real per-item UUIDs —
/// never for tag-style fallbacks, which would risk dropping a present item.
fn looks_like_item_uuid(value: &str) -> bool {
    value.len() >= 32 && value.contains('-')
}

fn missing_listing_inventory_uuid(entry: &QueueEntry, window: &WindowSnapshot) -> bool {
    let Some(item_uuid) = explicit_listing_inventory_uuid(entry) else {
        return false;
    };
    if !is_create_auction_window(window) || !create_window_has_item_slot(window) {
        return false;
    }
    if !create_window_has_selectable_inventory_item(window)
        && !create_window_has_loaded_inventory_area(window)
    {
        return false;
    }
    !window
        .slots
        .iter()
        .any(|slot| listing_slot_matches_entry(entry, slot, &item_uuid))
}

fn explicit_listing_inventory_uuid(entry: &QueueEntry) -> Option<String> {
    if !matches!(entry.state, BotState::Listing | BotState::ListingNoName) {
        return None;
    }
    string_value(&entry.action, &["inventory", "inv", "itemUuid", "itemUUID"])
}

fn is_expired_relist_entry(entry: &QueueEntry) -> bool {
    matches!(entry.state, BotState::ListingNoName)
        && explicit_listing_inventory_uuid(entry).is_some()
        && entry.action.get("oldPrice").is_some()
        && entry
            .action
            .get("pricePaid")
            .and_then(serde_json::Value::as_f64)
            .is_some_and(|price_paid| price_paid == 0.0)
}

fn is_create_auction_window(window: &WindowSnapshot) -> bool {
    let title = window.title.to_ascii_lowercase();
    title.contains("create") && title.contains("auction")
}

fn create_window_has_item_slot(window: &WindowSnapshot) -> bool {
    window.slots.iter().any(|slot| {
        slot.slot == 13 && {
            let text = slot_text_plain(slot).to_ascii_lowercase();
            text.contains("auction for item")
                || text.contains("item to auction")
                || text.contains("click an item")
                || text.contains("select an item")
                || text.contains("auction item")
        }
    })
}

fn create_window_has_selectable_inventory_item(window: &WindowSnapshot) -> bool {
    window
        .slots
        .iter()
        .any(|slot| ![13, 29, 31, 33, 48, 49].contains(&slot.slot) && slot.item_uuid.is_some())
}

fn create_window_has_loaded_inventory_area(window: &WindowSnapshot) -> bool {
    window.slots.len() >= 45 || window.slots.iter().any(|slot| slot.slot >= 44)
}

fn listing_slot_matches_entry(
    entry: &QueueEntry,
    slot: &saf_core::gui::WindowSlot,
    item_uuid: &str,
) -> bool {
    if slot
        .item_uuid
        .as_deref()
        .is_some_and(|uuid| uuid.eq_ignore_ascii_case(item_uuid))
    {
        return true;
    }
    if selected_auction_item_without_uuid(slot) && slot_matches_item_name_context(entry, slot) {
        return true;
    }
    let tag = string_value(&entry.action, &["tag"]);
    let allow_context_fallback = tag
        .as_deref()
        .is_some_and(|tag| item_uuid.eq_ignore_ascii_case(tag));
    if !allow_context_fallback {
        return false;
    }
    if let Some(item_name) = string_value(&entry.action, &["itemName", "item_name"]) {
        let wanted = normalized_listing_text(&item_name);
        if !wanted.is_empty() && normalized_slot_text(slot).contains(&wanted) {
            return true;
        }
    }
    if let Some(tag) = tag
        && listing_slot_matches_tag(slot, &tag)
    {
        return true;
    }
    false
}

fn selected_auction_item_without_uuid(slot: &saf_core::gui::WindowSlot) -> bool {
    slot.slot == 13 && slot.item_uuid.is_none()
}

fn slot_matches_item_name_context(entry: &QueueEntry, slot: &saf_core::gui::WindowSlot) -> bool {
    let Some(item_name) = string_value(&entry.action, &["itemName", "item_name"]) else {
        return false;
    };
    let wanted = normalized_listing_text(&item_name);
    !wanted.is_empty() && normalized_slot_text(slot).contains(&wanted)
}

fn listing_slot_matches_tag(slot: &saf_core::gui::WindowSlot, tag: &str) -> bool {
    if slot
        .item_uuid
        .as_deref()
        .is_some_and(|uuid| uuid.eq_ignore_ascii_case(tag))
    {
        return true;
    }
    let wanted = normalized_tag_text(tag);
    !wanted.is_empty() && normalized_slot_text(slot).contains(&wanted)
}

fn normalized_slot_text(slot: &saf_core::gui::WindowSlot) -> String {
    normalized_listing_text(
        &std::iter::once(slot.display_name.as_str())
            .chain(slot.lore.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn normalized_tag_text(tag: &str) -> String {
    let mut parts = tag
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    if parts
        .first()
        .is_some_and(|first| matches!(*first, "PET" | "RUNE" | "UNIQUE"))
    {
        parts.remove(0);
    }
    normalized_listing_text(&parts.join(" "))
}

fn normalized_listing_text(text: &str) -> String {
    let mut plain = String::new();
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '§' {
            chars.next();
            continue;
        }
        if ch.is_ascii_alphanumeric() {
            plain.push(ch.to_ascii_lowercase());
        } else {
            plain.push(' ');
        }
    }
    plain.split_whitespace().collect::<Vec<_>>().join(" ")
}
