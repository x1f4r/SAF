use saf_core::{AccountEnvironment, SafConfig, config_with_account_environment};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

mod actions;
mod checks;

use actions::build_required_actions;
use checks::{
    push_account_checks, push_cofl_checks, push_discord_checks, push_feature_checks,
    push_live_smoke_account_checks, push_minecraft_checks, push_path_checks,
};

#[derive(Clone, Debug)]
pub struct LivePreflightOptions {
    pub command_inbox: PathBuf,
    pub state_base_dir: PathBuf,
    pub require_discord: bool,
    pub require_cofl: bool,
    pub require_minecraft: bool,
    pub full: bool,
}

impl Default for LivePreflightOptions {
    fn default() -> Self {
        Self {
            command_inbox: PathBuf::from(".saf-commands.jsonl"),
            state_base_dir: PathBuf::from("."),
            require_discord: false,
            require_cofl: false,
            require_minecraft: false,
            full: false,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LivePreflightReport {
    pub ok: bool,
    pub configured_accounts: Vec<String>,
    pub startup_accounts: Vec<String>,
    pub feature_flags: LivePreflightFeatureFlags,
    pub checks: Vec<LivePreflightCheck>,
    pub required_actions: Vec<LivePreflightRequiredAction>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LivePreflightFeatureFlags {
    pub live_cofl: bool,
    pub live_discord: bool,
    pub live_minecraft: bool,
    pub production_runtime: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LivePreflightCheck {
    pub name: String,
    pub status: LivePreflightStatus,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LivePreflightRequiredAction {
    pub id: String,
    pub category: String,
    pub title: String,
    pub detail: String,
    pub checks: Vec<String>,
    pub env: Vec<String>,
    pub commands: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LivePreflightStatus {
    Pass,
    Warn,
    Fail,
}

pub fn run_live_preflight(
    config: &SafConfig,
    options: &LivePreflightOptions,
) -> LivePreflightReport {
    run_live_preflight_with_env(config, options, |name| std::env::var(name).ok())
}

fn run_live_preflight_with_env<F>(
    config: &SafConfig,
    options: &LivePreflightOptions,
    env: F,
) -> LivePreflightReport
where
    F: Fn(&str) -> Option<String>,
{
    let require_discord = options.full || options.require_discord;
    let require_cofl = options.full || options.require_cofl;
    let require_minecraft = options.full || options.require_minecraft;
    let account_env = AccountEnvironment::from_entries(
        [
            "SAF_DEFAULT_IGN",
            "SAF_ONLY_IGNS",
            "SAF_ONLY_IGN",
            "SAF_START_DEFAULT_ONLY",
        ]
        .into_iter()
        .filter_map(|name| env(name).map(|value| (name, value))),
    );
    let config = config_with_account_environment(config, &account_env);
    let configured_accounts = config.configured_igns();
    let startup_accounts = config.startup_igns();
    let feature_flags = LivePreflightFeatureFlags {
        live_cofl: cfg!(feature = "live-cofl"),
        live_discord: cfg!(feature = "live-discord"),
        live_minecraft: cfg!(feature = "live-minecraft"),
        production_runtime: cfg!(feature = "production-runtime"),
    };
    let mut checks = Vec::new();

    push_account_checks(
        &config,
        &configured_accounts,
        &startup_accounts,
        &mut checks,
    );
    push_path_checks(options, &mut checks);
    push_feature_checks(
        &feature_flags,
        require_discord,
        require_cofl,
        require_minecraft,
        options.full,
        &mut checks,
    );
    push_discord_checks(&config, require_discord, options.full, &env, &mut checks);
    push_cofl_checks(
        &config,
        &startup_accounts,
        require_cofl,
        options.full,
        &env,
        &mut checks,
    );
    push_minecraft_checks(&startup_accounts, require_minecraft, &env, &mut checks);
    push_live_smoke_account_checks(&config, options.full, &env, &mut checks);

    let ok = !checks
        .iter()
        .any(|check| check.status == LivePreflightStatus::Fail);
    let required_actions = build_required_actions(&checks, options, &startup_accounts);
    LivePreflightReport {
        ok,
        configured_accounts,
        startup_accounts,
        feature_flags,
        checks,
        required_actions,
    }
}

#[cfg(test)]
mod tests;
