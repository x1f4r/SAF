use super::super::inventory_logging::log_inventory_snapshot;
use super::super::notifier::{all_flip_notification, notify_operator_best_effort};
use super::super::stats::LiveStatsProvider;
use super::super::tracked::LiveTrackedFlipProvider;
use super::super::{COFL_READ_TIMEOUT, MarketActionMode};
use super::client::LiveCoflClient;
use super::live_buy::{PendingLiveBuy, timed_bed_click_at};
use anyhow::{Context, Result};
use saf_cofl::CoflExecuteInstruction;
use saf_core::ports::{CoflClient, Notification, NotificationKind, Notifier};
use saf_core::{
    AccountId, FlipEvent, FlipOutcome, Humanizer, RuntimeDirective, RuntimeOutcome, RuntimeSession,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

static PASSIVE_COFL_CHAT_ENVELOPES: AtomicU64 = AtomicU64::new(0);
static PASSIVE_COFL_CHAT_CATEGORIES: LazyLock<Mutex<BTreeMap<&'static str, u64>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));
pub(in crate::live_runtime) const COFL_AUTH_LINK_NOTIFY_DELAY: Duration = Duration::from_secs(8);

fn next_passive_chat_category_count(category: &'static str) -> u64 {
    match PASSIVE_COFL_CHAT_CATEGORIES.lock() {
        Ok(mut categories) => {
            let count = categories.entry(category).or_insert(0);
            *count += 1;
            *count
        }
        Err(_) => 0,
    }
}

pub(in crate::live_runtime) struct LiveCoflStream {
    pub(in crate::live_runtime) account: AccountId,
    pub(in crate::live_runtime) client: Arc<LiveCoflClient>,
    pub(in crate::live_runtime) tracked_flips: Arc<LiveTrackedFlipProvider>,
    pub(in crate::live_runtime) market_actions: MarketActionMode,
    pub(in crate::live_runtime) allow_execute_chat: bool,
    pub(in crate::live_runtime) all_flip_notifier: Option<Arc<dyn Notifier>>,
    pub(in crate::live_runtime) bed_click_offset: Duration,
    pub(in crate::live_runtime) bed_spam: bool,
    pub(in crate::live_runtime) bed_click_delay: Duration,
    pub(in crate::live_runtime) humanizer: Arc<Humanizer>,
    pub(in crate::live_runtime) pending_live_buys: Arc<Mutex<BTreeMap<AccountId, PendingLiveBuy>>>,
    pub(in crate::live_runtime) notified_auth_links: Arc<Mutex<BTreeSet<String>>>,
    pub(in crate::live_runtime) pending_auth_links: Arc<Mutex<BTreeMap<String, Instant>>>,
}

impl LiveCoflStream {
    pub(in crate::live_runtime) async fn poll_once(
        &self,
        session: &RuntimeSession,
        stats: &LiveStatsProvider,
        market_ready: bool,
    ) -> Result<bool> {
        match self.client.ensure_connected(false).await {
            Ok(true) => {}
            Ok(false) => return Ok(false),
            Err(error) => {
                tracing::warn!(
                    account = %self.account,
                    error = %error,
                    "Cofl websocket connect failed; reconnect scheduled"
                );
                return Ok(false);
            }
        }
        if let Some(retry_link) = self.client.reconnect_silent_open().await {
            tracing::warn!(
                account = %self.account,
                link = %saf_cofl::redact_cofl_socket_link(&retry_link),
                "Cofl websocket opened silently; reconnect scheduled"
            );
            return Ok(false);
        }
        if market_ready {
            self.upload_initial_scoreboard_if_available(stats).await;
            self.flush_deferred_inventory_upload(session).await?;
        }

        match tokio::time::timeout(COFL_READ_TIMEOUT, self.client.next_envelope()).await {
            Ok(Ok(Some(envelope))) => {
                let handled = self
                    .handle_envelope_best_effort(session, stats, market_ready, envelope)
                    .await;
                self.flush_pending_auth_links_best_effort(session).await;
                self.client.request_startup_account_info_if_needed().await;
                Ok(handled)
            }
            Ok(Ok(None)) => {
                self.flush_pending_auth_links_best_effort(session).await;
                Ok(false)
            }
            Ok(Err(error)) => {
                tracing::warn!(
                    account = %self.account,
                    error = %error,
                    "Cofl websocket read failed; reconnect scheduled"
                );
                Ok(false)
            }
            Err(_) => {
                self.flush_pending_auth_links_best_effort(session).await;
                Ok(false)
            }
        }
    }

    async fn upload_initial_scoreboard_if_available(&self, stats: &LiveStatsProvider) {
        let lines = match stats.latest_scoreboard(&self.account) {
            Ok(Some(lines)) => lines,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(
                    account = %self.account,
                    error = %error,
                    "failed to read cached scoreboard for Cofl open upload"
                );
                return;
            }
        };
        self.client
            .upload_initial_scoreboard_if_needed(&lines)
            .await;
    }

    pub(in crate::live_runtime) async fn upload_scoreboard_if_needed(&self, lines: &[String]) {
        self.client.upload_initial_scoreboard_if_needed(lines).await;
    }

    async fn handle_envelope(
        &self,
        session: &RuntimeSession,
        stats: &LiveStatsProvider,
        market_ready: bool,
        envelope: saf_cofl::CoflEnvelope,
    ) -> Result<()> {
        let mut observed = false;
        if envelope.settings_unavailable() {
            self.client.mark_settings_unloaded();
        }
        if let Some(flip) = envelope.parse_flip() {
            observed = true;
            self.log_received_flip(&flip);
            self.tracked_flips.record_flip(&self.account, &flip)?;
            self.notify_all_flip_best_effort(&flip).await;
            let settings_loaded = self.client.settings_loaded();
            if let Some(reason) = self.client.flip_safety_violation(&flip) {
                tracing::warn!(
                    account = %self.account,
                    item = %flip.item_name,
                    auction_id = ?flip.auction_id,
                    starting_bid = flip.starting_bid,
                    target = flip.target,
                    profit = flip.profit,
                    profit_percentage = flip.profit_percentage,
                    reason = %reason,
                    "blocked Cofl flip by local safety floor"
                );
            } else if self.market_actions.allows_market_actions() && market_ready && settings_loaded
            {
                let pending_flip = flip.clone();
                let outcome = session
                    .process_flip(&self.account, flip)
                    .await
                    .with_context(|| format!("processing Cofl flip for {}", self.account))?;
                self.log_flip_outcome(&outcome);
                self.remember_pending_live_buy(&outcome, &pending_flip)?;
            } else {
                tracing::info!(
                    account = %self.account,
                    market_ready,
                    settings_loaded,
                    market_actions = ?self.market_actions,
                    "tracked Cofl flip without opening auction because live market actions are disabled, gated, or Cofl settings are not loaded"
                );
            }
        }
        if let Some(update) = envelope.telemetry_update() {
            observed = true;
            stats
                .record_cofl_telemetry(&self.account, &update)
                .map_err(anyhow::Error::from)?;
        }
        if let Some(settings) = envelope.settings_summary() {
            observed = true;
            self.client.mark_settings_loaded();
            self.client.mark_startup_account_info_accepted();
            self.client.replace_settings_summary(&settings);
            self.clear_pending_auth_links();
            tracing::info!(
                account = %self.account,
                min_profit = ?settings.min_profit,
                min_volume = ?settings.min_volume,
                min_profit_percent = ?settings.min_profit_percent,
                max_flip_items_in_inventory = ?settings.max_flip_items_in_inventory,
                using = ?settings.using,
                "loaded Cofl flipper settings"
            );
        }
        if envelope.settings_json_available() {
            observed = true;
            self.client.mark_settings_loaded();
            self.client.mark_startup_account_info_accepted();
            self.clear_pending_auth_links();
            if let Some(settings) = envelope.settings_json_summary() {
                self.client.replace_settings_summary(&settings);
                tracing::info!(
                    account = %self.account,
                    min_profit = ?settings.min_profit,
                    min_volume = ?settings.min_volume,
                    min_profit_percent = ?settings.min_profit_percent,
                    max_flip_items_in_inventory = ?settings.max_flip_items_in_inventory,
                    using = ?settings.using,
                    "loaded Cofl flipper settings from JSON envelope"
                );
                if self.client.should_request_text_settings_summary() {
                    self.send_cofl_command_best_effort("/cofl get").await;
                }
            } else {
                tracing::info!(
                    account = %self.account,
                    "loaded Cofl flipper settings from JSON envelope"
                );
                if self.client.should_request_text_settings_summary() {
                    self.send_cofl_command_best_effort("/cofl get").await;
                }
            }
        }
        if let Some(mutation) = envelope.settings_mutation() {
            observed = true;
            self.client.apply_settings_mutation(&mutation);
            tracing::info!(
                account = %self.account,
                min_profit = ?mutation.min_profit,
                min_profit_percent = ?mutation.min_profit_percent,
                "applied Cofl setting update"
            );
        }
        if envelope.account_info_acceleration_ack() {
            observed = true;
            self.client.mark_startup_account_info_accepted();
            tracing::info!(
                account = %self.account,
                "Cofl accepted account info and started speeding up flips"
            );
        }
        let auth_links = envelope.auth_links();
        if !auth_links.is_empty() {
            observed = true;
            self.queue_auth_links_best_effort(&auth_links);
        }
        if let Some(command) = envelope.logged_out_settings_recovery_command() {
            observed = true;
            self.send_cofl_command_best_effort(command).await;
        }
        if let Some(pattern) = envelope.privacy_chat_regex() {
            observed = true;
            self.client.set_privacy_chat_regex(&pattern);
        }
        if envelope.is_inventory_request() {
            observed = true;
            self.handle_inventory_request(session, market_ready).await?;
        }
        if envelope.passive_chat_category() == Some("max_flip_items_in_inventory") {
            observed = true;
            self.log_passive_envelope(&envelope);
            self.handle_inventory_request(session, market_ready).await?;
        }
        if let Some(instruction) =
            envelope.execute_instruction(self.account.as_str(), self.client.session_id())
        {
            observed = true;
            self.handle_execute_instruction(session, instruction)
                .await?;
        }
        if !observed {
            self.log_passive_envelope(&envelope);
        }
        Ok(())
    }

    fn log_passive_envelope(&self, envelope: &saf_cofl::CoflEnvelope) {
        if matches!(envelope.kind.as_str(), "writeToChat" | "chatMessage") {
            let passive_count = PASSIVE_COFL_CHAT_ENVELOPES.fetch_add(1, Ordering::Relaxed) + 1;
            let category = envelope.passive_chat_category().unwrap_or("uncategorized");
            let category_count = next_passive_chat_category_count(category);
            if passive_count > 20 && passive_count % 1_000 != 0 && category_count % 500 != 0 {
                return;
            }
            let excerpt = matches!(
                category,
                "unknown_text" | "settings_blocked" | "max_flip_items_in_inventory"
            )
            .then(|| envelope.passive_chat_excerpt(140))
            .flatten()
            .unwrap_or_default();
            tracing::info!(
                account = %self.account,
                kind = %envelope.kind,
                passive_count,
                category,
                category_count,
                excerpt = %excerpt,
                "received passive Cofl chat envelope"
            );
            return;
        }

        tracing::info!(
            account = %self.account,
            kind = %envelope.kind,
            "received passive Cofl envelope"
        );
    }

    fn log_received_flip(&self, flip: &FlipEvent) {
        let auction_id = flip
            .auction_id
            .as_ref()
            .map(ToString::to_string)
            .unwrap_or_else(|| "unknown".to_string());
        if let Some(reason) = &flip.invalid_reason {
            tracing::info!(
                account = %self.account,
                auction_id = %auction_id,
                item = %flip.item_name,
                finder = %flip.finder,
                reason = %reason,
                "received invalid Cofl flip"
            );
            return;
        }
        tracing::info!(
            account = %self.account,
            auction_id = %auction_id,
            item = %flip.item_name,
            finder = %flip.finder,
            starting_bid = flip.starting_bid,
            target = flip.target,
            profit = flip.profit,
            profit_percentage = flip.profit_percentage,
            purchase_at_ms = ?flip.purchase_at_ms,
            "received actionable Cofl flip"
        );
    }

    fn log_flip_outcome(&self, outcome: &RuntimeOutcome) {
        let RuntimeOutcome::FlipProcessed { outcome, .. } = outcome else {
            tracing::debug!(
                account = %self.account,
                outcome = ?outcome,
                "Cofl flip produced non-flip runtime outcome"
            );
            return;
        };
        match outcome {
            FlipOutcome::OpenedAuction {
                auction_id,
                item_name,
                starting_bid,
                purchase_at_ms,
                profile,
                skip,
            } => tracing::info!(
                account = %self.account,
                auction_id = %auction_id,
                item = %item_name,
                starting_bid = *starting_bid,
                profile = %profile,
                skip_reasons = ?skip.reasons,
                purchase_at_ms = ?purchase_at_ms,
                "opened Cofl flip auction"
            ),
            FlipOutcome::IgnoredBlocked { item_name, reason } => tracing::info!(
                account = %self.account,
                item = %item_name,
                reason = %reason,
                "ignored Cofl flip because it matched the buy blacklist"
            ),
            FlipOutcome::IgnoredSkipped {
                auction_id,
                item_name,
                starting_bid,
                purchase_at_ms,
                skip,
            } => tracing::info!(
                account = %self.account,
                auction_id = %auction_id,
                item = %item_name,
                starting_bid = *starting_bid,
                skip_reasons = ?skip.reasons,
                purchase_at_ms = ?purchase_at_ms,
                "ignored Cofl flip because it matched skip policy"
            ),
            FlipOutcome::IgnoredInvalid { item_name, reason } => tracing::info!(
                account = %self.account,
                item = %item_name,
                reason = %reason,
                "ignored invalid Cofl flip"
            ),
        }
    }

    async fn send_cofl_command_best_effort(&self, command: &str) {
        if let Err(error) = self.client.send_command(&self.account, command).await {
            tracing::warn!(
                account = %self.account,
                command = %command,
                error = %error,
                "failed to send Cofl recovery command"
            );
        }
    }

    async fn handle_inventory_request(
        &self,
        session: &RuntimeSession,
        market_ready: bool,
    ) -> Result<()> {
        if !market_ready {
            self.client.defer_inventory_upload();
            tracing::debug!(
                account = %self.account,
                "deferred Cofl inventory upload until the account is market-ready"
            );
            return Ok(());
        }
        let Some(message) = self.inventory_upload_message(session).await? else {
            tracing::debug!(
                account = %self.account,
                "Cofl requested inventory but no live inventory snapshot was available"
            );
            return Ok(());
        };
        match self.client.send_raw_if_connected(&message).await {
            Ok(true) => tracing::info!(
                account = %self.account,
                "uploaded inventory snapshot to Cofl"
            ),
            Ok(false) => tracing::debug!(
                account = %self.account,
                "Cofl requested inventory before the websocket was connected"
            ),
            Err(error) => tracing::warn!(
                account = %self.account,
                error = %error,
                "failed to upload inventory to Cofl"
            ),
        }
        Ok(())
    }

    pub(in crate::live_runtime) async fn flush_deferred_inventory_upload(
        &self,
        session: &RuntimeSession,
    ) -> Result<()> {
        if !self.client.take_deferred_inventory_upload() {
            return Ok(());
        }
        self.handle_inventory_request(session, true).await
    }

    pub(in crate::live_runtime) async fn inventory_upload_message(
        &self,
        session: &RuntimeSession,
    ) -> Result<Option<String>> {
        match session
            .execute_directive(RuntimeDirective::ShowInventory {
                account: self.account.clone(),
            })
            .await
            .map_err(anyhow::Error::from)?
        {
            RuntimeOutcome::InventorySnapshot { snapshot } => {
                log_inventory_snapshot(&snapshot, "cofl_inventory_upload");
                Ok(Some(saf_cofl::encode_inventory_snapshot_upload(&snapshot)?))
            }
            RuntimeOutcome::Planned { .. } => Ok(None),
            other => {
                tracing::debug!(
                    account = %self.account,
                    outcome = ?other,
                    "Cofl inventory request returned a non-inventory runtime outcome"
                );
                Ok(None)
            }
        }
    }

    fn remember_pending_live_buy(&self, outcome: &RuntimeOutcome, flip: &FlipEvent) -> Result<()> {
        if let RuntimeOutcome::FlipProcessed {
            outcome:
                FlipOutcome::OpenedAuction {
                    auction_id,
                    purchase_at_ms,
                    ..
                },
            ..
        } = outcome
        {
            let click_at = (!self.bed_spam)
                .then(|| timed_bed_click_at(*purchase_at_ms, self.bed_click_offset))
                .flatten();
            let created_at_ms = super::super::now_ms();
            let profit_for_bias = flip.profit.max(flip.target - flip.starting_bid);
            let buy_action_retry_delay =
                Duration::from_millis(self.humanizer.buy_reaction_ms(profit_for_bias));
            // Independent profit-biased reaction draw that gates the FIRST click
            // once the auction window is observed (see live_buy.rs reaction gate).
            let buy_reaction_delay =
                Duration::from_millis(self.humanizer.buy_reaction_ms(profit_for_bias));
            let timed_bed_click_delay = self.humanizer.click_stream_delay();
            self.pending_live_buys
                .lock()
                .map_err(|_| anyhow::anyhow!("pending live-buy lock poisoned"))?
                .insert(
                    self.account.clone(),
                    PendingLiveBuy {
                        auction_id: auction_id.clone(),
                        expected_price: flip.starting_bid,
                        target_price: flip.target,
                        created_at: Instant::now(),
                        created_at_ms,
                        purchase_at_ms: *purchase_at_ms,
                        click_at,
                        bed_spam: self.bed_spam,
                        bed_click_delay: self.bed_click_delay,
                        timed_bed_click_delay,
                        buy_action_retry_delay,
                        buy_reaction_delay,
                        window_seen_at: None,
                        bed_spam_until: None,
                        timed_bed_clicks: 0,
                        timed_bed_cleanup_at: None,
                        action_clicks: 0,
                        action_clicked_at: None,
                        last_click: None,
                        last_attempt: Some(Instant::now()),
                    },
                );
        }
        Ok(())
    }

    async fn notify_all_flip_best_effort(&self, flip: &FlipEvent) {
        if !flip.is_valid() {
            return;
        }
        let Some(notifier) = &self.all_flip_notifier else {
            return;
        };
        let notification = all_flip_notification(&self.account, flip);
        if let Err(error) = notifier.notify(notification).await {
            tracing::warn!(
                account = %self.account,
                error = %error,
                "send-all-flips webhook notification failed"
            );
        }
    }

    fn queue_auth_links_best_effort(&self, links: &[String]) {
        if self.client.settings_loaded() {
            return;
        }
        match self.pending_auth_links.lock() {
            Ok(mut pending) => {
                let now = Instant::now();
                for link in links {
                    pending.entry(link.clone()).or_insert(now);
                }
            }
            Err(_) => tracing::warn!(
                account = %self.account,
                "Cofl auth-link pending lock poisoned; auth notification may be delayed"
            ),
        }
    }

    pub(in crate::live_runtime) async fn flush_pending_auth_links_best_effort(
        &self,
        session: &RuntimeSession,
    ) {
        if self.client.settings_loaded() {
            self.clear_pending_auth_links();
            return;
        }
        let links = match self.pending_auth_links.lock() {
            Ok(mut pending) => {
                let now = Instant::now();
                let due = pending
                    .iter()
                    .filter(|&(_, first_seen)| {
                        now.saturating_duration_since(*first_seen) >= COFL_AUTH_LINK_NOTIFY_DELAY
                    })
                    .map(|(link, _)| link.clone())
                    .collect::<Vec<_>>();
                for link in &due {
                    pending.remove(link);
                }
                due
            }
            Err(_) => {
                tracing::warn!(
                    account = %self.account,
                    "Cofl auth-link pending lock poisoned; cannot flush auth notification"
                );
                Vec::new()
            }
        };
        if links.is_empty() {
            return;
        }
        self.notify_auth_links_best_effort(session, &links).await;
    }

    fn clear_pending_auth_links(&self) {
        match self.pending_auth_links.lock() {
            Ok(mut pending) => pending.clear(),
            Err(_) => tracing::warn!(
                account = %self.account,
                "Cofl auth-link pending lock poisoned while clearing loaded settings state"
            ),
        }
    }

    async fn notify_auth_links_best_effort(&self, session: &RuntimeSession, links: &[String]) {
        let mut fresh_links = Vec::new();
        match self.notified_auth_links.lock() {
            Ok(mut notified) => {
                for link in links {
                    if notified.insert(link.clone()) {
                        fresh_links.push(link.clone());
                    }
                }
            }
            Err(_) => {
                tracing::warn!(
                    account = %self.account,
                    "Cofl auth-link notification lock poisoned; sending auth link anyway"
                );
                fresh_links.extend(links.iter().cloned());
            }
        }

        for link in fresh_links {
            notify_operator_best_effort(
                session,
                Notification::new(
                    NotificationKind::LoginRequired,
                    "SkyCofl Login Required",
                    format!(
                        "Authorize `{}` for the persisted SkyCofl session:\n{}\nKeep the runtime running while the login finishes.",
                        self.account, link
                    ),
                    Some(self.account.clone()),
                )
                .with_thumbnail(crate::player_head::account_head_thumbnail_url(
                    self.account.as_str(),
                )),
            )
            .await;
        }
    }

    pub(in crate::live_runtime) async fn handle_envelope_best_effort(
        &self,
        session: &RuntimeSession,
        stats: &LiveStatsProvider,
        market_ready: bool,
        envelope: saf_cofl::CoflEnvelope,
    ) -> bool {
        match self
            .handle_envelope(session, stats, market_ready, envelope)
            .await
        {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(
                    account = %self.account,
                    error = %error,
                    "Cofl envelope handling failed; continuing runtime loop"
                );
                false
            }
        }
    }

    pub(in crate::live_runtime) async fn handle_execute_instruction(
        &self,
        session: &RuntimeSession,
        instruction: CoflExecuteInstruction,
    ) -> Result<()> {
        match instruction {
            CoflExecuteInstruction::SwitchSocket { link } => {
                tracing::info!(
                    account = %self.account,
                    link = %saf_cofl::redact_cofl_socket_link(&link),
                    "Cofl requested websocket switch"
                );
                self.client.switch_link(link).await;
            }
            CoflExecuteInstruction::SendCoflCommand { command } => {
                self.client
                    .send_command(&self.account, &command)
                    .await
                    .map_err(anyhow::Error::from)?;
            }
            CoflExecuteInstruction::BlockedChat { command } => {
                if self.allow_execute_chat {
                    tracing::warn!(
                        account = %self.account,
                        command = %command.chars().take(120).collect::<String>(),
                        "forwarding non-Cofl execute command from Cofl socket because SAF_ALLOW_COFL_EXECUTE_CHAT is enabled"
                    );
                    session
                        .execute_directive(RuntimeDirective::SendMinecraftChat {
                            account: self.account.clone(),
                            message: command,
                        })
                        .await
                        .map_err(anyhow::Error::from)?;
                    return Ok(());
                }
                let redacted_command = command.chars().take(120).collect::<String>();
                if is_expected_blocked_chat_command(&command) {
                    tracing::debug!(
                        account = %self.account,
                        command = %redacted_command,
                        "blocked expected non-Cofl execute command from Cofl socket"
                    );
                } else {
                    tracing::warn!(
                        account = %self.account,
                        command = %redacted_command,
                        "blocked non-Cofl execute command from Cofl socket"
                    );
                }
            }
        }
        Ok(())
    }
}

pub(in crate::live_runtime) fn is_expected_blocked_chat_command(command: &str) -> bool {
    command
        .trim_start()
        .to_ascii_lowercase()
        .starts_with("/tip ")
}
