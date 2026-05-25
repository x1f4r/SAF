use super::super::{notify_operator_best_effort, now_ms};
use saf_core::ports::{Notification, ScheduledAccountAction};
use saf_core::{AccountId, RuntimeDirective, RuntimeSession, SafConfig};
use std::collections::BTreeSet;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::time::sleep;

pub(in crate::live_runtime) fn runtime_accounts(config: &SafConfig) -> Vec<AccountId> {
    config
        .configured_igns()
        .into_iter()
        .filter_map(AccountId::new)
        .collect()
}

pub(in crate::live_runtime) fn configured_startup_runtime_accounts(
    config: &SafConfig,
) -> Vec<AccountId> {
    config
        .startup_igns()
        .into_iter()
        .filter_map(AccountId::new)
        .collect()
}

pub(in crate::live_runtime) fn startup_runtime_accounts_with_rotation(
    startup_accounts: &[AccountId],
    schedules: &[AutoRotateSchedule],
) -> Vec<AccountId> {
    let rest_first = schedules
        .iter()
        .filter(|schedule| schedule.rest_first())
        .map(|schedule| schedule.account.clone())
        .collect::<BTreeSet<_>>();
    startup_accounts
        .iter()
        .filter(|account| !rest_first.contains(*account))
        .cloned()
        .collect()
}

pub(in crate::live_runtime) fn auto_rotate_schedules(
    config: &SafConfig,
    startup_accounts: &[AccountId],
) -> Vec<AutoRotateSchedule> {
    startup_accounts
        .iter()
        .filter_map(|account| auto_rotate_schedule(config, account))
        .collect()
}

fn auto_rotate_schedule(config: &SafConfig, account: &AccountId) -> Option<AutoRotateSchedule> {
    config
        .auto_rotate
        .get(account.as_str())
        .and_then(|value| parse_auto_rotate_schedule(account, value))
}

pub(in crate::live_runtime) fn parse_auto_rotate_schedule(
    account: &AccountId,
    value: &str,
) -> Option<AutoRotateSchedule> {
    let (first, second) = value.split_once(':')?;
    let first = parse_auto_rotate_leg(first)?;
    let second = parse_auto_rotate_leg(second)?;
    Some(AutoRotateSchedule {
        account: account.clone(),
        first,
        second,
    })
}

fn parse_auto_rotate_leg(value: &str) -> Option<AutoRotateLeg> {
    let lower = value.trim().to_ascii_lowercase();
    let phase = if lower.contains('r') {
        AutoRotatePhase::Rest
    } else if lower.contains('f') {
        AutoRotatePhase::Flip
    } else {
        return None;
    };
    let hours = lower
        .trim_matches(|character: char| character == 'r' || character == 'f')
        .trim()
        .parse::<f64>()
        .ok()?;
    if !hours.is_finite() || hours <= 0.0 {
        return None;
    }
    let millis = (hours * 3_600_000.0).round().max(1.0) as u64;
    Some(AutoRotateLeg {
        phase,
        duration: Duration::from_millis(millis),
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::live_runtime) struct AutoRotateSchedule {
    account: AccountId,
    first: AutoRotateLeg,
    second: AutoRotateLeg,
}

impl AutoRotateSchedule {
    pub(in crate::live_runtime) fn rest_first(&self) -> bool {
        self.first.phase == AutoRotatePhase::Rest
    }

    pub(in crate::live_runtime) fn action_steps(
        &self,
    ) -> [(Duration, ScheduledAccountAction, Duration); 2] {
        if self.rest_first() {
            [
                (
                    self.second.duration,
                    ScheduledAccountAction::Start,
                    self.first.duration,
                ),
                (
                    self.first.duration,
                    ScheduledAccountAction::Stop,
                    self.second.duration,
                ),
            ]
        } else {
            [
                (
                    self.second.duration,
                    ScheduledAccountAction::Stop,
                    self.first.duration,
                ),
                (
                    self.first.duration,
                    ScheduledAccountAction::Start,
                    self.second.duration,
                ),
            ]
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AutoRotateLeg {
    phase: AutoRotatePhase,
    duration: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AutoRotatePhase {
    Rest,
    Flip,
}

pub(in crate::live_runtime) fn start_auto_rotate_tasks(
    schedules: Vec<AutoRotateSchedule>,
    session: Arc<RuntimeSession>,
    shutdown: Arc<AtomicBool>,
) -> Vec<tokio::task::JoinHandle<()>> {
    schedules
        .into_iter()
        .map(|schedule| {
            let session = session.clone();
            let shutdown = shutdown.clone();
            tokio::spawn(async move {
                if schedule.rest_first() {
                    notify_auto_rotate_waiting(&session, &schedule).await;
                }
                loop {
                    for (delay, action, next_delay) in schedule.action_steps() {
                        sleep(delay).await;
                        if shutdown.load(Ordering::SeqCst) {
                            return;
                        }
                        execute_auto_rotate_action(&session, &schedule.account, action, next_delay)
                            .await;
                    }
                }
            })
        })
        .collect()
}

async fn execute_auto_rotate_action(
    session: &RuntimeSession,
    account: &AccountId,
    action: ScheduledAccountAction,
    next_delay: Duration,
) {
    let directive = match &action {
        ScheduledAccountAction::Start => RuntimeDirective::StartAccounts {
            accounts: vec![account.clone()],
        },
        ScheduledAccountAction::Stop => RuntimeDirective::StopAccounts {
            account: Some(account.clone()),
        },
    };
    match session.execute_directive(directive).await {
        Ok(_) => notify_auto_rotate_action(session, account, action, next_delay).await,
        Err(error) => tracing::warn!(
            account = %account,
            action = ?action,
            error = %error,
            "auto-rotate account action failed"
        ),
    }
}

async fn notify_auto_rotate_waiting(session: &RuntimeSession, schedule: &AutoRotateSchedule) {
    let first_start_delay = schedule.action_steps()[0].0;
    notify_operator_best_effort(
        session,
        Notification {
            title: "Waiting".to_string(),
            body: format!(
                "`{}` rests first and will log on in <t:{}:R>.",
                schedule.account,
                unix_timestamp_after(first_start_delay)
            ),
            account: Some(schedule.account.clone()),
        },
    )
    .await;
}

async fn notify_auto_rotate_action(
    session: &RuntimeSession,
    account: &AccountId,
    action: ScheduledAccountAction,
    next_delay: Duration,
) {
    let (title, body) = match action {
        ScheduledAccountAction::Start => (
            "Started flipping",
            format!(
                "Logged in as `{account}`.\nWill log off in <t:{}:R>.",
                unix_timestamp_after(next_delay)
            ),
        ),
        ScheduledAccountAction::Stop => (
            "Killed bot",
            format!(
                "Stopped `{account}`.\nWill log on in <t:{}:R>.",
                unix_timestamp_after(next_delay)
            ),
        ),
    };
    notify_operator_best_effort(
        session,
        Notification {
            title: title.to_string(),
            body,
            account: Some(account.clone()),
        },
    )
    .await;
}

fn unix_timestamp_after(delay: Duration) -> u64 {
    now_ms().saturating_add(delay.as_millis().min(u128::from(u64::MAX)) as u64) / 1_000
}
