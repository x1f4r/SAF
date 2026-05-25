use super::{LiveRuntime, PendingCompletionKind, PendingListingConfirmation};
use anyhow::Result;
use saf_core::gui::WindowSnapshot;
use saf_core::{AccountId, BotState, MarketInstruction, QueueEntry};
use std::time::{Duration, Instant};

const LISTING_CONFIRMATION_SIGNAL_TIMEOUT: Duration = Duration::from_secs(15);

impl LiveRuntime {
    pub(super) fn is_listing_confirmation_step(
        &self,
        entry: &QueueEntry,
        instruction: &MarketInstruction,
        reason: &str,
    ) -> bool {
        matches!(entry.state, BotState::Listing | BotState::ListingNoName)
            && matches!(instruction, MarketInstruction::ClickSlot { .. })
            && reason.eq_ignore_ascii_case("confirm listing")
    }

    pub(super) fn remember_pending_listing_confirmation(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
    ) {
        let pending = self
            .pending_listing_confirmations
            .entry(account.clone())
            .or_insert_with(|| PendingListingConfirmation {
                entry: entry.clone(),
                attempts: 0,
                last_click_at: Instant::now(),
            });
        pending.entry = entry.clone();
        pending.attempts = pending.attempts.saturating_add(1);
        pending.last_click_at = Instant::now();
    }

    pub(super) fn listing_confirmation_waiting_for_signal(
        &mut self,
        account: &AccountId,
        entry: &QueueEntry,
        window: Option<&WindowSnapshot>,
    ) -> bool {
        let Some(pending) = self.pending_listing_confirmations.get(account) else {
            return false;
        };
        if pending.entry != *entry {
            return false;
        }
        if window.is_some_and(is_confirm_listing_window) {
            return false;
        }
        if pending.last_click_at.elapsed() < LISTING_CONFIRMATION_SIGNAL_TIMEOUT {
            return true;
        }
        tracing::warn!(
            account = %account,
            attempts = pending.attempts,
            "listing confirmation timed out before success signal; leaving entry queued for retry"
        );
        self.pending_listing_confirmations.remove(account);
        false
    }

    pub(super) async fn complete_listing_confirmation_from_chat(
        &mut self,
        account: &AccountId,
        text: &str,
    ) -> Result<bool> {
        if !is_listing_success_message(text) {
            return Ok(false);
        }
        self.complete_pending_listing_confirmation(account, "listing success chat")
            .await
    }

    pub(super) async fn complete_listing_confirmation_from_window(
        &mut self,
        account: &AccountId,
        window: &WindowSnapshot,
    ) -> Result<bool> {
        if !is_active_listing_window(window) {
            return Ok(false);
        }
        self.complete_pending_listing_confirmation(account, "active listing window")
            .await
    }

    async fn complete_pending_listing_confirmation(
        &mut self,
        account: &AccountId,
        source: &str,
    ) -> Result<bool> {
        let Some(pending) = self.pending_listing_confirmations.remove(account) else {
            return Ok(false);
        };
        tracing::debug!(
            account = %account,
            attempts = pending.attempts,
            source,
            "listing confirmation observed; completing queued listing"
        );
        self.pending_market_steps.remove(account);
        self.complete_queue_entry_or_defer(account, &pending.entry, PendingCompletionKind::Generic)
            .await?;
        Ok(true)
    }
}

fn is_listing_success_message(text: &str) -> bool {
    let text = super::support::strip_minecraft_color_codes(text);
    let text = text.trim().to_ascii_lowercase();
    text.contains("bin auction started for") || text.contains("created a bin auction for")
}

fn is_confirm_listing_window(window: &WindowSnapshot) -> bool {
    let title = window.title.to_ascii_lowercase();
    if title.contains("confirm") && (title.contains("auction") || title.contains("bin")) {
        return true;
    }
    let text = super::support::window_text_plain(window);
    text.contains("confirm auction") || text.contains("confirm bin")
}

fn is_active_listing_window(window: &WindowSnapshot) -> bool {
    window
        .title
        .to_ascii_lowercase()
        .contains("bin auction view")
}
