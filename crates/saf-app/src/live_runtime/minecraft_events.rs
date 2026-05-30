use super::island::LiveIslandState;
use super::notifier::notify_operator_best_effort;
#[cfg(feature = "live-cofl")]
use super::should_upload_scoreboard;
use super::{
    LiveRuntime, STARTUP_COFL_TELEMETRY_WAIT, is_bad_modification_message, next_minecraft_event,
    pop_deferred_minecraft_event, startup_ready_notification_body,
};
use anyhow::{Context, Result};
use saf_core::gui::WindowSnapshot;
use saf_core::ports::{
    AccountConnectionProvider, AccountStats, AccountStatsProvider, MinecraftAction,
    MinecraftClient, MinecraftEvent, Notification, NotificationKind,
};
use saf_core::{AccountId, BotState, MarketInstruction};
use std::time::{Duration, Instant};

impl LiveRuntime {
    pub(super) async fn poll_minecraft_once(&mut self) -> Result<()> {
        let clients = self
            .minecraft_clients
            .iter()
            .map(|(account, client)| (account.clone(), client.clone()))
            .collect::<Vec<_>>();
        let running = self.running_account_set();
        for (account, client) in clients {
            if !running.contains(&account) {
                continue;
            }
            loop {
                let Some(event) = self
                    .next_minecraft_event_for_account(&account, client.as_ref())
                    .await?
                else {
                    break;
                };
                self.processed_minecraft_events += 1;
                match event {
                    MinecraftEvent::WindowOpen(window) => {
                        let previous_window = self
                            .active_windows
                            .lock()
                            .map_err(|_| anyhow::anyhow!("active window lock poisoned"))?
                            .get(&account)
                            .cloned();
                        self.remember_window_snapshot(account.clone(), window)?;
                        let window = self
                            .active_windows
                            .lock()
                            .map_err(|_| anyhow::anyhow!("active window lock poisoned"))?
                            .get(&account)
                            .cloned();
                        if let Some(window) = &window
                            && self
                                .complete_listing_confirmation_from_window(&account, window)
                                .await?
                        {
                            if let Err(error) = client
                                .perform(MinecraftAction::CloseWindow)
                                .await
                                .with_context(|| {
                                    format!(
                                        "closing active listing window after confirmation for {account}"
                                    )
                                })
                            {
                                self.handle_minecraft_action_failure(
                                    &account,
                                    "close listing confirmation window",
                                    error,
                                )
                                .await?;
                                break;
                            }
                            self.clear_active_window_cache(&account)?;
                            continue;
                        }
                        if window_open_releases_pending_market_step(
                            previous_window.as_ref(),
                            window.as_ref(),
                        ) {
                            let preserve_context = self
                                .pending_market_steps
                                .get(&account)
                                .is_some_and(|pending| {
                                    pending.opens_sold_claim_action
                                        || pending_listing_submit_transition(
                                            pending,
                                            previous_window.as_ref(),
                                            window.as_ref(),
                                        )
                                });
                            if preserve_context {
                                tracing::debug!(
                                    account = %account,
                                    previous_window = previous_window
                                        .as_ref()
                                        .map(|window| window.title.as_str())
                                        .unwrap_or("none"),
                                    current_window = window
                                        .as_ref()
                                        .map(|window| window.title.as_str())
                                        .unwrap_or("none"),
                                    "preserving pending market context across window transition"
                                );
                            } else {
                                self.pending_market_steps.remove(&account);
                            }
                        }
                        if self
                            .consume_visit_friend_window(&account, client.as_ref())
                            .await?
                        {
                            continue;
                        }
                        if self.consume_startup_profile_window(&account) {
                            let _ = client.perform(MinecraftAction::CloseWindow).await;
                            self.clear_active_window_cache(&account)?;
                            self.schedule_startup_cookie_scan(&account);
                            self.notify_if_startup_ready(&account).await;
                        }
                        if let Some(cookie_duration) = self.consume_startup_cookie_window(&account)
                        {
                            let _ = client.perform(MinecraftAction::CloseWindow).await;
                            self.clear_active_window_cache(&account)?;
                            self.start_auto_cookie_if_needed(&account, cookie_duration)
                                .await?;
                            self.notify_if_startup_ready(&account).await;
                            continue;
                        }
                        if self
                            .process_auto_cookie_window(&account, client.as_ref())
                            .await?
                        {
                            continue;
                        }
                    }
                    MinecraftEvent::WindowClosed => {
                        self.clear_active_window_cache(&account)?;
                        self.pending_market_steps.remove(&account);
                        self.remove_pending_live_buy(&account)?;
                    }
                    MinecraftEvent::Kicked { reason } => {
                        tracing::warn!(
                            account = %account,
                            reason = %reason,
                            "Minecraft account kicked; runtime state cleared and reconnect scheduled"
                        );
                        self.minecraft_ready_accounts.remove(&account);
                        self.clear_active_window_cache(&account)?;
                        self.pending_market_steps.remove(&account);
                        self.remove_pending_live_buy(&account)?;
                        self.pending_auto_cookies.remove(&account);
                        if let Some(state) = self.island_states.get_mut(&account) {
                            if is_bad_modification_message(&reason) {
                                state.mark_bad_modification(Instant::now());
                            } else {
                                state.mark_disconnected();
                            }
                        }
                    }
                    MinecraftEvent::Disconnected { reason } => {
                        tracing::warn!(
                            account = %account,
                            reason = %reason,
                            "Minecraft account disconnected; runtime state cleared and reconnect scheduled"
                        );
                        self.minecraft_ready_accounts.remove(&account);
                        self.clear_active_window_cache(&account)?;
                        self.pending_market_steps.remove(&account);
                        self.remove_pending_live_buy(&account)?;
                        self.pending_auto_cookies.remove(&account);
                        if let Some(state) = self.island_states.get_mut(&account) {
                            state.mark_disconnected();
                        }
                    }
                    MinecraftEvent::Scoreboard { lines } => {
                        if let Err(error) = self.stats.record_scoreboard(&account, &lines) {
                            tracing::warn!(
                                account = %account,
                                error = %error,
                                "scoreboard stats update failed; continuing runtime loop"
                            );
                        }
                        self.handle_island_scoreboard(&account, &lines);
                        self.notify_if_startup_ready(&account).await;
                        #[cfg(feature = "live-cofl")]
                        self.upload_scoreboard(&account, &lines).await;
                    }
                    MinecraftEvent::ChatMessage { text } => {
                        #[cfg(feature = "live-cofl")]
                        self.upload_chat_batch_if_requested(&account, &text).await;
                        if let Some(command) = self.handle_island_chat(&account, &text)? {
                            tracing::info!(
                                account = %account,
                                command = %command,
                                "sending island movement command"
                            );
                            if let Err(error) = client
                                .perform(MinecraftAction::Chat(command.clone()))
                                .await
                                .with_context(|| {
                                    format!(
                                        "sending island movement command {command} for {account}"
                                    )
                                })
                            {
                                self.handle_minecraft_action_failure(
                                    &account,
                                    "island movement command",
                                    error,
                                )
                                .await?;
                                break;
                            }
                        }
                        if self
                            .process_auto_cookie_chat(&account, client.as_ref(), &text)
                            .await?
                        {
                            continue;
                        }
                        self.notify_if_startup_ready(&account).await;
                        match self
                            .stats
                            .record_chat_message(&account, &text, &self.tracked_flips)
                        {
                            Ok(update) => {
                                if let Err(error) =
                                    self.handle_chat_stats_update(&account, update).await
                                {
                                    tracing::warn!(
                                        account = %account,
                                        error = %error,
                                        "chat side-effect handling failed; continuing runtime loop"
                                    );
                                }
                            }
                            Err(error) => {
                                tracing::warn!(
                                    account = %account,
                                    error = %error,
                                    "chat stats update failed; continuing runtime loop"
                                );
                            }
                        }
                        if let Err(error) = self
                            .complete_listing_confirmation_from_chat(&account, &text)
                            .await
                        {
                            tracing::warn!(
                                account = %account,
                                error = %error,
                                "listing confirmation handling failed; continuing runtime loop"
                            );
                        }
                    }
                    MinecraftEvent::Ready { reason } => {
                        tracing::info!(
                            account = %account,
                            reason = %reason,
                            "Minecraft account entered play state"
                        );
                        self.minecraft_ready_accounts.insert(account.clone());
                        self.schedule_island_locraw(&account);
                    }
                }
            }
        }
        Ok(())
    }

    async fn next_minecraft_event_for_account(
        &mut self,
        account: &AccountId,
        client: &dyn MinecraftClient,
    ) -> Result<Option<MinecraftEvent>> {
        if let Some(event) = pop_deferred_minecraft_event(&self.deferred_minecraft_events, account)
            .map_err(anyhow::Error::msg)?
        {
            return Ok(Some(event));
        }
        match next_minecraft_event(client).await {
            Ok(event) => Ok(event),
            Err(error) => {
                tracing::warn!(
                    account = %account,
                    error = %error,
                    "Minecraft event polling failed; account will reconnect on a later poll"
                );
                self.minecraft_ready_accounts.remove(account);
                self.forget_minecraft_runtime_state(account)?;
                if let Some(state) = self.island_states.get_mut(account) {
                    state.mark_disconnected();
                }
                self.schedule_minecraft_reconnect(account).await;
                Ok(None)
            }
        }
    }

    pub(super) async fn handle_minecraft_action_failure(
        &mut self,
        account: &AccountId,
        action: &str,
        error: anyhow::Error,
    ) -> Result<()> {
        tracing::warn!(
            account = %account,
            action = %action,
            error = %error,
            "Minecraft command failed; account will reconnect on a later poll"
        );
        self.minecraft_ready_accounts.remove(account);
        self.forget_minecraft_runtime_state(account)?;
        if let Some(state) = self.island_states.get_mut(account) {
            state.mark_disconnected();
        }
        self.schedule_minecraft_reconnect(account).await;
        Ok(())
    }

    async fn schedule_minecraft_reconnect(&self, account: &AccountId) {
        if let Some(managed) = self.managed_minecraft.get(account) {
            managed.mark_runtime_disconnected().await;
        }
    }

    fn forget_minecraft_runtime_state(&mut self, account: &AccountId) -> Result<()> {
        self.clear_active_window_cache(account)?;
        self.deferred_minecraft_events
            .lock()
            .map_err(|_| anyhow::anyhow!("deferred Minecraft event lock poisoned"))?
            .remove(account);
        self.pending_market_steps.remove(account);
        self.remove_pending_live_buy(account)?;
        self.pending_auto_cookies.remove(account);
        Ok(())
    }

    pub(super) async fn notify_if_startup_ready(&mut self, account: &AccountId) {
        if self.pending_auto_cookies.contains_key(account) {
            return;
        }
        let should_notify = self
            .island_states
            .get_mut(account)
            .is_some_and(LiveIslandState::take_startup_ready_notification_request);
        if should_notify {
            self.notify_startup_ready(account).await;
        }
    }

    async fn notify_startup_ready(&self, account: &AccountId) {
        let Some((stats, connection_id)) = self.startup_ready_details(account).await else {
            return;
        };
        notify_operator_best_effort(
            self.session.as_ref(),
            Notification::new(
                NotificationKind::Started,
                format!("{account} is now ready"),
                startup_ready_notification_body(&stats, connection_id.as_deref()),
                Some(account.clone()),
            )
            .with_thumbnail(crate::player_head::account_head_thumbnail_url(
                account.as_str(),
            )),
        )
        .await;
    }

    async fn startup_ready_details(
        &self,
        account: &AccountId,
    ) -> Option<(AccountStats, Option<String>)> {
        let wait_for_cofl = self.startup_ready_waits_for_cofl(account);
        let deadline = Instant::now() + STARTUP_COFL_TELEMETRY_WAIT;
        loop {
            let stats = match self.stats.stats(account).await {
                Ok(stats) => stats,
                Err(error) => {
                    tracing::warn!(account = %account, error = %error, "failed to load startup-ready stats");
                    return None;
                }
            };
            let connection_id = match self.stats.connection_id(account).await {
                Ok(connection_id) => connection_id,
                Err(error) => {
                    tracing::warn!(account = %account, error = %error, "failed to load startup-ready connection ID");
                    None
                }
            };
            if !wait_for_cofl
                || Self::startup_ready_cofl_telemetry_is_available(&stats, connection_id.as_deref())
                || Instant::now() >= deadline
            {
                return Some((stats, connection_id));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }

    fn startup_ready_waits_for_cofl(&self, account: &AccountId) -> bool {
        #[cfg(feature = "live-cofl")]
        {
            self.cofl_streams
                .iter()
                .any(|stream| &stream.account == account)
        }
        #[cfg(not(feature = "live-cofl"))]
        {
            let _ = account;
            false
        }
    }

    fn startup_ready_cofl_telemetry_is_available(
        stats: &AccountStats,
        connection_id: Option<&str>,
    ) -> bool {
        stats.cofl_tier.is_some() && connection_id.is_some()
    }

    #[cfg(feature = "live-cofl")]
    async fn upload_scoreboard(&self, account: &AccountId, lines: &[String]) {
        if !self.account_market_ready(account) {
            return;
        }
        if !should_upload_scoreboard(lines) {
            return;
        }
        let Some(stream) = self
            .cofl_streams
            .iter()
            .find(|stream| &stream.account == account)
        else {
            return;
        };
        stream.upload_scoreboard_if_needed(lines).await;
    }

    #[cfg(feature = "live-cofl")]
    async fn upload_chat_batch_if_requested(&self, account: &AccountId, text: &str) {
        let Some(stream) = self
            .cofl_streams
            .iter()
            .find(|stream| &stream.account == account)
        else {
            return;
        };
        stream.client.upload_chat_batch_if_requested(text).await;
    }
}

fn window_open_releases_pending_market_step(
    previous: Option<&WindowSnapshot>,
    current: Option<&WindowSnapshot>,
) -> bool {
    previous.is_none_or(|previous| {
        current.is_some_and(|current| previous.title.trim() != current.title.trim())
    })
}

fn pending_listing_submit_transition(
    pending: &super::PendingMarketStep,
    previous: Option<&WindowSnapshot>,
    current: Option<&WindowSnapshot>,
) -> bool {
    matches!(
        pending.entry.state,
        BotState::Listing | BotState::ListingNoName
    ) && matches!(pending.instruction, MarketInstruction::ClickSlot { .. })
        && previous.is_some_and(super::auction_flow::is_create_auction_window)
        && current.is_some_and(super::auction_flow::is_confirm_auction_window)
}
