use super::super::LiveRuntime;
use super::plan::{
    AutoCookieDecision, AutoCookiePhase, AutoCookieWindowAction, PendingAutoCookie,
    activation_actions_for_cookie, auto_cookie_threshold, dry_run_auto_cookie_action,
    full_inventory_cookie_message, should_buy_cookie,
};
use anyhow::{Context, Result};
use saf_core::ports::{AccountStatsProvider, InventoryProvider, MinecraftAction, MinecraftClient};
use saf_core::{AccountId, BotState};
use std::time::{Duration, Instant};

impl LiveRuntime {
    pub(in crate::live_runtime) fn record_cookie_duration_best_effort(
        &self,
        account: &AccountId,
        duration: Duration,
    ) {
        if let Err(error) = self.stats.record_cookie_duration(account, duration) {
            tracing::warn!(
                account = %account,
                error = %error,
                "cookie duration update failed; continuing runtime loop"
            );
        }
    }

    pub(in crate::live_runtime) async fn start_auto_cookie_if_needed(
        &mut self,
        account: &AccountId,
        remaining: Option<Duration>,
    ) -> Result<()> {
        if self.pending_auto_cookies.contains_key(account)
            || !self.config.use_cookie
            || !self.config.relist
        {
            return Ok(());
        }
        let Some(threshold) = auto_cookie_threshold(&self.config.auto_cookie) else {
            tracing::debug!(account = %account, "auto-cookie is disabled");
            return Ok(());
        };
        if let Some(remaining) = remaining
            && remaining > threshold
        {
            tracing::debug!(account = %account, "auto-cookie skipped because cookie duration is above threshold");
            return Ok(());
        }
        let purse = match self.stats.stats(account).await {
            Ok(stats) => stats.purse,
            Err(error) => {
                tracing::warn!(
                    account = %account,
                    error = %error,
                    "failed to load purse for auto-cookie check"
                );
                None
            }
        };
        let price = match self.cookie_prices.booster_cookie_buy_price().await {
            Ok(price) => price,
            Err(error) => {
                tracing::warn!(
                    account = %account,
                    error = %error,
                    "failed to load booster cookie price"
                );
                None
            }
        };
        match should_buy_cookie(Some(threshold), remaining, purse, price) {
            AutoCookieDecision::Buy { price } => {
                if !self.options.market_actions.allows_market_actions() {
                    self.queue.record_dry_run_action(
                        account,
                        BotState::Custom("autoCookie".to_string()),
                        2,
                        dry_run_auto_cookie_action(remaining, threshold, price),
                    )?;
                    return Ok(());
                }
                let Some(client) = self.minecraft_clients.get(account).cloned() else {
                    tracing::warn!(
                        account = %account,
                        "auto-cookie buy skipped because no Minecraft client is registered"
                    );
                    return Ok(());
                };
                self.pending_auto_cookies.insert(
                    account.clone(),
                    PendingAutoCookie::new(price, remaining, Instant::now()),
                );
                if !self
                    .perform_auto_cookie_action(
                        account,
                        client.as_ref(),
                        MinecraftAction::Chat("/bz booster cookie".to_string()),
                        "open booster cookie bazaar",
                    )
                    .await?
                {
                    self.pending_auto_cookies.remove(account);
                }
            }
            AutoCookieDecision::Disabled => {
                tracing::debug!(account = %account, "auto-cookie is disabled");
            }
            AutoCookieDecision::EnoughTime => {
                tracing::debug!(account = %account, "auto-cookie skipped because cookie duration is above threshold");
            }
            AutoCookieDecision::MissingPurse => {
                tracing::warn!(account = %account, "auto-cookie skipped because purse is unknown");
            }
            AutoCookieDecision::PriceUnavailable => {
                tracing::warn!(account = %account, "auto-cookie skipped because booster cookie price is unavailable");
            }
            AutoCookieDecision::TooExpensive { price, purse } => {
                tracing::warn!(
                    account = %account,
                    price,
                    purse,
                    "auto-cookie skipped because cookie price is above the safety limit or purse is too low"
                );
            }
        }
        Ok(())
    }

    pub(in crate::live_runtime) async fn process_auto_cookie_window(
        &mut self,
        account: &AccountId,
        client: &dyn MinecraftClient,
    ) -> Result<bool> {
        let Some(window) = self
            .active_windows
            .lock()
            .map_err(|_| anyhow::anyhow!("active window lock poisoned"))?
            .get(account)
            .cloned()
        else {
            return Ok(false);
        };
        let Some(action) = self
            .pending_auto_cookies
            .get(account)
            .and_then(|pending| pending.plan_window(&window, Instant::now()))
        else {
            return Ok(false);
        };

        match action {
            AutoCookieWindowAction::Click { slot, next } => {
                if self
                    .perform_auto_cookie_action(
                        account,
                        client,
                        MinecraftAction::ClickSlot(slot),
                        "click auto-cookie bazaar step",
                    )
                    .await?
                    && let Some(pending) = self.pending_auto_cookies.get_mut(account)
                {
                    pending.phase = next;
                }
            }
            AutoCookieWindowAction::ClickAndPrepareActivation { slot, due_at } => {
                if self
                    .perform_auto_cookie_action(
                        account,
                        client,
                        MinecraftAction::ClickSlot(slot),
                        "confirm booster cookie buy",
                    )
                    .await?
                {
                    let _ = client.perform(MinecraftAction::CloseWindow).await;
                    self.clear_active_window_cache(account)?;
                    if let Some(pending) = self.pending_auto_cookies.get_mut(account) {
                        pending.phase = AutoCookiePhase::PreparingActivation { due_at };
                    }
                }
            }
            AutoCookieWindowAction::ClickConsume { slot } => {
                let activated_duration = self
                    .pending_auto_cookies
                    .get(account)
                    .map(PendingAutoCookie::activated_cookie_duration);
                if self
                    .perform_auto_cookie_action(
                        account,
                        client,
                        MinecraftAction::ClickSlot(slot),
                        "consume booster cookie",
                    )
                    .await?
                {
                    let _ = client.perform(MinecraftAction::CloseWindow).await;
                    self.clear_active_window_cache(account)?;
                    if let Some(duration) = activated_duration {
                        self.record_cookie_duration_best_effort(account, duration);
                    }
                    self.pending_auto_cookies.remove(account);
                    self.notify_if_startup_ready(account).await;
                }
            }
        }
        Ok(true)
    }

    pub(in crate::live_runtime) async fn process_auto_cookie_chat(
        &mut self,
        account: &AccountId,
        client: &dyn MinecraftClient,
        text: &str,
    ) -> Result<bool> {
        if !full_inventory_cookie_message(text) || !self.pending_auto_cookies.contains_key(account)
        {
            return Ok(false);
        }
        let price = self
            .pending_auto_cookies
            .get(account)
            .map(|pending| pending.price);
        let _ = client.perform(MinecraftAction::CloseWindow).await;
        self.clear_active_window_cache(account)?;
        self.pending_auto_cookies.remove(account);
        tracing::warn!(
            account = %account,
            price = price.unwrap_or_default(),
            "auto-cookie bought a booster cookie but Hypixel moved it to the stash"
        );
        self.notify_if_startup_ready(account).await;
        Ok(true)
    }

    pub(in crate::live_runtime) async fn drive_auto_cookies_once(&mut self) -> Result<()> {
        let now = Instant::now();
        let expired = self
            .pending_auto_cookies
            .iter()
            .filter(|(_, pending)| pending.timed_out(now))
            .map(|(account, _)| account.clone())
            .collect::<Vec<_>>();
        let due_activation = self
            .pending_auto_cookies
            .iter()
            .filter_map(|(account, pending)| match pending.phase {
                AutoCookiePhase::PreparingActivation { due_at } if due_at <= now => {
                    Some(account.clone())
                }
                _ => None,
            })
            .collect::<Vec<_>>();

        for account in expired {
            self.pending_auto_cookies.remove(&account);
            if let Some(client) = self.minecraft_clients.get(&account) {
                let _ = client.perform(MinecraftAction::CloseWindow).await;
            }
            self.clear_active_window_cache(&account)?;
            tracing::warn!(account = %account, "auto-cookie timed out; releasing startup gate");
            self.notify_if_startup_ready(&account).await;
        }

        for account in due_activation {
            self.activate_pending_auto_cookie(&account).await?;
        }
        Ok(())
    }

    async fn activate_pending_auto_cookie(&mut self, account: &AccountId) -> Result<()> {
        let Some(managed) = self.managed_minecraft.get(account).cloned() else {
            self.finish_auto_cookie_without_activation(account, "no managed Minecraft client")
                .await;
            return Ok(());
        };
        let actions = match managed.snapshot(account).await {
            Ok(snapshot) => activation_actions_for_cookie(&snapshot.items),
            Err(error) => {
                tracing::warn!(
                    account = %account,
                    error = %error,
                    "auto-cookie inventory snapshot failed"
                );
                None
            }
        };
        let Some(actions) = actions else {
            self.finish_auto_cookie_without_activation(
                account,
                "booster cookie was not in inventory or no empty hotbar slot was available",
            )
            .await;
            return Ok(());
        };
        let Some(client) = self.minecraft_clients.get(account).cloned() else {
            self.finish_auto_cookie_without_activation(account, "no Minecraft client")
                .await;
            return Ok(());
        };
        for action in actions {
            if !self
                .perform_auto_cookie_action(
                    account,
                    client.as_ref(),
                    action,
                    "activate booster cookie",
                )
                .await?
            {
                return Ok(());
            }
        }
        if let Some(pending) = self.pending_auto_cookies.get_mut(account) {
            pending.phase = AutoCookiePhase::AwaitingConsumeConfirm;
        }
        Ok(())
    }

    async fn finish_auto_cookie_without_activation(&mut self, account: &AccountId, reason: &str) {
        self.pending_auto_cookies.remove(account);
        tracing::warn!(account = %account, reason = reason, "auto-cookie stopped before activation");
        self.notify_if_startup_ready(account).await;
    }

    async fn perform_auto_cookie_action(
        &mut self,
        account: &AccountId,
        client: &dyn MinecraftClient,
        action: MinecraftAction,
        context: &str,
    ) -> Result<bool> {
        let result = client
            .perform(action.clone())
            .await
            .with_context(|| format!("{context} for {account}"));
        match result {
            Ok(()) => Ok(true),
            Err(error) => {
                self.handle_minecraft_action_failure(account, context, error)
                    .await?;
                Ok(false)
            }
        }
    }
}
