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
}

impl LiveAccountScheduler {
    pub(in crate::live_runtime) fn new(
        session: Weak<RuntimeSession>,
        shutdown: Arc<AtomicBool>,
    ) -> Self {
        Self { session, shutdown }
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
        tokio::spawn(async move {
            sleep(Duration::from_millis(scheduled.delay_ms)).await;
            if shutdown.load(Ordering::SeqCst) {
                tracing::debug!(
                    account = %scheduled.account,
                    action = ?scheduled.action,
                    "skipping scheduled account action after runtime shutdown"
                );
                return;
            }
            let directive = match scheduled.action {
                ScheduledAccountAction::Start => RuntimeDirective::StartAccounts {
                    accounts: vec![scheduled.account.clone()],
                },
                ScheduledAccountAction::Stop => RuntimeDirective::StopAccounts {
                    account: Some(scheduled.account.clone()),
                },
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
