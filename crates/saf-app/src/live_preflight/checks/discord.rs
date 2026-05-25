use std::collections::BTreeSet;

use crate::live_preflight::LivePreflightCheck;
use saf_core::SafConfig;

use super::common::{fail, non_empty_env, pass, truthy_env, warn};

pub(in crate::live_preflight) fn push_discord_checks<F>(
    config: &SafConfig,
    required: bool,
    full_smoke: bool,
    env: &F,
    checks: &mut Vec<LivePreflightCheck>,
) where
    F: Fn(&str) -> Option<String>,
{
    let token_configured =
        non_empty_env(env, "SAF_DISCORD_TOKEN") || !config.discord_bot.token.trim().is_empty();
    let enabled = truthy_env(env, "SAF_DISCORD_BOT_ENABLED")
        || config.discord_bot.enabled
        || token_configured;
    let allowed_ids = allowed_discord_ids(config, env);

    if token_configured {
        checks.push(pass(
            "discord.token",
            "Discord token is configured without exposing its value.",
        ));
    } else if required {
        checks.push(fail(
            "discord.token",
            "Discord gateway requires SAF_DISCORD_TOKEN or discordBot.token.",
        ));
    } else {
        checks.push(warn(
            "discord.token",
            "Discord token is not configured; gateway stays disabled.",
        ));
    }

    if enabled {
        checks.push(pass("discord.enabled", "Discord gateway is enabled."));
    } else if required {
        checks.push(fail(
            "discord.enabled",
            "Discord gateway is required but not enabled; set SAF_DISCORD_BOT_ENABLED=1.",
        ));
    } else {
        checks.push(warn(
            "discord.enabled",
            "Discord gateway is disabled for this run.",
        ));
    }

    if allowed_ids.is_empty() {
        let message = "No Discord allowed-user IDs configured; the gateway will deny every Discord user who can invoke it.";
        if required {
            checks.push(fail("discord.allowedUsers", message));
        } else {
            checks.push(warn("discord.allowedUsers", message));
        }
    } else {
        checks.push(pass(
            "discord.allowedUsers",
            format!(
                "{} allowed Discord user ID(s) configured.",
                allowed_ids.len()
            ),
        ));
    }

    if full_smoke {
        if truthy_env(env, "SAF_DISCORD_BOT_ENABLED") {
            checks.push(pass(
                "liveSmoke.discordEnabled",
                "SAF_DISCORD_BOT_ENABLED=1 is set for env-gated smoke tests.",
            ));
        } else {
            checks.push(fail(
                "liveSmoke.discordEnabled",
                "full live smoke requires SAF_DISCORD_BOT_ENABLED=1.",
            ));
        }

        if non_empty_env(env, "SAF_DISCORD_TOKEN") {
            checks.push(pass(
                "liveSmoke.discordToken",
                "SAF_DISCORD_TOKEN is set for env-gated smoke tests.",
            ));
        } else {
            checks.push(fail(
                "liveSmoke.discordToken",
                "full live smoke reads SAF_DISCORD_TOKEN from the environment.",
            ));
        }

        let smoke_allowed_ids = env_discord_allowed_ids(env);
        if smoke_allowed_ids.is_empty() {
            checks.push(fail(
                "liveSmoke.discordAllowedUsers",
                "full live smoke requires SAF_DISCORD_ALLOWED_IDS with at least one numeric Discord user ID.",
            ));
        } else {
            checks.push(pass(
                "liveSmoke.discordAllowedUsers",
                format!(
                    "{} numeric Discord user ID(s) configured for env-gated smoke tests.",
                    smoke_allowed_ids.len()
                ),
            ));
        }
    }
}

fn allowed_discord_ids<F>(config: &SafConfig, env: &F) -> Vec<String>
where
    F: Fn(&str) -> Option<String>,
{
    let mut ids = Vec::new();
    ids.extend(config.allowed_i_ds.iter().cloned());
    ids.extend(config.discord_bot.allowed_i_ds.iter().cloned());
    ids.push(config.discord_id.clone());
    ids.extend(env_discord_allowed_ids(env));
    ids.into_iter()
        .filter_map(|id| normalize_discord_user_id(&id))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn env_discord_allowed_ids<F>(env: &F) -> Vec<String>
where
    F: Fn(&str) -> Option<String>,
{
    env("SAF_DISCORD_ALLOWED_IDS")
        .into_iter()
        .flat_map(|raw| {
            raw.split(',')
                .filter_map(normalize_discord_user_id)
                .collect::<Vec<_>>()
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn normalize_discord_user_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<u64>().ok().map(|id| id.to_string())
}
