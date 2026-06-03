use crate::inbox::{InboxCommandResult, process_inbox};
use anyhow::Result;
use saf_core::AccountId;
use saf_core::RuntimeSession;
use saf_core::gui::WindowSnapshot;
use saf_core::{RuntimeDirective, RuntimeOutcome};
use std::collections::BTreeSet;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::inventory_logging::log_inventory_snapshot;
use super::{DryRunMarketAction, LiveRuntime, MARKET_WINDOW_SETTLE_DELAY, RunLiveReport};

impl LiveRuntime {
    pub async fn poll_once(&mut self) -> Result<Vec<InboxCommandResult>> {
        let raw = match self.inbox.read_new().await {
            Ok(raw) => raw,
            Err(error) => {
                tracing::warn!(
                    path = %self.options.command_inbox.display(),
                    error = %error,
                    "failed to read command inbox; skipping inbox processing for this poll"
                );
                String::new()
            }
        };
        let results = if raw.trim().is_empty() {
            Vec::new()
        } else {
            let results = process_inbox(&self.session, &raw).await;
            self.processed_commands += results.len();
            for result in &results {
                log_inbox_result(result);
            }
            results
        };
        self.reconcile_inactive_accounts()?;
        if self.is_halted() {
            // Operator panic stop (Stop All) or runtime pause (.saf-paused):
            // keep the controller responsive but perform NO account or market
            // work — no flips, buys, listings, banking, drains, or reconnects —
            // until an explicit start clears the halt.
            self.poll_discord_gateway_once().await?;
            return Ok(results);
        }
        self.poll_minecraft_once().await?;
        self.process_pending_live_buys_once().await?;
        self.poll_cofl_once().await?;
        self.poll_discord_gateway_once().await?;
        self.poll_minecraft_once().await?;
        self.drive_island_checks_once().await?;
        self.drive_startup_profile_scans_once().await?;
        self.expire_startup_profile_scans_once().await;
        self.drive_startup_cookie_scans_once().await?;
        self.expire_startup_cookie_scans_once().await;
        self.drive_auto_cookies_once().await?;
        self.drive_forced_cookies_once().await?;
        self.process_pending_live_buys_once().await?;
        self.drain_pending_purchase_relists().await?;
        self.drain_pending_transfer_followups().await?;
        self.drain_pending_claimed_bid_relists().await?;
        self.drain_pending_completed_entries().await?;
        self.drain_deferred_queue_entries().await?;
        self.queue_reconciliation_polls_once().await?;
        self.process_market_queue_once().await?;
        Ok(results)
    }

    async fn poll_cofl_once(&mut self) -> Result<()> {
        #[cfg(feature = "live-cofl")]
        {
            let running = self.running_account_set();
            for stream in &self.cofl_streams {
                if !running.contains(&stream.account) {
                    continue;
                }
                if stream
                    .poll_once(
                        &self.session,
                        &self.stats,
                        self.account_market_ready(&stream.account),
                    )
                    .await?
                {
                    self.processed_cofl_envelopes += 1;
                }
            }
            self.cofl_connected = self.cofl_connected_count().await;
        }
        Ok(())
    }

    #[cfg(not(feature = "live-cofl"))]
    pub(in crate::live_runtime) fn remove_pending_live_buy(
        &self,
        _account: &AccountId,
    ) -> Result<()> {
        Ok(())
    }

    #[cfg(not(feature = "live-cofl"))]
    pub(in crate::live_runtime) async fn process_pending_live_buys_once(&mut self) -> Result<()> {
        Ok(())
    }

    fn reconcile_inactive_accounts(&mut self) -> Result<()> {
        let running = self.running_account_set();
        self.active_windows
            .lock()
            .map_err(|_| anyhow::anyhow!("active window lock poisoned"))?
            .retain(|account, _| running.contains(account));
        self.active_window_received_at
            .lock()
            .map_err(|_| anyhow::anyhow!("active window received timestamp lock poisoned"))?
            .retain(|account, _| running.contains(account));
        self.active_window_observed_at
            .lock()
            .map_err(|_| anyhow::anyhow!("active window timestamp lock poisoned"))?
            .retain(|account, _| running.contains(account));
        self.market_settle_jitter
            .lock()
            .map_err(|_| anyhow::anyhow!("market settle jitter lock poisoned"))?
            .retain(|account, _| running.contains(account));
        self.deferred_minecraft_events
            .lock()
            .map_err(|_| anyhow::anyhow!("deferred Minecraft event lock poisoned"))?
            .retain(|account, _| running.contains(account));
        self.pending_market_steps
            .retain(|account, _| running.contains(account));
        self.minecraft_ready_accounts
            .retain(|account| running.contains(account));
        #[cfg(feature = "live-cofl")]
        self.pending_live_buys
            .lock()
            .map_err(|_| anyhow::anyhow!("pending live-buy lock poisoned"))?
            .retain(|account, _| running.contains(account));
        for (account, state) in &mut self.island_states {
            if !running.contains(account) {
                state.mark_disconnected();
            }
        }
        Ok(())
    }

    pub(in crate::live_runtime) fn running_accounts(&self) -> Vec<AccountId> {
        self.session
            .running_accounts()
            .into_iter()
            .filter_map(AccountId::new)
            .collect()
    }

    pub(in crate::live_runtime) fn running_account_set(&self) -> BTreeSet<AccountId> {
        self.running_accounts().into_iter().collect()
    }

    /// True when the runtime must not perform any account or market work: either
    /// the operator latched a Stop All (`halted`) or a pause is active via
    /// `SAF_START_PAUSED` / the `.saf-paused` file (re-checked every poll, so it
    /// is a live out-of-band kill switch).
    pub(in crate::live_runtime) fn is_halted(&self) -> bool {
        self.halted.load(Ordering::SeqCst) || super::lifecycle::startup_paused()
    }

    pub fn report(&self) -> RunLiveReport {
        RunLiveReport {
            accounts: self.accounts.clone(),
            running_accounts: self.running_accounts(),
            command_inbox: self.options.command_inbox.display().to_string(),
            state_base_dir: self.options.state_base_dir.display().to_string(),
            market_actions: self.options.market_actions,
            processed_commands: self.processed_commands,
            processed_cofl_envelopes: self.processed_cofl_envelopes,
            processed_minecraft_events: self.processed_minecraft_events,
            processed_queue_steps: self.processed_queue_steps,
            completed_queue_entries: self.completed_queue_entries,
            dry_run_market_actions: self.queue.dry_run_records().len(),
            cofl_connections: self.cofl_connections,
            cofl_connected: self.cofl_connected,
            discord_started: self.discord_started,
        }
    }

    pub fn session(&self) -> Arc<RuntimeSession> {
        self.session.clone()
    }

    pub fn remember_window_snapshot(
        &self,
        account: AccountId,
        window: WindowSnapshot,
    ) -> Result<()> {
        let received_at = Instant::now();
        if let Err(error) = self.stats.record_window_snapshot(&account, &window) {
            tracing::warn!(
                account = %account,
                error = %error,
                "failed to record window stats; caching window snapshot anyway"
            );
        }
        let changed = {
            let mut active_windows = self
                .active_windows
                .lock()
                .map_err(|_| anyhow::anyhow!("active window lock poisoned"))?;
            let changed = active_windows
                .get(&account)
                .is_none_or(|active| active != &window);
            active_windows.insert(account.clone(), window);
            changed
        };
        self.active_window_received_at
            .lock()
            .map_err(|_| anyhow::anyhow!("active window received timestamp lock poisoned"))?
            .insert(account.clone(), received_at);
        if changed {
            let observed_at = Instant::now();
            self.active_window_observed_at
                .lock()
                .map_err(|_| anyhow::anyhow!("active window timestamp lock poisoned"))?
                .insert(account.clone(), observed_at);
            // Resample the jittered settle window for this account so menu/
            // inter-click pacing varies every time instead of sitting on a fixed
            // ~300 ms beat. Sampled from the per-account humanizer's
            // default-action delay; falls back to the fixed delay if no
            // per-account humanizer is wired (e.g. cofl-less builds).
            let settle = self
                .account_humanizers
                .get(&account)
                .map(|humanizer| humanizer.default_action_delay())
                .unwrap_or(MARKET_WINDOW_SETTLE_DELAY);
            self.market_settle_jitter
                .lock()
                .map_err(|_| anyhow::anyhow!("market settle jitter lock poisoned"))?
                .insert(account, (observed_at, settle));
        }
        Ok(())
    }

    #[cfg(feature = "live-cofl")]
    pub async fn cofl_connected_count(&self) -> usize {
        let mut connected = 0;
        for stream in &self.cofl_streams {
            if stream.client.is_connected().await {
                connected += 1;
            }
        }
        connected
    }

    #[cfg(not(feature = "live-cofl"))]
    pub async fn cofl_connected_count(&self) -> usize {
        0
    }

    pub fn dry_run_records(&self) -> Vec<DryRunMarketAction> {
        self.queue.dry_run_records()
    }

    pub async fn shutdown(&self) {
        self.shutdown_runtime().await;
    }

    #[cfg(feature = "live-discord")]
    pub fn discord_gateway_task_finished(&self) -> bool {
        self.discord_task
            .as_ref()
            .is_some_and(tokio::task::JoinHandle::is_finished)
    }

    #[cfg(not(feature = "live-discord"))]
    pub fn discord_gateway_task_finished(&self) -> bool {
        false
    }
}

fn log_inbox_result(result: &InboxCommandResult) {
    match result {
        InboxCommandResult::Processed { index, outcome } => {
            tracing::info!(
                index,
                outcome = runtime_outcome_name(outcome),
                directive = outcome_directive_summary(outcome).as_deref().unwrap_or(""),
                "processed command inbox entry"
            );
            if let RuntimeOutcome::GuiSlotDiagnostics { diagnostics } = outcome {
                for window in &diagnostics.windows {
                    let slots = window
                        .slots
                        .iter()
                        .take(54)
                        .map(|slot| {
                            format!(
                                "{}:{}:{}:{}",
                                slot.slot,
                                slot.name,
                                slot.display_name,
                                slot.lore.join(" | ")
                            )
                        })
                        .collect::<Vec<_>>();
                    tracing::info!(
                        account = %diagnostics.account,
                        target = diagnostics.target.as_deref().unwrap_or("current"),
                        window = %window.title,
                        slot_count = window.slots.len(),
                        slots = ?slots,
                        "gui slot diagnostics"
                    );
                }
            }
            if let RuntimeOutcome::InventorySnapshot { snapshot } = outcome {
                log_inventory_snapshot(snapshot, "command_inbox");
            }
        }
        InboxCommandResult::Failed { index, error } => {
            tracing::warn!(index, error = %error, "command inbox entry failed");
        }
        InboxCommandResult::Invalid { index, error, .. } => {
            tracing::warn!(index, error = %error, "command inbox entry is invalid");
        }
    }
}

fn outcome_directive_summary(outcome: &RuntimeOutcome) -> Option<String> {
    match outcome {
        RuntimeOutcome::Planned { directive }
        | RuntimeOutcome::Executed { directive }
        | RuntimeOutcome::Queued { directive, .. } => Some(runtime_directive_summary(directive)),
        _ => None,
    }
}

fn runtime_directive_summary(directive: &RuntimeDirective) -> String {
    match directive {
        RuntimeDirective::SendMinecraftChat { account, message } => {
            format!("{account} chat {}", command_excerpt(message))
        }
        RuntimeDirective::SendCoflCommand { account, command } => {
            format!("{account} cofl {}", command_excerpt(command))
        }
        RuntimeDirective::QueueState {
            account,
            state,
            priority,
            ..
        } => format!("{account} queue {} priority={priority}", state.as_str()),
        RuntimeDirective::StartAccounts { accounts } => {
            format!("start {} account(s)", accounts.len())
        }
        RuntimeDirective::StopAccounts { account } => account
            .as_ref()
            .map(|account| format!("stop {account}"))
            .unwrap_or_else(|| "stop all accounts".to_string()),
        RuntimeDirective::DiagnoseSlots { account, target } => format!(
            "{account} slotdiag {}",
            target.as_deref().unwrap_or("current")
        ),
        RuntimeDirective::ShowInventory { account } => format!("{account} inventory"),
        RuntimeDirective::ShowQueue { account } => format!("{account} queue snapshot"),
        RuntimeDirective::ClearQueue { account } => format!("{account} clear queue"),
        RuntimeDirective::CancelQueueEntry { account, index } => {
            format!("{account} cancel queue entry {index}")
        }
        RuntimeDirective::ClearAllQueues => "clear all queues".to_string(),
        RuntimeDirective::ClearData { account } => format!("{account} clear data"),
        RuntimeDirective::CheckBids { account } => format!("{account} check bids"),
        RuntimeDirective::Bank { account, .. } => format!("{account} bank"),
        RuntimeDirective::TransferCoins { from, to, .. } => {
            format!("transfer {from} -> {to}")
        }
        RuntimeDirective::ExternalBuy {
            account,
            auction_id,
        } => {
            format!("{account} external buy {}", command_excerpt(auction_id))
        }
        RuntimeDirective::TrackedListFlip {
            account,
            auction_id,
            ..
        } => format!("{account} tracked list {}", command_excerpt(auction_id)),
        RuntimeDirective::SellInventory { account, .. } => format!("{account} sell inventory"),
        RuntimeDirective::QueueDelistAll { account } => format!("{account} delist all"),
        RuntimeDirective::ScheduleAccount {
            account,
            action,
            delay_ms,
        } => format!("{account} schedule {action:?} delay_ms={delay_ms}"),
        RuntimeDirective::BlacklistCommand { account, .. } => format!("{account} blacklist"),
        RuntimeDirective::TestWebhook { account } => format!("{account} test webhook"),
        RuntimeDirective::Cookie { account } => format!("{account} cookie"),
        RuntimeDirective::ShowStats { account } => format!("{account} stats"),
        RuntimeDirective::ShowProfit { account } => format!("{account} profit"),
        RuntimeDirective::ShowPing { account } => format!("{account} ping"),
        RuntimeDirective::ShowUsers => "users".to_string(),
        RuntimeDirective::ShowGlobalStats => "global stats".to_string(),
        RuntimeDirective::ShowConnections => "connections".to_string(),
        RuntimeDirective::ShowLogs { lines } => format!("logs lines={lines}"),
        RuntimeDirective::UnknownTerminalCommand {
            account, command, ..
        } => format!("{account} unknown {}", command_excerpt(command)),
    }
}

fn command_excerpt(value: &str) -> String {
    const MAX_CHARS: usize = 96;
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(MAX_CHARS)
        .collect()
}

fn runtime_outcome_name(outcome: &RuntimeOutcome) -> &'static str {
    match outcome {
        RuntimeOutcome::Planned { .. } => "planned",
        RuntimeOutcome::Executed { .. } => "executed",
        RuntimeOutcome::FlipProcessed { .. } => "flipProcessed",
        RuntimeOutcome::Queued { .. } => "queued",
        RuntimeOutcome::QueueSnapshot { .. } => "queueSnapshot",
        RuntimeOutcome::QueueCleared { .. } => "queueCleared",
        RuntimeOutcome::QueueEntryCancelled { .. } => "queueEntryCancelled",
        RuntimeOutcome::QueuesCleared { .. } => "queuesCleared",
        RuntimeOutcome::SavedDataCleared { .. } => "savedDataCleared",
        RuntimeOutcome::BlacklistApplied { .. } => "blacklistApplied",
        RuntimeOutcome::StatsSnapshot { .. } => "statsSnapshot",
        RuntimeOutcome::ProfitSnapshot { .. } => "profitSnapshot",
        RuntimeOutcome::PingSnapshot { .. } => "pingSnapshot",
        RuntimeOutcome::UsersSnapshot { .. } => "usersSnapshot",
        RuntimeOutcome::GlobalStatsSnapshot { .. } => "globalStatsSnapshot",
        RuntimeOutcome::ConnectionsSnapshot { .. } => "connectionsSnapshot",
        RuntimeOutcome::LogSnapshot { .. } => "logSnapshot",
        RuntimeOutcome::InventorySnapshot { .. } => "inventorySnapshot",
        RuntimeOutcome::InventoryListingsQueued { .. } => "inventoryListingsQueued",
        RuntimeOutcome::DelistAllQueued { .. } => "delistAllQueued",
        RuntimeOutcome::GuiSlotDiagnostics { .. } => "guiSlotDiagnostics",
        RuntimeOutcome::AccountScheduled { .. } => "accountScheduled",
    }
}
