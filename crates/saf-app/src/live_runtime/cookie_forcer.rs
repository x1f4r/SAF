use async_trait::async_trait;
use saf_core::AccountId;
use saf_core::ports::{CookieForcer, PortError};
use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

/// Port implementation that records a manual cookie-buy request. It only stages
/// the account; the runtime loop drains the shared set in
/// `drive_forced_cookies_once` and runs the existing booster-cookie machinery,
/// so this never touches the persisted queue (which would risk a stuck entry).
pub(super) struct LiveCookieForcer {
    requests: Arc<Mutex<BTreeSet<AccountId>>>,
    /// Snapshot of `config.use_cookie && config.relist` — the same gate the
    /// auto-cookie flow enforces. When off, surface a clear error instead of
    /// silently no-op'ing.
    cookies_enabled: bool,
    /// Whether `config.auto_cookie` yields a non-zero threshold. The buy path
    /// short-circuits when the threshold is absent, so without this the command
    /// would report success while never buying.
    auto_cookie_enabled: bool,
}

impl LiveCookieForcer {
    pub(super) fn new(
        requests: Arc<Mutex<BTreeSet<AccountId>>>,
        cookies_enabled: bool,
        auto_cookie_enabled: bool,
    ) -> Self {
        Self {
            requests,
            cookies_enabled,
            auto_cookie_enabled,
        }
    }
}

#[async_trait]
impl CookieForcer for LiveCookieForcer {
    async fn force_cookie(&self, account: &AccountId) -> Result<(), PortError> {
        if !self.cookies_enabled {
            return Err(PortError::Failed(
                "Cookie buying is disabled in config (useCookie/relist).".to_string(),
            ));
        }
        if !self.auto_cookie_enabled {
            return Err(PortError::Failed(
                "Auto-cookie threshold is disabled in config (autoCookie); set a non-zero value to buy cookies.".to_string(),
            ));
        }
        self.requests
            .lock()
            .map_err(|_| PortError::Failed("cookie force-request lock poisoned".to_string()))?
            .insert(account.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn forcer(cookies: bool, auto: bool) -> (LiveCookieForcer, Arc<Mutex<BTreeSet<AccountId>>>) {
        let requests = Arc::new(Mutex::new(BTreeSet::new()));
        (LiveCookieForcer::new(requests.clone(), cookies, auto), requests)
    }

    #[tokio::test]
    async fn all_gates_open_stages_the_account() {
        let (f, requests) = forcer(true, true);
        let account = AccountId::new("MainAccount").unwrap();
        f.force_cookie(&account).await.unwrap();
        assert!(requests.lock().unwrap().contains(&account));
    }

    #[tokio::test]
    async fn auto_cookie_disabled_errors_and_stages_nothing() {
        let (f, requests) = forcer(true, false);
        let account = AccountId::new("MainAccount").unwrap();
        assert!(f.force_cookie(&account).await.is_err());
        assert!(requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn cookies_disabled_errors_and_stages_nothing() {
        let (f, requests) = forcer(false, true);
        let account = AccountId::new("MainAccount").unwrap();
        assert!(f.force_cookie(&account).await.is_err());
        assert!(requests.lock().unwrap().is_empty());
    }
}
