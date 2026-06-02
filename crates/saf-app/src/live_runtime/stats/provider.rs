mod chat_state;
mod ports;
mod telemetry;

use super::super::now_ms;
use super::auction_slots::{
    AuctionSlotStats, auction_slot_stats_from_window, auction_slot_stats_full,
    profile_auction_slots_max,
};
use saf_core::AccountId;
use saf_core::gui::WindowSnapshot;
use saf_core::ports::{AccountPing, PortError};
use saf_core::protocol_text::{clean_scoreboard_lines, parse_purse_from_scoreboard_lines};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const AUCTION_COLLECTION_CONTEXT_TTL_MS: u64 = 15_000;
const AUCTION_COLLECTION_CONTEXT_LIMIT: usize = 10;

#[derive(Clone, Debug)]
pub(in crate::live_runtime) struct LiveStatsProvider {
    pub(super) started_at_ms: u64,
    pub(super) accounts: BTreeSet<AccountId>,
    pub(in crate::live_runtime) purses: Arc<Mutex<BTreeMap<AccountId, f64>>>,
    pub(in crate::live_runtime) auction_slots: Arc<Mutex<BTreeMap<AccountId, AuctionSlotStats>>>,
    pub(super) bought_profits: Arc<Mutex<BTreeMap<AccountId, Vec<f64>>>>,
    pub(super) sold_counts: Arc<Mutex<BTreeMap<AccountId, usize>>>,
    latest_scoreboards: Arc<Mutex<BTreeMap<AccountId, Vec<String>>>>,
    pub(super) cofl_ping: Arc<Mutex<BTreeMap<AccountId, AccountPing>>>,
    pub(super) cofl_info: Arc<Mutex<BTreeMap<AccountId, CoflAccountInfo>>>,
    pub(super) cookie_expires_at: Arc<Mutex<BTreeMap<AccountId, u64>>>,
    pub(super) connection_ids: Arc<Mutex<BTreeMap<AccountId, String>>>,
    pub(super) auction_collections:
        Arc<Mutex<BTreeMap<AccountId, VecDeque<PendingAuctionCollection>>>>,
    pub(super) pending_zero_claims: Arc<Mutex<BTreeMap<AccountId, VecDeque<PendingZeroClaim>>>>,
    #[cfg(feature = "api")]
    pub(super) dashboard_sink: Option<Arc<dyn crate::live_runtime::dashboard::DashboardEventSink>>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct CoflAccountInfo {
    pub(super) tier: Option<String>,
    pub(super) expires_at: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PendingAuctionCollection {
    pub(super) coins: u64,
    pub(super) at_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PendingZeroClaim {
    pub(super) item_name: String,
    pub(super) buyer: String,
    pub(super) at_ms: u64,
}

impl LiveStatsProvider {
    pub(in crate::live_runtime) fn new(accounts: Vec<AccountId>) -> Self {
        Self {
            started_at_ms: now_ms(),
            accounts: accounts.into_iter().collect(),
            purses: Arc::new(Mutex::new(BTreeMap::new())),
            auction_slots: Arc::new(Mutex::new(BTreeMap::new())),
            bought_profits: Arc::new(Mutex::new(BTreeMap::new())),
            sold_counts: Arc::new(Mutex::new(BTreeMap::new())),
            latest_scoreboards: Arc::new(Mutex::new(BTreeMap::new())),
            cofl_ping: Arc::new(Mutex::new(BTreeMap::new())),
            cofl_info: Arc::new(Mutex::new(BTreeMap::new())),
            cookie_expires_at: Arc::new(Mutex::new(BTreeMap::new())),
            connection_ids: Arc::new(Mutex::new(BTreeMap::new())),
            auction_collections: Arc::new(Mutex::new(BTreeMap::new())),
            pending_zero_claims: Arc::new(Mutex::new(BTreeMap::new())),
            #[cfg(feature = "api")]
            dashboard_sink: None,
        }
    }

    /// Attach the dashboard event sink so parsed buys/sells/claims are mirrored
    /// to the live API stream and persistent ledger.
    #[cfg(feature = "api")]
    pub(in crate::live_runtime) fn set_dashboard_sink(
        &mut self,
        sink: Arc<dyn crate::live_runtime::dashboard::DashboardEventSink>,
    ) {
        self.dashboard_sink = Some(sink);
    }

    pub(in crate::live_runtime) fn record_window_snapshot(
        &self,
        account: &AccountId,
        window: &WindowSnapshot,
    ) -> Result<(), PortError> {
        if !self.accounts.contains(account) {
            return Err(PortError::Unavailable(format!(
                "no stats provider for {account}"
            )));
        }
        let max_from_profile = profile_auction_slots_max(window);
        let stats_from_manage = auction_slot_stats_from_window(window);
        if max_from_profile.is_none() && stats_from_manage.is_none() {
            return Ok(());
        }

        let mut auction_slots = self
            .auction_slots
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?;
        let entry = auction_slots.entry(account.clone()).or_default();
        let previous = *entry;
        if let Some(stats) = stats_from_manage {
            entry.used = stats.used;
            if stats.max.is_some() {
                entry.max = stats.max;
            }
        }
        if let Some(max) = max_from_profile {
            entry.max = Some(max);
        }
        if previous != *entry {
            tracing::debug!(
                account = %account,
                window = %window.title,
                auction_slots_used = ?entry.used,
                auction_slots_max = ?entry.max,
                "updated auction slot stats from window"
            );
        }
        Ok(())
    }

    pub(in crate::live_runtime) fn auction_slots_full(
        &self,
        account: &AccountId,
    ) -> Result<bool, PortError> {
        let Some(stats) = self
            .auction_slots
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .get(account)
            .copied()
        else {
            return Ok(false);
        };
        Ok(auction_slot_stats_full(stats))
    }

    pub(in crate::live_runtime) fn set_auction_slots_max(
        &self,
        account: &AccountId,
        max: usize,
    ) -> Result<(), PortError> {
        if !self.accounts.contains(account) {
            return Err(PortError::Unavailable(format!(
                "no stats provider for {account}"
            )));
        }
        if max == 0 {
            return Err(PortError::Failed(
                "auction slot max override must be greater than zero".to_string(),
            ));
        }
        let mut slots = self
            .auction_slots
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?;
        slots.entry(account.clone()).or_default().max = Some(max);
        Ok(())
    }

    pub(in crate::live_runtime) fn apply_auction_slot_max_overrides<F>(
        &self,
        env: F,
    ) -> Result<(), PortError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let global = parse_auction_slot_max_env(env("SAF_AUCTION_SLOTS_MAX"));
        for account in &self.accounts {
            let account_key =
                crate::config_session::account_env_key("SAF_AUCTION_SLOTS_MAX", account);
            let Some(max) = parse_auction_slot_max_env(env(&account_key)).or(global) else {
                continue;
            };
            self.set_auction_slots_max(account, max)?;
            tracing::info!(
                account = %account,
                auction_slots_max = max,
                "applied configured auction slot capacity override"
            );
        }
        Ok(())
    }

    pub(in crate::live_runtime) fn increment_auction_slots_used(
        &self,
        account: &AccountId,
    ) -> Result<(), PortError> {
        if !self.accounts.contains(account) {
            return Err(PortError::Unavailable(format!(
                "no stats provider for {account}"
            )));
        }
        let mut slots = self
            .auction_slots
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?;
        let entry = slots.entry(account.clone()).or_default();
        let next = entry.used.unwrap_or_default().saturating_add(1);
        entry.used = Some(entry.max.map_or(next, |max| next.min(max)));
        Ok(())
    }

    pub(in crate::live_runtime) fn record_scoreboard(
        &self,
        account: &AccountId,
        lines: &[String],
    ) -> Result<(), PortError> {
        if !self.accounts.contains(account) {
            return Err(PortError::Unavailable(format!(
                "no stats provider for {account}"
            )));
        }
        let lines = clean_scoreboard_lines(lines);
        let values = lines
            .iter()
            .map(|line| Value::String(line.clone()))
            .collect::<Vec<_>>();
        self.latest_scoreboards
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .insert(account.clone(), lines.clone());
        if let Some(purse) = parse_purse_from_scoreboard_lines(&values) {
            self.purses
                .lock()
                .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
                .insert(account.clone(), purse as f64);
        }
        Ok(())
    }

    #[cfg(feature = "live-cofl")]
    pub(in crate::live_runtime) fn latest_scoreboard(
        &self,
        account: &AccountId,
    ) -> Result<Option<Vec<String>>, PortError> {
        if !self.accounts.contains(account) {
            return Err(PortError::Unavailable(format!(
                "no stats provider for {account}"
            )));
        }
        Ok(self
            .latest_scoreboards
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .get(account)
            .cloned())
    }

    pub(in crate::live_runtime) fn record_cookie_duration(
        &self,
        account: &AccountId,
        duration: Duration,
    ) -> Result<(), PortError> {
        if !self.accounts.contains(account) {
            return Err(PortError::Unavailable(format!(
                "no stats provider for {account}"
            )));
        }
        let expires_at =
            now_ms().saturating_add(duration.as_millis().min(u128::from(u64::MAX)) as u64) / 1_000;
        self.cookie_expires_at
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .insert(account.clone(), expires_at);
        Ok(())
    }
}

fn parse_auction_slot_max_env(value: Option<String>) -> Option<usize> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
}
