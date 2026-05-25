use saf_core::SafConfig;

use crate::live_preflight::LivePreflightCheck;

use super::common::{fail, pass, warn};

pub(in crate::live_preflight) fn push_live_smoke_account_checks<F>(
    config: &SafConfig,
    full_smoke: bool,
    env: &F,
    checks: &mut Vec<LivePreflightCheck>,
) where
    F: Fn(&str) -> Option<String>,
{
    if !full_smoke {
        return;
    }

    let Some(smoke_ign) = env("SAF_LIVE_SMOKE_IGN")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    else {
        checks.push(fail(
            "liveSmoke.ign",
            "full live smoke requires SAF_LIVE_SMOKE_IGN so the harness does not use the placeholder SmokeAccount.",
        ));
        return;
    };

    if config
        .configured_igns()
        .iter()
        .any(|ign| ign.eq_ignore_ascii_case(&smoke_ign))
    {
        checks.push(pass(
            "liveSmoke.ign",
            format!("SAF_LIVE_SMOKE_IGN is configured for `{smoke_ign}`."),
        ));
    } else {
        checks.push(fail(
            "liveSmoke.ign",
            format!("SAF_LIVE_SMOKE_IGN `{smoke_ign}` is not present in configured igns."),
        ));
    }
}

pub(in crate::live_preflight) fn push_account_checks(
    config: &SafConfig,
    configured_accounts: &[String],
    startup_accounts: &[String],
    checks: &mut Vec<LivePreflightCheck>,
) {
    if configured_accounts.is_empty() {
        checks.push(fail(
            "config.accounts",
            "config.json5 does not define any non-empty Minecraft accounts.",
        ));
    } else {
        checks.push(pass(
            "config.accounts",
            format!("{} configured account(s).", configured_accounts.len()),
        ));
    }

    if startup_accounts.is_empty() {
        checks.push(fail(
            "config.startupAccounts",
            "No startup account resolved from igns/defaultIgn/startDefaultOnly.",
        ));
    } else {
        checks.push(pass(
            "config.startupAccounts",
            format!("startup target(s): {}.", startup_accounts.join(", ")),
        ));
    }

    if let Some(default) = config.default_account() {
        checks.push(pass(
            "config.defaultAccount",
            format!("default account resolves to `{default}`."),
        ));
    } else {
        checks.push(warn(
            "config.defaultAccount",
            "No default account resolved; commands without an explicit account may fail.",
        ));
    }
}
