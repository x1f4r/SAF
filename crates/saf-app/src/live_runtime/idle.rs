//! Per-account idle "anti-AFK" behaviour.
//!
//! A frozen avatar that never moves or looks around is one of the loudest
//! tells that an account is bot-controlled. This module spawns one cheap
//! background task per account that, while the account is running, NOT halted,
//! and NOT in the middle of a market flow, emits small randomised look deltas
//! (and rarely an in-place jump) at human-plausible randomised intervals.
//!
//! Hard safety rules baked in here:
//! - It NEVER issues a positional translation (no walking), only rotation and
//!   in-place jumps, so it cannot walk the avatar off its island.
//! - It pauses entirely whenever a GUI window is open or a live buy is pending
//!   (i.e. an active buy/list/claim/reconcile flow), so it never perturbs a
//!   click sequence.
//! - It respects the kill switch: while `halted` (operator Stop All) or paused
//!   it does nothing, and it never reconnects a stopped client.
//! - Every error from performing an action is logged and swallowed; the task
//!   can never crash the runtime.

use super::minecraft::ManagedMinecraftClient;
use saf_core::gui::WindowSnapshot;
use saf_core::ports::{MinecraftAction, MinecraftClient};
use saf_core::{AccountId, Humanizer, IdleAction, RuntimeSession};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tokio::time::sleep;

#[cfg(feature = "live-cofl")]
use super::cofl::PendingLiveBuy;

/// References an idle task needs to decide whether it may act this tick and to
/// perform the gesture. Cloning is cheap (all `Arc`s).
#[derive(Clone)]
pub(in crate::live_runtime) struct IdleBehaviorContext {
    pub(in crate::live_runtime) account: AccountId,
    pub(in crate::live_runtime) session: Arc<RuntimeSession>,
    pub(in crate::live_runtime) minecraft: Arc<dyn MinecraftClient>,
    pub(in crate::live_runtime) managed: Arc<ManagedMinecraftClient>,
    pub(in crate::live_runtime) humanizer: Arc<Humanizer>,
    pub(in crate::live_runtime) shutdown: Arc<AtomicBool>,
    pub(in crate::live_runtime) halted: Arc<AtomicBool>,
    pub(in crate::live_runtime) active_windows: Arc<Mutex<BTreeMap<AccountId, WindowSnapshot>>>,
    #[cfg(feature = "live-cofl")]
    pub(in crate::live_runtime) pending_live_buys: Arc<Mutex<BTreeMap<AccountId, PendingLiveBuy>>>,
}

impl IdleBehaviorContext {
    /// True when this account is currently listed as running by the session.
    fn account_running(&self) -> bool {
        self.session
            .running_accounts()
            .iter()
            .any(|running| running == self.account.as_str())
    }

    /// True when a GUI window is open for this account — i.e. a market/menu
    /// flow (buy/list/claim/reconcile/bank) is in progress.
    fn window_open(&self) -> bool {
        self.active_windows
            .lock()
            .map(|windows| windows.contains_key(&self.account))
            .unwrap_or(true)
    }

    /// True when a live buy is pending for this account.
    #[cfg(feature = "live-cofl")]
    fn live_buy_pending(&self) -> bool {
        self.pending_live_buys
            .lock()
            .map(|pending| pending.contains_key(&self.account))
            .unwrap_or(true)
    }

    #[cfg(not(feature = "live-cofl"))]
    fn live_buy_pending(&self) -> bool {
        false
    }

    /// The single gate that decides whether an idle gesture may fire right now.
    /// Conservative on every axis: any uncertainty (poisoned lock, halted,
    /// stopped client) resolves to "do not act".
    fn may_act_now(&self) -> bool {
        if self.shutdown.load(Ordering::SeqCst) {
            return false;
        }
        if self.halted.load(Ordering::SeqCst) || super::lifecycle::startup_paused() {
            return false;
        }
        if !self.account_running() {
            return false;
        }
        // Never perturb an in-flight market flow.
        if self.window_open() || self.live_buy_pending() {
            return false;
        }
        true
    }

    /// Perform one idle gesture if allowed. Returns the action that was sent
    /// (for tests/telemetry) or `None` if the tick was skipped. Errors from the
    /// underlying client are logged and swallowed.
    pub(in crate::live_runtime) async fn act_once(&self) -> Option<IdleAction> {
        if !self.may_act_now() {
            return None;
        }
        // Only act on a live runtime; never force a stopped client back online.
        if !self.managed.has_active_runtime().await {
            return None;
        }
        let idle_action = self.humanizer.sample_idle_action()?;
        let action = match idle_action {
            IdleAction::Look {
                yaw_delta,
                pitch_delta,
            } => MinecraftAction::LookDelta {
                yaw_delta,
                pitch_delta,
            },
            IdleAction::Jump => MinecraftAction::Jump,
        };
        if let Err(error) = self.minecraft.perform(action).await {
            tracing::debug!(
                account = %self.account,
                error = %error,
                "idle anti-AFK gesture failed; ignoring"
            );
            return None;
        }
        tracing::trace!(account = %self.account, action = ?idle_action, "performed idle anti-AFK gesture");
        Some(idle_action)
    }
}

/// Spawn one idle-behaviour task per account. Tasks exit when `shutdown` is set.
/// When idle behaviour is disabled in config no tasks are spawned.
pub(in crate::live_runtime) fn start_idle_behavior_tasks(
    contexts: Vec<IdleBehaviorContext>,
) -> Vec<tokio::task::JoinHandle<()>> {
    contexts
        .into_iter()
        .filter(|context| context.humanizer.config().idle.enabled)
        .map(|context| {
            tokio::spawn(async move {
                loop {
                    let delay = context.humanizer.next_idle_delay();
                    sleep(delay).await;
                    if context.shutdown.load(Ordering::SeqCst) {
                        return;
                    }
                    // act_once swallows its own errors; a panic guard is not
                    // needed because nothing here can panic, but if a future
                    // change introduced one the JoinHandle is simply dropped.
                    let _ = context.act_once().await;
                }
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use saf_core::{BotRuntime, HumanizerConfig, IdleBehaviorConfig, SafConfig};
    use saf_minecraft::RecordedMinecraftClient;

    fn humanizer_always_acting() -> Arc<Humanizer> {
        // jump_probability 0 so every gesture is a Look (deterministic shape),
        // tiny intervals so the scheduler test is fast.
        let cfg = HumanizerConfig {
            idle: IdleBehaviorConfig {
                enabled: true,
                min_interval_ms: 1,
                max_interval_ms: 1,
                jump_probability: 0.0,
                ..IdleBehaviorConfig::default()
            },
            ..HumanizerConfig::default()
        };
        Arc::new(Humanizer::seeded(cfg, 7))
    }

    fn make_context(
        account: &AccountId,
        humanizer: Arc<Humanizer>,
        running: bool,
        halted: bool,
        window_open: bool,
        recorder: Arc<RecordedMinecraftClient>,
    ) -> IdleBehaviorContext {
        let running_igns = if running {
            vec![account.to_string()]
        } else {
            Vec::new()
        };
        let session = Arc::new(RuntimeSession::new(BotRuntime::from_config(
            &SafConfig::default(),
            running_igns,
        )));
        let managed = Arc::new(ManagedMinecraftClient::from_bundle(
            account.clone(),
            crate::live_runtime::MarketActionMode::Live,
            crate::live_runtime::LiveMinecraftClientBundle {
                minecraft: recorder.clone(),
                inventory_provider: None,
            },
            None,
        ));
        let active_windows = Arc::new(Mutex::new(BTreeMap::new()));
        if window_open {
            active_windows.lock().unwrap().insert(
                account.clone(),
                WindowSnapshot {
                    title: "Auction View".to_string(),
                    slots: Vec::new(),
                },
            );
        }
        IdleBehaviorContext {
            account: account.clone(),
            session,
            minecraft: recorder,
            managed,
            humanizer,
            shutdown: Arc::new(AtomicBool::new(false)),
            halted: Arc::new(AtomicBool::new(halted)),
            active_windows,
            #[cfg(feature = "live-cofl")]
            pending_live_buys: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    #[tokio::test]
    async fn acts_when_running_idle_and_not_halted() {
        let account = AccountId::new("Main").unwrap();
        let recorder = Arc::new(RecordedMinecraftClient::new(account.clone()));
        let context = make_context(
            &account,
            humanizer_always_acting(),
            true,
            false,
            false,
            recorder.clone(),
        );
        let action = context.act_once().await;
        assert!(
            matches!(action, Some(IdleAction::Look { .. })),
            "expected a look gesture, got {action:?}"
        );
        let recorded = recorder.actions();
        assert_eq!(recorded.len(), 1);
        assert!(matches!(recorded[0], MinecraftAction::LookDelta { .. }));
    }

    #[tokio::test]
    async fn does_not_act_when_halted() {
        let account = AccountId::new("Main").unwrap();
        let recorder = Arc::new(RecordedMinecraftClient::new(account.clone()));
        let context = make_context(
            &account,
            humanizer_always_acting(),
            true,
            true, // halted
            false,
            recorder.clone(),
        );
        assert_eq!(context.act_once().await, None);
        assert!(
            recorder.actions().is_empty(),
            "halted runtime must not move"
        );
    }

    #[tokio::test]
    async fn does_not_act_when_account_not_running() {
        let account = AccountId::new("Main").unwrap();
        let recorder = Arc::new(RecordedMinecraftClient::new(account.clone()));
        let context = make_context(
            &account,
            humanizer_always_acting(),
            false, // not running
            false,
            false,
            recorder.clone(),
        );
        assert_eq!(context.act_once().await, None);
        assert!(recorder.actions().is_empty());
    }

    #[tokio::test]
    async fn does_not_act_while_a_window_is_open() {
        let account = AccountId::new("Main").unwrap();
        let recorder = Arc::new(RecordedMinecraftClient::new(account.clone()));
        let context = make_context(
            &account,
            humanizer_always_acting(),
            true,
            false,
            true, // window open -> market flow in progress
            recorder.clone(),
        );
        assert_eq!(context.act_once().await, None);
        assert!(
            recorder.actions().is_empty(),
            "must not perturb an open GUI window"
        );
    }

    #[tokio::test]
    async fn does_not_act_when_idle_behaviour_disabled() {
        let account = AccountId::new("Main").unwrap();
        let recorder = Arc::new(RecordedMinecraftClient::new(account.clone()));
        let cfg = HumanizerConfig {
            idle: IdleBehaviorConfig {
                enabled: false,
                min_interval_ms: 1,
                max_interval_ms: 1,
                ..IdleBehaviorConfig::default()
            },
            ..HumanizerConfig::default()
        };
        let context = make_context(
            &account,
            Arc::new(Humanizer::seeded(cfg, 1)),
            true,
            false,
            false,
            recorder.clone(),
        );
        assert_eq!(context.act_once().await, None);
        assert!(recorder.actions().is_empty());
    }
}
