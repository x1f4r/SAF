use super::stats::LiveStatsProvider;
use super::windows::{active_auction_summaries, is_manage_auctions_window, window_title_matches};
use super::{
    DeferredMinecraftEvents, MarketActionMode, next_minecraft_event, push_deferred_minecraft_event,
};
use async_trait::async_trait;
use saf_core::AccountId;
use saf_core::gui::WindowSnapshot;
use saf_core::ports::{
    ActiveAuction, ActiveAuctionProvider, GuiDiagnosticsProvider, GuiSlotDiagnostics,
    MinecraftAction, MinecraftClient, MinecraftEvent, PortError,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[derive(Clone)]
pub(super) struct LiveActiveAuctionProvider {
    pub(super) clients: BTreeMap<AccountId, Arc<dyn MinecraftClient>>,
    pub(super) active_windows: Arc<Mutex<BTreeMap<AccountId, WindowSnapshot>>>,
    pub(super) deferred_events: DeferredMinecraftEvents,
    pub(super) stats: Arc<LiveStatsProvider>,
    pub(super) mode: MarketActionMode,
    pub(super) wait_timeout: Duration,
}

impl LiveActiveAuctionProvider {
    pub(super) fn new(
        clients: BTreeMap<AccountId, Arc<dyn MinecraftClient>>,
        active_windows: Arc<Mutex<BTreeMap<AccountId, WindowSnapshot>>>,
        deferred_events: DeferredMinecraftEvents,
        stats: Arc<LiveStatsProvider>,
        mode: MarketActionMode,
    ) -> Self {
        Self {
            clients,
            active_windows,
            deferred_events,
            stats,
            mode,
            wait_timeout: Duration::from_secs(5),
        }
    }

    fn cached_window(&self, account: &AccountId) -> Result<Option<WindowSnapshot>, PortError> {
        Ok(self
            .active_windows
            .lock()
            .map_err(|_| PortError::Failed("active window lock poisoned".to_string()))?
            .get(account)
            .cloned())
    }

    fn remember_window(
        &self,
        account: &AccountId,
        window: WindowSnapshot,
    ) -> Result<(), PortError> {
        if let Err(error) = self.stats.record_window_snapshot(account, &window) {
            tracing::warn!(
                account = %account,
                error = %error,
                "failed to record active-auction scan window stats; caching window snapshot anyway"
            );
        }
        self.active_windows
            .lock()
            .map_err(|_| PortError::Failed("active window lock poisoned".to_string()))?
            .insert(account.clone(), window);
        Ok(())
    }

    fn forget_window(&self, account: &AccountId) -> Result<(), PortError> {
        self.active_windows
            .lock()
            .map_err(|_| PortError::Failed("active window lock poisoned".to_string()))?
            .remove(account);
        Ok(())
    }

    fn defer_event(&self, account: &AccountId, event: MinecraftEvent) -> Result<(), PortError> {
        push_deferred_minecraft_event(&self.deferred_events, account, event)
            .map_err(PortError::Failed)
    }

    async fn wait_for_window(
        &self,
        account: &AccountId,
        client: &dyn MinecraftClient,
        title_patterns: &[&str],
        required_slot_patterns: &[&str],
        context: &str,
    ) -> Result<WindowSnapshot, PortError> {
        let deadline = Instant::now() + self.wait_timeout;
        loop {
            if let Some(window) = self.cached_window(account)?
                && window_title_matches(&window, title_patterns)
                && window_has_required_slots(&window, required_slot_patterns)
            {
                return Ok(window);
            }

            match next_minecraft_event(client).await {
                Ok(Some(MinecraftEvent::WindowOpen(window))) => {
                    let matches = window_title_matches(&window, title_patterns);
                    let has_required_slots =
                        window_has_required_slots(&window, required_slot_patterns);
                    self.remember_window(account, window.clone())?;
                    if matches && has_required_slots {
                        return Ok(window);
                    }
                }
                Ok(Some(event @ MinecraftEvent::WindowClosed)) => {
                    self.forget_window(account)?;
                    self.defer_event(account, event)?;
                }
                Ok(Some(
                    event @ (MinecraftEvent::Kicked { .. } | MinecraftEvent::Disconnected { .. }),
                )) => {
                    let reason = match &event {
                        MinecraftEvent::Kicked { reason }
                        | MinecraftEvent::Disconnected { reason } => reason.clone(),
                        _ => String::new(),
                    };
                    self.forget_window(account)?;
                    self.defer_event(account, event)?;
                    return Err(PortError::Unavailable(format!(
                        "{account} disconnected while {context}: {reason}"
                    )));
                }
                Ok(Some(
                    event @ (MinecraftEvent::Ready { .. } | MinecraftEvent::ChatMessage { .. }),
                )) => {
                    self.defer_event(account, event)?;
                }
                Ok(None) => {}
                Ok(Some(event @ MinecraftEvent::Scoreboard { .. })) => {
                    if let MinecraftEvent::Scoreboard { lines } = &event
                        && let Err(error) = self.stats.record_scoreboard(account, lines)
                    {
                        tracing::warn!(
                            account = %account,
                            error = %error,
                            "scoreboard stats update failed during GUI scan"
                        );
                    }
                    self.defer_event(account, event)?;
                }
                Err(error) => return Err(PortError::Failed(error.to_string())),
            }

            if Instant::now() >= deadline {
                return Err(PortError::Unavailable(format!(
                    "timed out waiting for {} window while {context}",
                    title_patterns.join("/")
                )));
            }
            sleep(Duration::from_millis(50)).await;
        }
    }
}

#[async_trait]
impl ActiveAuctionProvider for LiveActiveAuctionProvider {
    async fn active_auctions(&self, account: &AccountId) -> Result<Vec<ActiveAuction>, PortError> {
        if let Some(window) = self.cached_window(account)?
            && is_manage_auctions_window(&window)
        {
            return Ok(active_auction_summaries(&window));
        }
        if !self.mode.allows_market_actions() {
            return Err(PortError::Unavailable(format!(
                "dry-run active auction scan for {account} requires a cached Manage Auctions window"
            )));
        }

        let client = self.clients.get(account).ok_or_else(|| {
            PortError::Unavailable(format!("no Minecraft client registered for {account}"))
        })?;
        client
            .perform(MinecraftAction::Chat("/ah".to_string()))
            .await?;
        let auction_house = self
            .wait_for_window(
                account,
                client.as_ref(),
                &["auction house"],
                &[
                    "manage auctions",
                    "your auctions",
                    "create auction",
                    "auction browser",
                ],
                "scanning active auctions",
            )
            .await?;
        let Some(manage_slot) =
            auction_house.resolve_slot(&["manage auctions", "your auctions"], None)
        else {
            if let Err(error) = client.perform(MinecraftAction::CloseWindow).await {
                tracing::warn!(
                    account = %account,
                    error = %error,
                    "failed to close auction-house scan window without auction management"
                );
            }
            if let Err(error) = self.forget_window(account) {
                tracing::warn!(
                    account = %account,
                    error = %error,
                    "failed to clear cached auction-house scan window without auction management"
                );
            }
            return Ok(Vec::new());
        };
        client
            .perform(MinecraftAction::ClickSlot(manage_slot))
            .await?;
        let manage = self
            .wait_for_window(
                account,
                client.as_ref(),
                &["manage auctions"],
                &[],
                "scanning active auctions",
            )
            .await?;
        let auctions = active_auction_summaries(&manage);
        if let Err(error) = client.perform(MinecraftAction::CloseWindow).await {
            tracing::warn!(
                account = %account,
                error = %error,
                "failed to close active-auction scan window"
            );
        }
        if let Err(error) = self.forget_window(account) {
            tracing::warn!(
                account = %account,
                error = %error,
                "failed to clear cached active-auction scan window"
            );
        }
        Ok(auctions)
    }
}

#[async_trait]
impl GuiDiagnosticsProvider for LiveActiveAuctionProvider {
    async fn diagnose_slots(
        &self,
        account: &AccountId,
        target: Option<&str>,
    ) -> Result<GuiSlotDiagnostics, PortError> {
        let targets = gui_diagnostic_targets(target);
        let mut windows = Vec::new();
        if targets.is_empty()
            && let Some(window) = self.cached_window(account)?
        {
            push_unique_window(&mut windows, window);
        }

        for target in targets {
            let Some(client) = self.clients.get(account) else {
                return Err(PortError::Unavailable(format!(
                    "no Minecraft client registered for {account}"
                )));
            };
            client
                .perform(MinecraftAction::Chat(target.command.to_string()))
                .await?;
            match self
                .wait_for_window(
                    account,
                    client.as_ref(),
                    target.title_patterns,
                    target.required_slot_patterns,
                    "diagnosing GUI slots",
                )
                .await
            {
                Ok(window) => {
                    push_unique_window(&mut windows, window.clone());
                    if let Some(follow_up) = target.follow_up {
                        let slot = window
                            .resolve_slot(follow_up.slot_patterns, Some(follow_up.fallback_slot))
                            .unwrap_or(follow_up.fallback_slot);
                        client.perform(MinecraftAction::ClickSlot(slot)).await?;
                        match self
                            .wait_for_window(
                                account,
                                client.as_ref(),
                                follow_up.title_patterns,
                                follow_up.required_slot_patterns,
                                "diagnosing GUI slots",
                            )
                            .await
                        {
                            Ok(window) => push_unique_window(&mut windows, window),
                            Err(PortError::Unavailable(message))
                                if !is_window_wait_disconnect(&message) => {}
                            Err(error) => return Err(error),
                        }
                    }
                    if let Err(error) = client.perform(MinecraftAction::CloseWindow).await {
                        tracing::warn!(
                            account = %account,
                            target = %target.command,
                            error = %error,
                            "failed to close diagnostic window"
                        );
                    }
                    if let Err(error) = self.forget_window(account) {
                        tracing::warn!(
                            account = %account,
                            target = %target.command,
                            error = %error,
                            "failed to clear cached diagnostic window"
                        );
                    }
                }
                Err(PortError::Unavailable(message)) if !is_window_wait_disconnect(&message) => {}
                Err(error) => return Err(error),
            }
        }

        if windows.is_empty() {
            return Err(PortError::Unavailable(format!(
                "no GUI window snapshot is available for {account}"
            )));
        }

        Ok(GuiSlotDiagnostics {
            account: account.clone(),
            target: target
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string),
            windows,
        })
    }
}

fn push_unique_window(windows: &mut Vec<WindowSnapshot>, window: WindowSnapshot) {
    if !windows
        .iter()
        .any(|existing| existing.title == window.title && existing.slots == window.slots)
    {
        windows.push(window);
    }
}

fn is_window_wait_disconnect(message: &str) -> bool {
    message.contains(" disconnected while ")
}

fn window_has_required_slots(window: &WindowSnapshot, required_slot_patterns: &[&str]) -> bool {
    required_slot_patterns.is_empty()
        || required_slot_patterns.iter().any(|pattern| {
            let pattern = pattern.to_ascii_lowercase();
            window
                .slots
                .iter()
                .any(|slot| slot.text().contains(&pattern))
        })
}

#[derive(Clone, Copy)]
struct GuiDiagnosticTarget {
    command: &'static str,
    title_patterns: &'static [&'static str],
    required_slot_patterns: &'static [&'static str],
    follow_up: Option<GuiDiagnosticFollowUp>,
}

#[derive(Clone, Copy)]
struct GuiDiagnosticFollowUp {
    slot_patterns: &'static [&'static str],
    fallback_slot: usize,
    title_patterns: &'static [&'static str],
    required_slot_patterns: &'static [&'static str],
}

fn gui_diagnostic_targets(target: Option<&str>) -> Vec<GuiDiagnosticTarget> {
    const AUCTION_HOUSE: GuiDiagnosticTarget = GuiDiagnosticTarget {
        command: "/ah",
        title_patterns: &["auction house"],
        required_slot_patterns: &[
            "manage auctions",
            "your auctions",
            "create auction",
            "auction browser",
        ],
        follow_up: None,
    };
    const MANAGE_AUCTIONS: GuiDiagnosticTarget = GuiDiagnosticTarget {
        command: "/ah",
        title_patterns: &["auction house"],
        required_slot_patterns: &["manage auctions", "your auctions"],
        follow_up: Some(GuiDiagnosticFollowUp {
            slot_patterns: &["manage auctions", "your auctions"],
            fallback_slot: 15,
            title_patterns: &["manage auctions"],
            required_slot_patterns: &[
                "auction slots",
                "create auction",
                "seller:",
                "buyer:",
                "claim all",
                "expired",
            ],
        }),
    };
    const BANK: GuiDiagnosticTarget = GuiDiagnosticTarget {
        command: "/bank",
        title_patterns: &["bank"],
        required_slot_patterns: &[],
        follow_up: None,
    };
    const PROFILES: GuiDiagnosticTarget = GuiDiagnosticTarget {
        command: "/profiles",
        title_patterns: &["profiles"],
        required_slot_patterns: &[],
        follow_up: None,
    };
    const SKYBLOCK_MENU: GuiDiagnosticTarget = GuiDiagnosticTarget {
        command: "/sbmenu",
        title_patterns: &["skyblock menu", "skyblock"],
        required_slot_patterns: &[],
        follow_up: None,
    };

    let normalized = target
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("current")
        .to_ascii_lowercase();
    match normalized.as_str() {
        "all" => vec![AUCTION_HOUSE, BANK, PROFILES, SKYBLOCK_MENU],
        "ah" | "auction" | "auctionhouse" | "auction_house" | "bids" => vec![AUCTION_HOUSE],
        "manage" | "manage_auctions" | "auctions" | "auction_management" => {
            vec![MANAGE_AUCTIONS]
        }
        "bank" | "coins" => vec![BANK],
        "profiles" | "profile" => vec![PROFILES],
        "sbmenu" | "menu" | "skyblock" | "skyblock_menu" => vec![SKYBLOCK_MENU],
        "current" | "cache" | "cached" => Vec::new(),
        _ => Vec::new(),
    }
}
