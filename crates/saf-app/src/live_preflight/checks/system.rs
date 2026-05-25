use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::config_session::account_env_key;
use crate::live_preflight::{LivePreflightCheck, LivePreflightFeatureFlags, LivePreflightOptions};
use saf_core::AccountId;

use super::common::{fail, feature_check, non_empty_env, parent_exists, pass, warn};

pub(in crate::live_preflight) fn push_path_checks(
    options: &LivePreflightOptions,
    checks: &mut Vec<LivePreflightCheck>,
) {
    let state = &options.state_base_dir;
    if state.exists() && !state.is_dir() {
        checks.push(fail(
            "paths.stateBaseDir",
            format!("{} exists but is not a directory.", state.display()),
        ));
    } else if state.exists() {
        checks.push(pass(
            "paths.stateBaseDir",
            format!("{} exists.", state.display()),
        ));
    } else if parent_exists(state) {
        checks.push(warn(
            "paths.stateBaseDir",
            format!(
                "{} does not exist yet; Rust will create state files under it when needed.",
                state.display()
            ),
        ));
    } else {
        checks.push(fail(
            "paths.stateBaseDir",
            format!("parent directory for {} does not exist.", state.display()),
        ));
    }

    let inbox_parent = options
        .command_inbox
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if inbox_parent.exists() && inbox_parent.is_dir() {
        checks.push(pass(
            "paths.commandInbox",
            format!("command inbox parent {} exists.", inbox_parent.display()),
        ));
    } else {
        checks.push(fail(
            "paths.commandInbox",
            format!(
                "command inbox parent {} does not exist.",
                inbox_parent.display()
            ),
        ));
    }
}

pub(in crate::live_preflight) fn push_feature_checks(
    features: &LivePreflightFeatureFlags,
    require_discord: bool,
    require_cofl: bool,
    require_minecraft: bool,
    require_full: bool,
    checks: &mut Vec<LivePreflightCheck>,
) {
    if require_full && !features.production_runtime {
        checks.push(fail(
            "features.productionRuntime",
            "full live parity requires building saf-app with --features production-runtime.",
        ));
    } else if require_full {
        checks.push(pass(
            "features.productionRuntime",
            "production-runtime feature bundle is enabled.",
        ));
    }

    feature_check(
        checks,
        "features.liveDiscord",
        features.live_discord,
        require_discord,
        "live-discord feature is enabled.",
        "Discord gateway requires --features live-discord or production-runtime.",
    );
    feature_check(
        checks,
        "features.liveCofl",
        features.live_cofl,
        require_cofl,
        "live-cofl feature is enabled.",
        "Cofl websocket runtime requires --features live-cofl or production-runtime.",
    );
    feature_check(
        checks,
        "features.liveMinecraft",
        features.live_minecraft,
        require_minecraft,
        "live-minecraft feature is enabled.",
        "native Minecraft login requires --features live-minecraft or production-runtime.",
    );
}

pub(in crate::live_preflight) fn push_minecraft_checks<F>(
    startup_accounts: &[String],
    required: bool,
    env: &F,
    checks: &mut Vec<LivePreflightCheck>,
) where
    F: Fn(&str) -> Option<String>,
{
    let azalea_enabled =
        env("SAF_RUST_MINECRAFT").is_some_and(|value| value.trim().eq_ignore_ascii_case("azalea"));
    if azalea_enabled {
        checks.push(pass(
            "minecraft.azalea",
            "SAF_RUST_MINECRAFT=azalea is set.",
        ));
    } else if required {
        checks.push(fail(
            "minecraft.azalea",
            "native Minecraft live smoke requires SAF_RUST_MINECRAFT=azalea.",
        ));
    } else {
        checks.push(warn(
            "minecraft.azalea",
            "SAF_RUST_MINECRAFT=azalea is not set; Rust will use recorded Minecraft clients.",
        ));
    }

    if required {
        let accounts = startup_accounts
            .iter()
            .filter_map(|account| AccountId::new(account.clone()))
            .collect::<Vec<_>>();
        if accounts.is_empty() {
            checks.push(fail(
                "minecraft.microsoftAuth",
                "No startup accounts are available for Microsoft auth.",
            ));
            return;
        }

        let missing = accounts
            .iter()
            .filter(|account| {
                let key = account_env_key("SAF_MICROSOFT_CACHE", account);
                let cache_key = env(&key)
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty())
                    .unwrap_or_else(|| account.to_string());
                !microsoft_auth_cache_contains_key(env, &cache_key)
            })
            .map(|account| {
                let key = account_env_key("SAF_MICROSOFT_CACHE", account);
                if non_empty_env(env, &key) {
                    format!("{account}: {key}")
                } else {
                    format!("{account}: default cache key")
                }
            })
            .collect::<Vec<_>>();

        if missing.is_empty() {
            checks.push(pass(
                "minecraft.microsoftAuth",
                format!(
                    "Azalea Microsoft auth cache contains startup account entries: {}.",
                    accounts
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            ));
        } else {
            checks.push(warn(
                "minecraft.microsoftAuth",
                format!(
                    "Microsoft auth may require an interactive/device login before live smoke: {}.",
                    missing.join(", ")
                ),
            ));
        }
    }
}

fn microsoft_auth_cache_contains_key<F>(env: &F, cache_key: &str) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    let now = unix_now_seconds();
    microsoft_auth_cache_paths(env).into_iter().any(|path| {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return false;
        };
        serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .is_some_and(|value| microsoft_auth_cache_value_contains_key(&value, cache_key, now))
    })
}

fn microsoft_auth_cache_value_contains_key(
    value: &serde_json::Value,
    cache_key: &str,
    now: u64,
) -> bool {
    if let Some(entries) = value.as_array() {
        return entries
            .iter()
            .any(|entry| microsoft_auth_cache_entry_matches(entry, cache_key, now));
    }

    value.as_object().is_some_and(|entries| {
        entries.iter().any(|(entry_key, entry)| {
            microsoft_auth_cache_entry_matches(entry, cache_key, now)
                || entry_key == cache_key
                    && microsoft_auth_cache_entry_has_valid_minecraft_token(entry, now)
        })
    })
}

fn microsoft_auth_cache_entry_matches(
    entry: &serde_json::Value,
    cache_key: &str,
    now: u64,
) -> bool {
    entry
        .get("cache_key")
        .or_else(|| entry.get("email"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|key| key == cache_key)
        && microsoft_auth_cache_entry_has_valid_minecraft_token(entry, now)
}

fn microsoft_auth_cache_entry_has_valid_minecraft_token(
    entry: &serde_json::Value,
    now: u64,
) -> bool {
    entry
        .get("mca")
        .and_then(|mca| mca.get("expires_at"))
        .and_then(serde_json::Value::as_u64)
        .is_some_and(|expires_at| expires_at > now)
}

fn unix_now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

fn microsoft_auth_cache_paths<F>(env: &F) -> Vec<PathBuf>
where
    F: Fn(&str) -> Option<String>,
{
    let mut paths = Vec::new();
    if let Some(home) = env("HOME")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        let home = PathBuf::from(home);
        paths.push(
            home.join("Library")
                .join("Application Support")
                .join("minecraft")
                .join("azalea-auth.json"),
        );
        paths.push(home.join(".minecraft").join("azalea-auth.json"));
    }
    if let Some(appdata) = env("APPDATA")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        paths.push(
            PathBuf::from(appdata)
                .join(".minecraft")
                .join("azalea-auth.json"),
        );
    }
    paths
}
