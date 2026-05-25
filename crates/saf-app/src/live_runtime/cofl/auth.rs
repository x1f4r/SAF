use crate::config_session::account_env_key;
use saf_core::{AccountId, SafConfig};

pub(super) fn cofl_socket_link(config: &SafConfig, account: &AccountId) -> Option<String> {
    let session = cofl_session(config);
    if let Ok(link) = std::env::var(account_env_key("SAF_COFL_SOCKET", account))
        && !link.trim().is_empty()
    {
        let link_session = account_socket_session_source(&link, &session);
        return normalize_account_cofl_socket_link(&link, account, link_session);
    }

    if session.trim().is_empty() {
        return None;
    }
    default_cofl_socket_link(account, &session)
}

pub(in crate::live_runtime) fn normalize_account_cofl_socket_link(
    link: &str,
    account: &AccountId,
    session: &str,
) -> Option<String> {
    let normalized = saf_cofl::build_cofl_socket_link(link, account.as_str(), session)?;
    (!session.trim().is_empty() || saf_cofl::cofl_socket_session_id(&normalized).is_some())
        .then_some(normalized)
}

pub(in crate::live_runtime) fn account_socket_session_source<'a>(
    link: &str,
    configured_session: &'a str,
) -> &'a str {
    if saf_cofl::cofl_socket_session_id(link).is_some() {
        ""
    } else {
        configured_session
    }
}

pub(in crate::live_runtime) fn cofl_session_for_link(
    configured_session: &str,
    link: &str,
) -> String {
    saf_cofl::cofl_socket_session_id(link)
        .or_else(|| normalize_cofl_session_id(configured_session))
        .unwrap_or_default()
}

pub(super) fn default_cofl_socket_link(account: &AccountId, session: &str) -> Option<String> {
    saf_cofl::build_cofl_socket_link("wss://sky.coflnet.com/modsocket", account.as_str(), session)
}

pub(in crate::live_runtime) fn cofl_session(config: &SafConfig) -> String {
    cofl_session_with_env(config, |name| std::env::var(name).ok())
}

pub(in crate::live_runtime) fn cofl_session_with_env<F>(config: &SafConfig, env: F) -> String
where
    F: Fn(&str) -> Option<String>,
{
    env("SAF_COFL_SESSION")
        .and_then(|session| normalize_cofl_session_id(&session))
        .or_else(|| normalize_cofl_session_id(&config.session))
        .unwrap_or_default()
}

fn normalize_cofl_session_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}
