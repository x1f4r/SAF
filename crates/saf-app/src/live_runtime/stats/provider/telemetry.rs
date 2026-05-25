use super::LiveStatsProvider;
use saf_cofl::CoflTelemetryUpdate;
use saf_core::AccountId;
use saf_core::ports::PortError;

impl LiveStatsProvider {
    #[cfg_attr(not(feature = "live-cofl"), allow(dead_code))]
    pub(in crate::live_runtime) fn record_cofl_telemetry(
        &self,
        account: &AccountId,
        update: &CoflTelemetryUpdate,
    ) -> Result<(), PortError> {
        if !self.accounts.contains(account) {
            return Err(PortError::Unavailable(format!(
                "no stats provider for {account}"
            )));
        }
        if let Some(connection_id) = &update.connection_id {
            self.connection_ids
                .lock()
                .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?
                .insert(account.clone(), connection_id.clone());
            tracing::info!(
                account = %account,
                connection_id = %connection_id,
                "updated Cofl connection telemetry"
            );
        }
        if update.cofl_delay_ms.is_some() || update.cofl_ping_ms.is_some() {
            let mut ping = self
                .cofl_ping
                .lock()
                .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?;
            let entry = ping.entry(account.clone()).or_default();
            if let Some(delay) = update.cofl_delay_ms {
                entry.cofl_delay_ms = Some(delay);
            }
            if let Some(cofl_ping) = update.cofl_ping_ms {
                entry.cofl_ping_ms = Some(cofl_ping);
            }
        }
        if update.cofl_tier.is_some() || update.cofl_expires_at.is_some() {
            let mut info = self
                .cofl_info
                .lock()
                .map_err(|_| PortError::Failed("stats provider lock poisoned".to_string()))?;
            let entry = info.entry(account.clone()).or_default();
            if let Some(tier) = &update.cofl_tier {
                entry.tier = Some(tier.clone());
            }
            if let Some(expires_at) = update.cofl_expires_at {
                entry.expires_at = Some(expires_at);
            }
            tracing::info!(
                account = %account,
                tier = update.cofl_tier.as_deref().unwrap_or("unchanged"),
                expires_at = update.cofl_expires_at,
                "updated Cofl account telemetry"
            );
        }
        Ok(())
    }
}
