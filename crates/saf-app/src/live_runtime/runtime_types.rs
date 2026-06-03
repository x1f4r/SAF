use super::{DEFAULT_RUN_LIVE_POLL_INTERVAL_MS, TransferFollowup};
use clap::ValueEnum;
use saf_core::{AccountId, BotState, MarketInstruction, QueueEntry, ports::MinecraftEvent};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum MarketActionMode {
    DryRun,
    Live,
}

impl MarketActionMode {
    pub fn allows_market_actions(self) -> bool {
        matches!(self, Self::Live)
    }
}

#[derive(Clone, Debug)]
pub struct RunLiveOptions {
    pub config_path: PathBuf,
    pub command_inbox: PathBuf,
    pub state_base_dir: PathBuf,
    pub market_actions: MarketActionMode,
    pub poll_interval: Duration,
    pub once: bool,
    pub connect_cofl: bool,
}

impl RunLiveOptions {
    pub fn new(command_inbox: impl Into<PathBuf>, state_base_dir: impl Into<PathBuf>) -> Self {
        Self {
            config_path: PathBuf::from("config.json5"),
            command_inbox: command_inbox.into(),
            state_base_dir: state_base_dir.into(),
            market_actions: MarketActionMode::DryRun,
            poll_interval: Duration::from_millis(DEFAULT_RUN_LIVE_POLL_INTERVAL_MS),
            once: false,
            connect_cofl: true,
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunLiveReport {
    pub accounts: Vec<AccountId>,
    pub running_accounts: Vec<AccountId>,
    pub command_inbox: String,
    pub state_base_dir: String,
    pub market_actions: MarketActionMode,
    pub processed_commands: usize,
    pub processed_cofl_envelopes: usize,
    pub processed_minecraft_events: usize,
    pub processed_queue_steps: usize,
    pub completed_queue_entries: usize,
    pub dry_run_market_actions: usize,
    pub cofl_connections: usize,
    pub cofl_connected: usize,
    pub discord_started: bool,
}

pub(super) type DeferredMinecraftEvents = Arc<Mutex<BTreeMap<AccountId, VecDeque<MinecraftEvent>>>>;

#[derive(Clone, Debug)]
pub(super) struct PendingMarketStep {
    pub(super) entry: QueueEntry,
    pub(super) instruction: MarketInstruction,
    pub(super) last_attempt: Instant,
    pub(super) opens_sold_claim_action: bool,
}

#[derive(Clone, Debug)]
pub(super) struct PendingOpenAuctionRetry {
    pub(super) entry: QueueEntry,
    pub(super) instruction: MarketInstruction,
    pub(super) attempts: u8,
}

/// Counts how many times the *same* market step (same queue entry + click
/// instruction) has fired without the GUI advancing — i.e. the click produced
/// no fresh window and we cleared a stale one. A run of these is the signature
/// of a stuck loop (e.g. a create-auction "submit" that never opens the
/// confirmation), which spams the server and is a ban risk. Tracked per account
/// so the driver can trip a kill-switch instead of retrying forever.
#[derive(Clone, Debug)]
pub(super) struct StaleTransitionStrikes {
    pub(super) entry: QueueEntry,
    pub(super) count: u8,
}

#[derive(Clone, Debug)]
pub(super) struct PendingMissingListingInventoryRetry {
    pub(super) entry: QueueEntry,
    pub(super) retry_at: Instant,
    pub(super) attempts: u8,
}

#[derive(Clone, Debug)]
pub(super) struct PendingListingPriceMismatchRetry {
    pub(super) entry: QueueEntry,
    pub(super) retry_at: Instant,
    pub(super) attempts: u8,
}

/// Records that an account's listings are held off because it could not afford
/// an auction creation fee. The hold is per-account (the entry representation
/// varies between reconcile cycles, so keying on a specific entry would let the
/// hold leak): all of the account's listings wait until `retry_at`, then one
/// re-check is allowed. `notified` dedupes the operator alert to once per blocked
/// episode (cleared when the account can afford listings again).
#[derive(Clone, Debug)]
pub(super) struct PendingUnaffordableListingRetry {
    pub(super) retry_at: Instant,
    pub(super) notified: bool,
}

#[derive(Clone, Debug)]
pub(super) struct DeferredQueueEntry {
    pub(super) account: AccountId,
    pub(super) action: Value,
    pub(super) state: BotState,
    pub(super) priority: u8,
    pub(super) ready_at: Instant,
}

#[derive(Clone, Debug)]
pub(super) struct PendingTransferFollowup {
    pub(super) source: AccountId,
    pub(super) transfer: TransferFollowup,
    pub(super) ready_at: Instant,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PendingCompletionKind {
    Generic,
    Reconcile,
    CountOnly,
}

#[derive(Clone, Debug)]
pub(super) struct PendingCompletedQueueEntry {
    pub(super) account: AccountId,
    pub(super) entry: QueueEntry,
    pub(super) kind: PendingCompletionKind,
    pub(super) finished: bool,
    pub(super) ready_at: Instant,
}

#[derive(Clone, Debug)]
pub(super) struct PendingListingConfirmation {
    pub(super) entry: QueueEntry,
    pub(super) attempts: u8,
    pub(super) last_click_at: Instant,
}
