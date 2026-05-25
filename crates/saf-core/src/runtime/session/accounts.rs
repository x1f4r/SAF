use super::*;

impl RuntimeSession {
    pub(super) fn runtime_with_running(&self) -> BotRuntime {
        self.runtime.with_running(self.running_accounts())
    }

    pub(super) fn mark_accounts_started(&self, accounts: &[AccountId]) -> Result<(), RuntimeError> {
        let mut running = self
            .running_accounts
            .lock()
            .map_err(|_| RuntimeError::Port("running account lock poisoned".to_string()))?;
        for account in accounts {
            let account = self.canonical_account_name(account);
            if !running
                .iter()
                .any(|running| running.eq_ignore_ascii_case(&account))
            {
                running.push(account);
            }
        }
        self.sort_running_accounts(&mut running);
        Ok(())
    }

    pub(super) fn mark_accounts_stopped(
        &self,
        account: Option<&AccountId>,
    ) -> Result<(), RuntimeError> {
        let mut running = self
            .running_accounts
            .lock()
            .map_err(|_| RuntimeError::Port("running account lock poisoned".to_string()))?;
        if let Some(account) = account {
            running.retain(|running| !running.eq_ignore_ascii_case(account.as_str()));
        } else {
            running.clear();
        }
        Ok(())
    }

    pub(super) fn canonical_account_name(&self, account: &AccountId) -> String {
        self.runtime
            .selector
            .configured
            .iter()
            .find(|configured| configured.eq_ignore_ascii_case(account.as_str()))
            .cloned()
            .unwrap_or_else(|| account.to_string())
    }

    pub(super) fn sort_running_accounts(&self, running: &mut [String]) {
        let configured = &self.runtime.selector.configured;
        running.sort_by_key(|account| {
            configured
                .iter()
                .position(|configured| configured.eq_ignore_ascii_case(account))
                .unwrap_or(usize::MAX)
        });
    }

    pub(super) fn all_configured_accounts(&self) -> Result<Vec<AccountId>, RuntimeError> {
        let selector = self.selector();
        let candidates = if selector.configured.is_empty() {
            selector.running
        } else {
            selector.configured
        };
        let accounts = candidates
            .iter()
            .filter_map(AccountId::new)
            .collect::<Vec<_>>();
        if accounts.is_empty() {
            return Err(RuntimeError::Invalid(
                "No configured accounts are available.".to_string(),
            ));
        }
        Ok(accounts)
    }
}
