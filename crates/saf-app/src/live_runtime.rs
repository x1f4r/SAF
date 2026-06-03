use crate::blacklist_config::FileBlacklistStore;
use crate::inbox::FileLogReader;
#[cfg(feature = "live-discord")]
use crate::player_head::PlayerHeadRefresher;
use crate::state_cli::FileQueueStore;
mod account_control;
mod active_auction;
mod auction_flow;
mod auction_windows;
mod auto_cookie;
mod bank;
mod chat_events;
mod cookie_forcer;
#[cfg(feature = "live-cofl")]
mod cofl;
#[cfg(feature = "api")]
mod dashboard;
mod deferred_queue;
#[cfg(feature = "live-discord")]
mod discord_gateway;
mod formatting;
mod idle;
mod inbox_cursor;
mod inventory_logging;
mod island;
mod lifecycle;
mod listing_confirmation;
mod listing_safety;
mod market_driver;
mod market_queue;
mod minecraft;
mod minecraft_events;
mod missing_listing_inventory;
mod notifier;
mod purchase_relist;
mod queue_completion;
mod reconcile;
mod runtime_loop;
mod runtime_types;
mod stats;
mod support;
mod tracked;
mod windows;

#[cfg(test)]
use account_control::parse_auto_rotate_schedule;
#[cfg(test)]
use account_control::scheduled_directive;
use account_control::{
    LiveAccountScheduler, LiveAccountSupervisor, auto_rotate_schedules,
    configured_startup_runtime_accounts, runtime_accounts, start_auto_rotate_tasks,
    startup_runtime_accounts_with_rotation,
};
use active_auction::LiveActiveAuctionProvider;
use anyhow::{Context, Result};
#[cfg(test)]
use async_trait::async_trait;
use auction_flow::{PendingClaimedBidRelist, TransferFollowup};
#[cfg(test)]
use auction_flow::{
    pending_create_auction_draft, purchased_bid_listing_action, reconcile_auction_window,
};
#[cfg(test)]
use auto_cookie::AutoCookiePhase;
use auto_cookie::{CookiePriceProvider, HypixelCookiePriceProvider, PendingAutoCookie};
use bank::BankCooldownStore;
#[cfg(test)]
use chat_events::claim_notification_collected_coins;
#[cfg(all(test, feature = "live-cofl"))]
use cofl::LiveCoflClient;
#[cfg(all(test, feature = "live-cofl"))]
use cofl::{
    CoflReconnectBackoff, CoflSilentOpenWatchdog, account_socket_session_source,
    cofl_session_for_link, cofl_session_with_env, cofl_socket_host,
    normalize_account_cofl_socket_link,
};
#[cfg(feature = "live-cofl")]
use cofl::{LiveCoflStream, PendingLiveBuy, add_cofl_clients};
#[cfg(feature = "live-discord")]
use discord_gateway::start_discord_gateway;
#[cfg(all(test, feature = "live-discord"))]
use discord_gateway::{
    DiscordGatewayConfig, DiscordInteractionReply, execute_discord_plan, execute_discord_terminal,
    format_inventory_listing_preview, format_planned_directive,
};
use formatting::startup_ready_notification_body;
use idle::start_idle_behavior_tasks;
pub use inbox_cursor::CommandInboxCursor;
use island::LiveIslandState;
pub use market_queue::DryRunMarketAction;
use market_queue::MarketActionQueueStore;
#[cfg(test)]
use market_queue::MarketGuardMinecraftClient;
#[cfg(test)]
use minecraft::{InventoryPriceLookup, LiveMinecraftClientBundle};
use minecraft::{ManagedMinecraftClient, add_minecraft_clients, default_inventory_price_lookup};
#[cfg(all(test, feature = "live-cofl"))]
use notifier::all_flip_notification;
#[cfg(test)]
use notifier::purchase_notification_body;
use notifier::{default_notifier, notify_operator_best_effort};
use purchase_relist::PendingPurchaseRelist;
use reconcile::LiveAuctionReconcilePoller;
use runtime_types::{
    DeferredMinecraftEvents, DeferredQueueEntry, PendingCompletedQueueEntry, PendingCompletionKind,
    PendingListingConfirmation, PendingListingPriceMismatchRetry, PendingMarketStep,
    PendingMissingListingInventoryRetry, PendingOpenAuctionRetry, PendingTransferFollowup,
    PendingUnaffordableListingRetry, StaleTransitionStrikes,
};
pub use runtime_types::{MarketActionMode, RunLiveOptions, RunLiveReport};
#[cfg(all(test, feature = "live-cofl"))]
use saf_cofl::CoflExecuteInstruction;
#[cfg(test)]
use saf_cofl::CoflTelemetryUpdate;
#[cfg(test)]
use saf_core::FlipEvent;
use saf_core::gui::WindowSnapshot;
#[cfg(test)]
use saf_core::ports::AccountPing;
#[cfg(test)]
use saf_core::ports::AccountScheduleRequest;
#[cfg(test)]
use saf_core::ports::AccountStats;
#[cfg(all(test, not(feature = "live-cofl")))]
use saf_core::ports::AccountSupervisor;
#[cfg(test)]
use saf_core::ports::ActiveAuction;
#[cfg(test)]
use saf_core::ports::GuiSlotDiagnostics;
#[cfg(test)]
use saf_core::ports::InventoryProvider;
#[cfg(test)]
use saf_core::ports::Notifier;
#[cfg(test)]
use saf_core::ports::QueueStore;
#[cfg(test)]
use saf_core::ports::ScheduledAccountAction;
#[cfg(test)]
use saf_core::ports::{AccountStatsProvider, MinecraftAction, Notification};
#[cfg(test)]
use saf_core::ports::{ActiveAuctionProvider, GuiDiagnosticsProvider};
#[cfg(test)]
use saf_core::ports::{InventoryItem, InventorySnapshot};
use saf_core::ports::{MinecraftClient, MinecraftEvent};
use saf_core::{AccountId, BotRuntime, Humanizer, RuntimeSession, SafConfig};
#[cfg(all(test, feature = "live-discord"))]
use saf_discord::{DiscordCommandPlan, DiscordControllerAction};
#[cfg(test)]
use saf_minecraft::RecordedMinecraftClient;
#[cfg(test)]
use stats::AuctionSlotStats;
#[cfg(test)]
use stats::ClaimStatsUpdate;
#[cfg(test)]
use stats::auction_slot_stats_from_window;
use stats::{LiveStatsProvider, PurchaseStatsUpdate};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
#[cfg(test)]
use tokio::sync::Mutex as AsyncMutex;
use tokio::time::sleep;
#[cfg(test)]
use tracked::SoldListingMetadata;
use tracked::{LiveSoldTracker, LiveTrackedFlipProvider};
#[cfg(test)]
use windows::active_auction_summaries;

const MARKET_STEP_RETRY_INTERVAL: Duration = Duration::from_secs(5);
const MARKET_WINDOW_SETTLE_DELAY: Duration = Duration::from_millis(300);
const CLAIM_PURCHASED_OPEN_MAX_ATTEMPTS: u8 = 3;
/// How many consecutive no-progress clicks the same market step may produce
/// before the kill-switch aborts it. A stuck create-auction "submit" that never
/// opens the confirmation would otherwise retry forever and spam the server
/// (ban risk); this caps the run, abandons the step, and alerts the operator.
const MARKET_STEP_NO_PROGRESS_MAX_STRIKES: u8 = 3;
const STARTUP_COFL_TELEMETRY_WAIT: Duration = Duration::from_secs(5);
const DEFERRED_QUEUE_RETRY_DELAY: Duration = Duration::from_secs(5);
const EXPIRED_RELIST_QUEUE_DELAY: Duration = Duration::from_secs(10);
const MISSING_LISTING_INVENTORY_RETRY_DELAY: Duration = Duration::from_secs(60);
const MISSING_LISTING_INVENTORY_MAX_ATTEMPTS: u8 = 3;
/// How long to hold a listing back after the account was found unable to afford
/// its auction creation fee. Coins may free up as other auctions sell, so the
/// listing is re-checked rather than dropped.
const UNAFFORDABLE_LISTING_RETRY_DELAY: Duration = Duration::from_secs(120);
const STARTUP_RECONCILE_QUEUE_DELAY: Duration = Duration::from_secs(5);
const STARTUP_PROFILE_SCAN_TIMEOUT: Duration = Duration::from_secs(20);
const PURCHASE_RELIST_RETRY_DELAY: Duration = Duration::from_secs(2);
const PURCHASE_RELIST_MAX_ATTEMPTS: u8 = 5;
const DEFAULT_LOCRAW_DELAY_MS: u64 = 5_000;
const DEFAULT_BAD_MOD_KICK_BACKOFF_MS: u64 = 120_000;
const DEFAULT_SOLD_AUCTION_POLL_MS: u64 = 10_000;
const DEFAULT_IDLE_AUCTION_RECONCILE_MS: u64 = 120_000;
const DEFAULT_BANK_COOLDOWN_MS: u64 = 60 * 60 * 1_000;
#[cfg(feature = "live-discord")]
const DEFAULT_DISCORD_GATEWAY_RESTART_MS: u64 = 30_000;
const BASE_AUCTION_SLOTS: usize = 14;
#[cfg(feature = "live-cofl")]
const COFL_SILENT_OPEN_TIMEOUT: Duration = Duration::from_secs(20);
#[cfg(feature = "live-cofl")]
const COFL_REGION_BACKOFF: Duration = Duration::from_secs(5 * 60);
#[cfg(feature = "live-cofl")]
const COFL_READ_TIMEOUT: Duration = Duration::from_millis(10);
pub const DEFAULT_RUN_LIVE_POLL_INTERVAL_MS: u64 = 10;

pub async fn run_live(config: SafConfig, options: RunLiveOptions) -> Result<RunLiveReport> {
    let mut runner = LiveRuntime::start(config, options).await?;
    let result = if runner.options.once {
        runner.poll_once().await.map(|_| ())
    } else {
        runner.run_until_shutdown().await
    };
    if runner.options.once {
        runner.shutdown_runtime().await;
    }
    let report = runner.report();
    result?;
    Ok(report)
}

pub struct LiveRuntime {
    session: Arc<RuntimeSession>,
    config: SafConfig,
    accounts: Vec<AccountId>,
    options: RunLiveOptions,
    #[allow(dead_code)]
    humanizer: Arc<Humanizer>,
    inbox: CommandInboxCursor,
    queue: Arc<MarketActionQueueStore>,
    stats: Arc<LiveStatsProvider>,
    tracked_flips: Arc<LiveTrackedFlipProvider>,
    sold_tracker: LiveSoldTracker,
    minecraft_clients: BTreeMap<AccountId, Arc<dyn MinecraftClient>>,
    managed_minecraft: BTreeMap<AccountId, Arc<ManagedMinecraftClient>>,
    minecraft_ready_accounts: BTreeSet<AccountId>,
    active_windows: Arc<Mutex<BTreeMap<AccountId, WindowSnapshot>>>,
    /// Last Manage Auctions view seen per account, filled passively whenever the
    /// bot opens that menu. Shared with the dashboard API so the auctions view
    /// never has to navigate the GUI itself. See [[windows::AuctionViewSnapshot]].
    auction_views: Arc<Mutex<BTreeMap<AccountId, windows::AuctionViewSnapshot>>>,
    active_window_received_at: Mutex<BTreeMap<AccountId, Instant>>,
    active_window_observed_at: Mutex<BTreeMap<AccountId, Instant>>,
    /// Per-account jittered settle window for menu/inter-click pacing. Resampled
    /// each time a new GUI window is observed so the menu-navigation cadence
    /// (open menu, list, claim, bank, reconcile) varies every time instead of
    /// sitting on a fixed ~300 ms beat. Keyed to the observed-at instant so a
    /// stale sample is never reused across windows.
    market_settle_jitter: Mutex<BTreeMap<AccountId, (Instant, Duration)>>,
    account_humanizers: BTreeMap<AccountId, Arc<Humanizer>>,
    deferred_minecraft_events: DeferredMinecraftEvents,
    island_states: BTreeMap<AccountId, LiveIslandState>,
    cookie_prices: Arc<dyn CookiePriceProvider>,
    pending_auto_cookies: BTreeMap<AccountId, PendingAutoCookie>,
    /// Accounts the operator has manually requested an immediate booster-cookie
    /// buy for. Shared with the `LiveCookieForcer` port; drained each poll by
    /// `drive_forced_cookies_once`.
    cookie_force_requests: Arc<Mutex<BTreeSet<AccountId>>>,
    auction_reconcile_poller: Option<LiveAuctionReconcilePoller>,
    bank_cooldowns: BankCooldownStore,
    pending_market_steps: BTreeMap<AccountId, PendingMarketStep>,
    pending_open_auction_retries: BTreeMap<AccountId, PendingOpenAuctionRetry>,
    /// Consecutive no-progress strikes for the current market step, per account.
    /// Drives the listing/transition kill-switch that aborts a stuck step
    /// instead of spamming the server forever.
    stale_transition_strikes: BTreeMap<AccountId, StaleTransitionStrikes>,
    pending_missing_listing_inventory_retries:
        BTreeMap<AccountId, PendingMissingListingInventoryRetry>,
    pending_listing_price_mismatch_retries: BTreeMap<AccountId, PendingListingPriceMismatchRetry>,
    /// Listings held off because the account can't afford the auction creation
    /// fee right now. Re-checked after a delay rather than retried immediately.
    pending_unaffordable_listing_retries: BTreeMap<AccountId, PendingUnaffordableListingRetry>,
    deferred_queue_entries: Vec<DeferredQueueEntry>,
    pending_purchase_relists: Vec<PendingPurchaseRelist>,
    pending_transfer_followups: Vec<PendingTransferFollowup>,
    pending_claimed_bid_relists: Vec<PendingClaimedBidRelist>,
    pending_completed_entries: Vec<PendingCompletedQueueEntry>,
    pending_listing_confirmations: BTreeMap<AccountId, PendingListingConfirmation>,
    auto_rotate_tasks: Vec<tokio::task::JoinHandle<()>>,
    idle_tasks: Vec<tokio::task::JoinHandle<()>>,
    #[cfg(feature = "live-cofl")]
    pending_live_buys: Arc<Mutex<BTreeMap<AccountId, PendingLiveBuy>>>,
    #[cfg(feature = "live-cofl")]
    cofl_streams: Vec<LiveCoflStream>,
    #[cfg(feature = "live-discord")]
    discord_task: Option<tokio::task::JoinHandle<()>>,
    #[cfg(feature = "live-discord")]
    discord_head_refresher: Arc<PlayerHeadRefresher>,
    #[cfg(feature = "live-discord")]
    discord_restart_at: Option<Instant>,
    #[cfg(feature = "live-discord")]
    discord_restart_delay: Duration,
    #[cfg(feature = "api")]
    api_task: Option<tokio::task::JoinHandle<()>>,
    shutdown: Arc<AtomicBool>,
    /// Operator "panic stop". When set, the poll loop performs no account or
    /// market work and background tasks (auto-rotate, scheduler) refuse to bring
    /// accounts back online. Distinct from `shutdown`, which means the whole
    /// process is exiting. Cleared by an explicit operator start.
    halted: Arc<AtomicBool>,
    cofl_connections: usize,
    cofl_connected: usize,
    discord_started: bool,
    processed_commands: usize,
    processed_cofl_envelopes: usize,
    processed_minecraft_events: usize,
    processed_queue_steps: usize,
    completed_queue_entries: usize,
}

async fn next_minecraft_event(client: &dyn MinecraftClient) -> Result<Option<MinecraftEvent>> {
    match tokio::time::timeout(Duration::from_millis(1), client.next_event()).await {
        Ok(result) => result.map_err(anyhow::Error::from),
        Err(_) => Ok(None),
    }
}

fn push_deferred_minecraft_event(
    events: &DeferredMinecraftEvents,
    account: &AccountId,
    event: MinecraftEvent,
) -> std::result::Result<(), String> {
    events
        .lock()
        .map_err(|_| "deferred Minecraft event lock poisoned".to_string())?
        .entry(account.clone())
        .or_default()
        .push_back(event);
    Ok(())
}

fn pop_deferred_minecraft_event(
    events: &DeferredMinecraftEvents,
    account: &AccountId,
) -> std::result::Result<Option<MinecraftEvent>, String> {
    let mut events = events
        .lock()
        .map_err(|_| "deferred Minecraft event lock poisoned".to_string())?;
    let mut remove_account = false;
    let event = events.get_mut(account).and_then(|queue| {
        let event = queue.pop_front();
        remove_account = queue.is_empty();
        event
    });
    if remove_account {
        events.remove(account);
    }
    Ok(event)
}

#[cfg_attr(not(feature = "live-cofl"), allow(dead_code))]
fn should_upload_scoreboard(lines: &[String]) -> bool {
    lines
        .iter()
        .any(|line| line.contains("Purse:") || line.contains("Piggy:"))
}

#[cfg(not(feature = "live-cofl"))]
async fn add_cofl_clients(
    session: &mut RuntimeSession,
    accounts: &[AccountId],
    config: &SafConfig,
    options: &RunLiveOptions,
    account_humanizers: &BTreeMap<AccountId, Arc<Humanizer>>,
) -> Result<usize> {
    let _ = (session, accounts, config, options, account_humanizers);
    Ok(0)
}

fn env_duration_ms(name: &str, default_ms: u64) -> Duration {
    Duration::from_millis(env_u64(name, default_ms))
}

fn env_u64(name: &str, default_value: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default_value)
}

#[cfg_attr(not(feature = "live-discord"), allow(dead_code))]
fn env_truthy(value: &str) -> bool {
    let value = value.trim();
    value == "1" || value.eq_ignore_ascii_case("true")
}

fn env_optional_duration_ms(name: &str, default_ms: u64) -> Option<Duration> {
    match std::env::var(name) {
        Ok(value) => value
            .parse::<u64>()
            .ok()
            .filter(|value| *value > 0)
            .map(Duration::from_millis),
        Err(_) => Some(Duration::from_millis(default_ms)),
    }
}

fn is_bad_modification_message(text: &str) -> bool {
    text.to_ascii_lowercase()
        .contains("badly behaving modifications")
}

#[cfg(feature = "live-minecraft")]
fn native_minecraft_enabled() -> bool {
    std::env::var("SAF_RUST_MINECRAFT")
        .map(|value| value.trim().eq_ignore_ascii_case("azalea"))
        .unwrap_or(false)
}

#[cfg(not(feature = "live-minecraft"))]
fn native_minecraft_enabled() -> bool {
    false
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[allow(dead_code)]
fn path_exists(path: impl AsRef<Path>) -> bool {
    path.as_ref().exists()
}

#[cfg(test)]
mod tests;
