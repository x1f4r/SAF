use super::super::windows::{
    cookie_duration_from_window, is_profiles_window, is_skyblock_menu_window,
};
use super::super::{DeferredQueueEntry, LiveRuntime, STARTUP_RECONCILE_QUEUE_DELAY};
use super::state::LiveIslandState;
use anyhow::{Context, Result};
use saf_core::ports::MinecraftAction;
use saf_core::{AccountId, BotState};
use std::time::{Duration, Instant};

impl LiveRuntime {
    pub(in crate::live_runtime) async fn drive_startup_profile_scans_once(&mut self) -> Result<()> {
        let now = Instant::now();
        let running = self.running_account_set();
        let due = self
            .island_states
            .iter_mut()
            .filter_map(|(account, state)| {
                (running.contains(account) && state.take_due_startup_profile_scan(now))
                    .then(|| account.clone())
            })
            .collect::<Vec<_>>();

        for account in due {
            let Some(client) = self.minecraft_clients.get(&account).cloned() else {
                continue;
            };
            if let Err(error) = client
                .perform(MinecraftAction::Chat("/profiles".to_string()))
                .await
                .with_context(|| format!("requesting profiles for {account} startup setup"))
            {
                self.handle_minecraft_action_failure(&account, "startup profile request", error)
                    .await?;
            }
        }
        Ok(())
    }

    pub(in crate::live_runtime) async fn expire_startup_profile_scans_once(&mut self) {
        let now = Instant::now();
        let running = self.running_account_set();
        let expired = self
            .island_states
            .iter_mut()
            .filter_map(|(account, state)| {
                (running.contains(account) && state.expire_startup_profile_scan(now))
                    .then(|| account.clone())
            })
            .collect::<Vec<_>>();

        for account in expired {
            tracing::warn!(
                account = %account,
                "startup profile scan timed out; auction slot capacity remains unknown until a profile or manage-auctions scan confirms it"
            );
            self.schedule_startup_cookie_scan(&account);
            self.notify_if_startup_ready(&account).await;
        }
    }

    pub(in crate::live_runtime) async fn drive_startup_cookie_scans_once(&mut self) -> Result<()> {
        let now = Instant::now();
        let running = self.running_account_set();
        let due = self
            .island_states
            .iter_mut()
            .filter_map(|(account, state)| {
                (running.contains(account) && state.take_due_startup_cookie_scan(now))
                    .then(|| account.clone())
            })
            .collect::<Vec<_>>();

        for account in due {
            let Some(client) = self.minecraft_clients.get(&account).cloned() else {
                continue;
            };
            if let Err(error) = client
                .perform(MinecraftAction::Chat("/sbmenu".to_string()))
                .await
                .with_context(|| format!("requesting SkyBlock menu for {account} cookie status"))
            {
                self.handle_minecraft_action_failure(&account, "startup cookie request", error)
                    .await?;
            }
        }
        Ok(())
    }

    pub(in crate::live_runtime) async fn expire_startup_cookie_scans_once(&mut self) {
        let now = Instant::now();
        let running = self.running_account_set();
        let expired = self
            .island_states
            .iter_mut()
            .filter_map(|(account, state)| {
                (running.contains(account) && state.expire_startup_cookie_scan(now))
                    .then(|| account.clone())
            })
            .collect::<Vec<_>>();

        for account in expired {
            tracing::warn!(
                account = %account,
                "startup cookie scan timed out; continuing with unknown cookie duration"
            );
            self.notify_if_startup_ready(&account).await;
        }
    }

    pub(in crate::live_runtime) fn consume_startup_profile_window(
        &mut self,
        account: &AccountId,
    ) -> bool {
        self.active_windows
            .lock()
            .ok()
            .and_then(|windows| windows.get(account).cloned())
            .is_some_and(|window| is_profiles_window(&window))
            && self
                .island_states
                .get_mut(account)
                .is_some_and(LiveIslandState::finish_startup_profile_scan)
    }

    pub(in crate::live_runtime) fn schedule_startup_cookie_scan(&mut self, account: &AccountId) {
        if !self.config.use_cookie || !self.config.relist {
            return;
        }
        if let Some(state) = self.island_states.get_mut(account) {
            state.schedule_startup_cookie_scan(Instant::now());
        }
    }

    pub(in crate::live_runtime) fn consume_startup_cookie_window(
        &mut self,
        account: &AccountId,
    ) -> Option<Option<Duration>> {
        let window = self
            .active_windows
            .lock()
            .ok()
            .and_then(|windows| windows.get(account).cloned());
        let window = window?;
        if !is_skyblock_menu_window(&window) {
            return None;
        }
        let duration = cookie_duration_from_window(&window);
        if let Some(duration) = duration {
            self.record_cookie_duration_best_effort(account, duration);
        }
        self.island_states
            .get_mut(account)
            .is_some_and(LiveIslandState::finish_startup_cookie_scan)
            .then_some(duration)
    }

    pub(in crate::live_runtime) fn queue_startup_reconcile(&mut self, account: &AccountId) {
        if !self.config.use_cookie || !self.config.relist {
            return;
        }
        let Some(state) = self.island_states.get_mut(account) else {
            return;
        };
        if !state.take_startup_reconcile_request() {
            return;
        }
        state.schedule_startup_profile_scan(Instant::now());
        self.deferred_queue_entries.push(DeferredQueueEntry {
            account: account.clone(),
            action: serde_json::json!({"reason": "startup"}),
            state: BotState::Custom("reconcileAuctions".to_string()),
            priority: 2,
            ready_at: Instant::now() + STARTUP_RECONCILE_QUEUE_DELAY,
        });
    }

    pub(in crate::live_runtime) fn account_market_ready(&self, account: &AccountId) -> bool {
        !self.pending_auto_cookies.contains_key(account)
            && self
                .island_states
                .get(account)
                .is_none_or(LiveIslandState::allows_market_work)
    }
}
