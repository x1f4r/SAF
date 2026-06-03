use super::RunLiveOptions;
use super::notifier::all_flip_notifier;
use super::tracked::LiveTrackedFlipProvider;
use anyhow::Result;
use saf_core::{AccountId, Humanizer, RuntimeSession, SafConfig};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

mod auth;
mod client;
mod connection;
mod live_buy;
mod stream;

#[cfg(test)]
pub(super) use auth::cofl_session_with_env;
#[cfg(all(test, feature = "live-cofl"))]
pub(super) use auth::{account_socket_session_source, normalize_account_cofl_socket_link};
pub(super) use auth::{cofl_session, cofl_session_for_link};
use auth::{cofl_socket_link, default_cofl_socket_link};
pub(super) use client::{CoflFlipSafety, LiveCoflClient};
#[cfg(test)]
pub(super) use connection::{CoflReconnectBackoff, CoflSilentOpenWatchdog, cofl_socket_host};
pub(super) use live_buy::PendingLiveBuy;
pub(super) use stream::LiveCoflStream;
#[cfg(test)]
pub(super) use stream::is_expected_blocked_chat_command;

pub(super) async fn add_cofl_clients(
    session: &mut RuntimeSession,
    accounts: &[AccountId],
    config: &SafConfig,
    options: &RunLiveOptions,
    tracked_flips: Arc<LiveTrackedFlipProvider>,
    pending_live_buys: Arc<Mutex<BTreeMap<AccountId, PendingLiveBuy>>>,
    account_humanizers: &BTreeMap<AccountId, Arc<Humanizer>>,
) -> Result<(usize, Vec<LiveCoflStream>)> {
    if !options.connect_cofl {
        return Ok((0, Vec::new()));
    }

    session
        .set_auction_metadata_provider(Arc::new(saf_cofl::http_client::CoflHttpClient::default()));

    let mut streams = Vec::new();
    let session_id = cofl_session(config);
    let flip_safety = cofl_flip_safety(config);
    for account in accounts {
        let Some(link) = cofl_socket_link(config, account) else {
            continue;
        };
        let account_session_id = cofl_session_for_link(&session_id, &link);
        let default_link =
            default_cofl_socket_link(account, &account_session_id).unwrap_or_else(|| link.clone());
        // Each account drives its own humanizer (own throttle + server-switch
        // budgets) so one busy account cannot starve another.
        let humanizer = account_humanizers.get(account).cloned();
        let client = Arc::new(LiveCoflClient::new_with_extras(
            account.clone(),
            link,
            account_session_id,
            default_link,
            flip_safety,
            humanizer.clone(),
        ));
        session.add_cofl_client(account.clone(), client.clone());
        streams.push(LiveCoflStream {
            account: account.clone(),
            client,
            tracked_flips: tracked_flips.clone(),
            market_actions: options.market_actions,
            allow_execute_chat: allow_cofl_execute_chat(),
            all_flip_notifier: all_flip_notifier(config),
            bed_click_offset: Duration::from_millis(config.waittime),
            bed_spam: config.bed_spam,
            bed_click_delay: Duration::from_millis(config.click_delay.max(1)),
            humanizer: humanizer
                .unwrap_or_else(|| Arc::new(Humanizer::new(config.humanizer.clone()))),
            pending_live_buys: pending_live_buys.clone(),
            notified_auth_links: Arc::new(Mutex::new(BTreeSet::new())),
            pending_auth_links: Arc::new(Mutex::new(BTreeMap::new())),
        });
    }

    tracing::info!(
        count = streams.len(),
        "registered live Cofl websocket stream(s)"
    );
    Ok((streams.len(), streams))
}

fn allow_cofl_execute_chat() -> bool {
    std::env::var("SAF_ALLOW_COFL_EXECUTE_CHAT").is_ok_and(|value| value.trim() == "1")
}

fn cofl_flip_safety(config: &SafConfig) -> Option<CoflFlipSafety> {
    let guard_env = std::env::var("SAF_COFL_SETTINGS_GUARD").ok();
    if cfg!(test) && guard_env.is_none() {
        return None;
    }
    if std::env::var("SAF_COFL_SETTINGS_GUARD").is_ok_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "false" | "off" | "disabled"
        )
    }) {
        return None;
    }

    // Default the safety floor to the operator's normal min-profit thresholds
    // (config `skip.minProfit` / `skip.profitPercentage`, editable from the
    // dashboard Config tab). The env vars remain optional hard overrides.
    let default_min_profit = config_number_text(&config.skip.min_profit, "20m");
    let default_min_profit_percent = config_number_text(&config.skip.profit_percentage, "20");
    let min_profit = cofl_safety_number("SAF_COFL_MIN_PROFIT_FLOOR", &default_min_profit)?;
    let min_profit_percent =
        cofl_safety_number("SAF_COFL_MIN_PROFIT_PERCENT_FLOOR", &default_min_profit_percent)?;
    let safety = CoflFlipSafety::new(min_profit, min_profit_percent);
    if let Some(safety) = safety {
        tracing::info!(
            min_profit = safety.min_profit,
            min_profit_percent = safety.min_profit_percent,
            "enabled local Cofl flip safety floor"
        );
    }
    safety
}

/// Renders a config `Value` (a string like `"10m"` or a bare number) into the
/// text form `cofl_safety_number` parses, falling back when it is null/empty.
fn config_number_text(value: &serde_json::Value, fallback: &str) -> String {
    match value {
        serde_json::Value::String(text) if !text.trim().is_empty() => text.clone(),
        serde_json::Value::Number(number) => number.to_string(),
        _ => fallback.to_string(),
    }
}

fn cofl_safety_number(name: &str, default_value: &str) -> Option<f64> {
    let raw = std::env::var(name).unwrap_or_else(|_| default_value.to_string());
    let parsed = saf_core::numbers::parse_number_input(raw.clone().into());
    if parsed.is_none() {
        tracing::warn!(
            env = name,
            value = %raw,
            "invalid Cofl safety floor; disabling local Cofl flip safety"
        );
    }
    parsed
}
