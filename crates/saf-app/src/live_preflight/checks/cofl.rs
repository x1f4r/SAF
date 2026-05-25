use crate::config_session::{account_env_key, should_generate_cofl_session};
use crate::live_preflight::LivePreflightCheck;
use saf_core::{AccountId, SafConfig};

use super::common::{fail, non_empty_env, pass, warn};

pub(in crate::live_preflight) fn push_cofl_checks<F>(
    config: &SafConfig,
    startup_accounts: &[String],
    required: bool,
    full_smoke: bool,
    env: &F,
    checks: &mut Vec<LivePreflightCheck>,
) where
    F: Fn(&str) -> Option<String>,
{
    let startup_account_ids = startup_accounts
        .iter()
        .filter_map(|account| AccountId::new(account.clone()))
        .collect::<Vec<_>>();
    let has_session = !config.session.trim().is_empty() || non_empty_env(env, "SAF_COFL_SESSION");
    let can_generate_session = should_generate_cofl_session(config, &startup_account_ids, env);
    let mut socket_accounts = 0usize;
    let mut socket_without_session = 0usize;
    let mut accounts_without_session = 0usize;
    for account in startup_account_ids {
        let key = account_env_key("SAF_COFL_SOCKET", &account);
        let Some(link) = env(&key)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if has_session || saf_cofl::cofl_socket_session_id(&link).is_some() {
            socket_accounts += 1;
        } else {
            socket_without_session += 1;
        }
    }
    if !has_session {
        accounts_without_session = startup_accounts
            .len()
            .saturating_sub(socket_accounts + socket_without_session);
    }

    if has_session {
        checks.push(pass(
            "cofl.session",
            "Cofl session is configured without exposing its value.",
        ));
    } else if can_generate_session {
        checks.push(pass(
            "cofl.session",
            "Cofl session is not configured yet; run-live will generate and persist one before connecting Cofl.",
        ));
    } else if socket_accounts == startup_accounts.len() && !startup_accounts.is_empty() {
        checks.push(pass(
            "cofl.session",
            format!("{socket_accounts} account-specific Cofl socket link(s) configured."),
        ));
    } else if socket_without_session > 0 || accounts_without_session > 0 {
        let mut parts = Vec::new();
        if socket_without_session > 0 {
            parts.push(format!(
                "{socket_without_session} account-specific Cofl socket link(s) are set without an SId query parameter"
            ));
        }
        if accounts_without_session > 0 {
            parts.push(format!(
                "{accounts_without_session} startup account(s) have no Cofl session/socket auth"
            ));
        }
        let message = format!(
            "{}; configure config.session, SAF_COFL_SESSION, or SAF_COFL_SOCKET_<IGN> with SId.",
            parts.join("; ")
        );
        if required {
            checks.push(fail("cofl.session", message));
        } else {
            checks.push(warn("cofl.session", message));
        }
    } else if required {
        checks.push(fail(
            "cofl.session",
            "Cofl live runtime requires config.session, SAF_COFL_SESSION, or SAF_COFL_SOCKET_<IGN>.",
        ));
    } else {
        checks.push(warn(
            "cofl.session",
            "No Cofl session/socket configured; run-live can still start with --disable-cofl.",
        ));
    }

    if full_smoke {
        if live_smoke_cofl_auth_configured(env) {
            checks.push(pass(
                "liveSmoke.coflSession",
                "Cofl session/socket auth is set for env-gated smoke tests.",
            ));
        } else {
            checks.push(fail(
                "liveSmoke.coflSession",
                "full live smoke requires SAF_LIVE_SMOKE_COFL_SESSION, SAF_COFL_SESSION, or SAF_COFL_SOCKET_<SAF_LIVE_SMOKE_IGN> with SId.",
            ));
        }
    }
}

fn live_smoke_cofl_auth_configured<F>(env: &F) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    if non_empty_env(env, "SAF_LIVE_SMOKE_COFL_SESSION") || non_empty_env(env, "SAF_COFL_SESSION") {
        return true;
    }
    let Some(account) = env("SAF_LIVE_SMOKE_IGN")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .and_then(AccountId::new)
        .map(|account| account_env_key("SAF_COFL_SOCKET", &account))
    else {
        return false;
    };
    env(&account)
        .map(|link| saf_cofl::cofl_socket_session_id(&link).is_some())
        .unwrap_or(false)
}
