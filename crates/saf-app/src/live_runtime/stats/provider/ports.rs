use super::super::super::now_ms;
use super::LiveStatsProvider;
use async_trait::async_trait;
use saf_core::AccountId;
use saf_core::ports::{
    AccountConnectionProvider, AccountPing, AccountStats, AccountStatsProvider, PortError,
};

#[async_trait]
impl AccountStatsProvider for LiveStatsProvider {
    async fn stats(&self, account: &AccountId) -> Result<AccountStats, PortError> {
        if !self.accounts.contains(account) {
            return Err(PortError::Unavailable(format!(
                "no stats provider for {account}"
            )));
        }
        let purse = self
            .purses
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .get(account)
            .copied();
        let bought_profits = self
            .bought_profits
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .get(account)
            .cloned()
            .unwrap_or_default();
        let bought = bought_profits.len();
        let total_profit = bought_profits
            .iter()
            .copied()
            .filter(|profit| profit.is_finite() && *profit > 0.0)
            .sum::<f64>();
        let user_finder_flips = bought_profits
            .iter()
            .filter(|profit| !profit.is_finite() || **profit <= 0.0)
            .count();
        let sold = self
            .sold_counts
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .get(account)
            .copied()
            .unwrap_or_default();
        let ping = self
            .cofl_ping
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .get(account)
            .cloned()
            .unwrap_or_default();
        let cofl_info = self
            .cofl_info
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .get(account)
            .cloned()
            .unwrap_or_default();
        let cookie_expires_at = self
            .cookie_expires_at
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .get(account)
            .copied();
        let auction_slots = self
            .auction_slots
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .get(account)
            .copied()
            .unwrap_or_default();
        Ok(AccountStats {
            started_at_ms: Some(self.started_at_ms),
            bought,
            sold,
            total_profit,
            user_finder_flips,
            profit_per_hour: profit_per_hour(total_profit, self.started_at_ms, now_ms()),
            purse,
            cofl_delay_ms: ping.cofl_delay_ms,
            cofl_ping_ms: ping.cofl_ping_ms,
            cofl_tier: cofl_info.tier,
            cofl_expires_at: cofl_info.expires_at,
            cookie_expires_at,
            hypixel_ping_ms: ping.hypixel_ping_ms,
            auction_slots_used: auction_slots.used,
            auction_slots_max: auction_slots.max,
        })
    }

    async fn ping(&self, account: &AccountId) -> Result<AccountPing, PortError> {
        if !self.accounts.contains(account) {
            return Err(PortError::Unavailable(format!(
                "no ping provider for {account}"
            )));
        }
        Ok(self
            .cofl_ping
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .get(account)
            .cloned()
            .unwrap_or_default())
    }
}

#[async_trait]
impl AccountConnectionProvider for LiveStatsProvider {
    async fn connection_id(&self, account: &AccountId) -> Result<Option<String>, PortError> {
        if !self.accounts.contains(account) {
            return Err(PortError::Unavailable(format!(
                "no connection provider for {account}"
            )));
        }
        Ok(self
            .connection_ids
            .lock()
            .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
            .get(account)
            .cloned())
    }
}

fn profit_per_hour(total_profit: f64, started_at_ms: u64, now_ms: u64) -> Option<f64> {
    if !total_profit.is_finite() {
        return None;
    }
    let elapsed_ms = now_ms.saturating_sub(started_at_ms);
    if elapsed_ms == 0 {
        return Some(0.0);
    }
    Some(total_profit / (elapsed_ms as f64 / 3_600_000.0))
}
