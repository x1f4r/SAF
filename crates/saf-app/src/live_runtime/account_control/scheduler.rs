use async_trait::async_trait;
use saf_core::ports::{
    AccountScheduleRequest, AccountScheduleResult, AccountScheduler, PortError,
    ScheduledAccountAction,
};
use saf_core::{RuntimeDirective, RuntimeSession};
use std::sync::{
    Arc, Weak,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use tokio::time::sleep;

pub(in crate::live_runtime) struct LiveAccountScheduler {
    session: Weak<RuntimeSession>,
    shutdown: Arc<AtomicBool>,
    halted: Arc<AtomicBool>,
}

impl LiveAccountScheduler {
    pub(in crate::live_runtime) fn new(
        session: Weak<RuntimeSession>,
        shutdown: Arc<AtomicBool>,
        halted: Arc<AtomicBool>,
    ) -> Self {
        Self {
            session,
            shutdown,
            halted,
        }
    }
}

#[async_trait]
impl AccountScheduler for LiveAccountScheduler {
    async fn schedule(
        &self,
        request: AccountScheduleRequest,
    ) -> Result<AccountScheduleResult, PortError> {
        let session = self.session.upgrade().ok_or_else(|| {
            PortError::Unavailable("live runtime session is not available".to_string())
        })?;
        let scheduled = request.clone();
        let shutdown = self.shutdown.clone();
        let halted = self.halted.clone();
        tokio::spawn(async move {
            sleep(Duration::from_millis(scheduled.delay_ms)).await;
            let Some(directive) = scheduled_directive(
                &scheduled,
                shutdown.load(Ordering::SeqCst),
                halted.load(Ordering::SeqCst),
            ) else {
                return;
            };
            if let Err(error) = session.execute_directive(directive).await {
                tracing::warn!(
                    account = %scheduled.account,
                    action = ?scheduled.action,
                    error = %error,
                    "scheduled account action failed"
                );
            }
        });
        Ok(AccountScheduleResult {
            account: request.account,
            action: request.action,
            delay_ms: request.delay_ms,
        })
    }
}

/// Decides, once a scheduled delay has elapsed, whether the action should still
/// run and which directive to dispatch. Returns `None` when the runtime has shut
/// down or is halted (Stop All), so neither the live task nor tests need to race
/// the wall clock to observe the skip behavior.
pub(in crate::live_runtime) fn scheduled_directive(
    scheduled: &AccountScheduleRequest,
    shutdown: bool,
    halted: bool,
) -> Option<RuntimeDirective> {
    if shutdown {
        tracing::debug!(
            account = %scheduled.account,
            action = ?scheduled.action,
            "skipping scheduled account action after runtime shutdown"
        );
        return None;
    }
    if halted {
        tracing::warn!(
            account = %scheduled.account,
            action = ?scheduled.action,
            "skipping scheduled account action while runtime is halted (Stop All)"
        );
        return None;
    }
    Some(match scheduled.action {
        ScheduledAccountAction::Start => RuntimeDirective::StartAccounts {
            accounts: vec![scheduled.account.clone()],
        },
        ScheduledAccountAction::Stop => RuntimeDirective::StopAccounts {
            account: Some(scheduled.account.clone()),
        },
    })
}
