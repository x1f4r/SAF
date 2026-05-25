use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::time::{SystemTime, UNIX_EPOCH};

fn env(values: BTreeMap<&'static str, &'static str>) -> impl Fn(&str) -> Option<String> {
    move |name| values.get(name).map(|value| value.to_string())
}

fn env_owned(values: BTreeMap<String, String>) -> impl Fn(&str) -> Option<String> {
    move |name| values.get(name).cloned()
}

fn config() -> SafConfig {
    SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    }
}

fn future_expiry() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600
}

#[test]
fn default_preflight_passes_with_configured_account() {
    let report = run_live_preflight_with_env(
        &config(),
        &LivePreflightOptions::default(),
        env(BTreeMap::new()),
    );

    assert!(report.ok);
    assert_eq!(report.configured_accounts, vec!["Main".to_string()]);
    assert!(report.checks.iter().any(|check| {
        check.name == "discord.token" && check.status == LivePreflightStatus::Warn
    }));
    assert!(report.required_actions.is_empty());
}

#[test]
fn preflight_reports_effective_account_environment_overrides() {
    let config = SafConfig {
        igns: vec!["Main".to_string(), "Alt".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };

    let report = run_live_preflight_with_env(
        &config,
        &LivePreflightOptions::default(),
        env(BTreeMap::from([
            ("SAF_ONLY_IGNS", "Alt"),
            ("SAF_DEFAULT_IGN", "Main"),
            ("SAF_START_DEFAULT_ONLY", "1"),
        ])),
    );

    assert!(report.ok);
    assert_eq!(report.configured_accounts, vec!["Alt".to_string()]);
    assert_eq!(report.startup_accounts, vec!["Alt".to_string()]);
    assert!(report.checks.iter().any(|check| {
        check.name == "config.defaultAccount"
            && check.status == LivePreflightStatus::Pass
            && check.message.contains("Alt")
    }));
}

#[test]
fn full_preflight_reports_missing_live_requirements() {
    let options = LivePreflightOptions {
        full: true,
        ..LivePreflightOptions::default()
    };

    let report = run_live_preflight_with_env(&config(), &options, env(BTreeMap::new()));

    assert!(!report.ok);
    assert!(report.checks.iter().any(|check| {
        check.name == "discord.token" && check.status == LivePreflightStatus::Fail
    }));
    assert!(report.checks.iter().any(|check| {
        check.name == "cofl.session"
            && check.status == LivePreflightStatus::Pass
            && check.message.contains("generate")
    }));
    assert!(report.checks.iter().any(|check| {
        check.name == "minecraft.azalea" && check.status == LivePreflightStatus::Fail
    }));
    assert!(report.checks.iter().any(|check| {
        check.name == "liveSmoke.discordToken" && check.status == LivePreflightStatus::Fail
    }));
    assert!(report.checks.iter().any(|check| {
        check.name == "liveSmoke.coflSession" && check.status == LivePreflightStatus::Fail
    }));
    assert!(report.checks.iter().any(|check| {
        check.name == "liveSmoke.ign" && check.status == LivePreflightStatus::Fail
    }));

    let action_ids = report
        .required_actions
        .iter()
        .map(|action| action.id.as_str())
        .collect::<BTreeSet<_>>();
    assert!(action_ids.contains("configure-discord-gateway"));
    assert!(action_ids.contains("enable-native-minecraft"));
    assert!(action_ids.contains("configure-cofl-auth"));
    assert!(action_ids.contains("set-live-smoke-account"));
    assert!(action_ids.contains("warm-microsoft-auth"));
    if !cfg!(feature = "production-runtime") {
        assert!(action_ids.contains("build-production-runtime"));
    }
    assert!(report.required_actions.iter().any(|action| {
        action
            .commands
            .iter()
            .any(|command| command.contains("rust-minecraft-auth Main"))
    }));
    assert!(report.required_actions.iter().any(|action| {
        action.id == "configure-cofl-auth"
            && action
                .commands
                .iter()
                .any(|command| command == "./saf.sh rust-cofl-auth-link Main")
    }));
}

#[test]
fn microsoft_auth_required_action_lists_each_startup_account() {
    let options = LivePreflightOptions {
        full: true,
        ..LivePreflightOptions::default()
    };
    let config = SafConfig {
        igns: vec!["Main".to_string(), "Alt".to_string()],
        default_ign: "Main".to_string(),
        start_default_only: false,
        ..SafConfig::default()
    };

    let report = run_live_preflight_with_env(&config, &options, env(BTreeMap::new()));

    let action = report
        .required_actions
        .iter()
        .find(|action| action.id == "warm-microsoft-auth")
        .expect("warm auth action should be present");
    assert!(action.commands.iter().any(|command| {
        command == "SAF_RUST_MINECRAFT=azalea ./saf.sh rust-minecraft-auth Main"
    }));
    assert!(action.commands.iter().any(|command| {
        command == "SAF_RUST_MINECRAFT=azalea ./saf.sh rust-minecraft-auth Alt"
    }));
    assert_eq!(
        action
            .commands
            .iter()
            .filter(|command| command.contains("rust-minecraft-auth"))
            .count(),
        2
    );
}

#[test]
fn cofl_auth_required_action_lists_each_startup_account() {
    let options = LivePreflightOptions {
        full: true,
        ..LivePreflightOptions::default()
    };
    let config = SafConfig {
        igns: vec!["Main".to_string(), "Alt".to_string()],
        default_ign: "Main".to_string(),
        start_default_only: false,
        ..SafConfig::default()
    };

    let report = run_live_preflight_with_env(&config, &options, env(BTreeMap::new()));

    let action = report
        .required_actions
        .iter()
        .find(|action| action.id == "configure-cofl-auth")
        .expect("Cofl auth action should be present");
    assert!(
        action
            .commands
            .iter()
            .any(|command| { command == "./saf.sh rust-cofl-auth-link Main" })
    );
    assert!(
        action
            .commands
            .iter()
            .any(|command| { command == "./saf.sh rust-cofl-auth-link Alt" })
    );
    assert_eq!(
        action
            .commands
            .iter()
            .filter(|command| command.contains("rust-cofl-auth-link"))
            .count(),
        2
    );
    assert!(action.detail.contains("auth-link helper"));
}

#[test]
fn full_preflight_requires_env_gated_smoke_values() {
    let options = LivePreflightOptions {
        full: true,
        ..LivePreflightOptions::default()
    };
    let mut config = config();
    config.session = "config-cofl-session".to_string();
    config.discord_bot.enabled = true;
    config.discord_bot.token = "config-discord-token".to_string();
    config.discord_bot.allowed_i_ds = vec!["123".to_string()];

    let report = run_live_preflight_with_env(
        &config,
        &options,
        env(BTreeMap::from([("SAF_RUST_MINECRAFT", " Azalea ")])),
    );

    assert!(!report.ok);
    for name in [
        "liveSmoke.discordEnabled",
        "liveSmoke.discordToken",
        "liveSmoke.discordAllowedUsers",
        "liveSmoke.coflSession",
        "liveSmoke.ign",
    ] {
        assert!(
            report
                .checks
                .iter()
                .any(|check| check.name == name && check.status == LivePreflightStatus::Fail),
            "{name} should fail when the full-smoke env value is missing"
        );
    }
}

#[test]
fn full_preflight_requires_numeric_env_discord_allowed_ids() {
    let options = LivePreflightOptions {
        full: true,
        ..LivePreflightOptions::default()
    };
    let mut config = config();
    config.session = "config-cofl-session".to_string();
    config.discord_bot.enabled = true;
    config.discord_bot.token = "config-discord-token".to_string();
    config.discord_bot.allowed_i_ds = vec!["123".to_string()];

    let report = run_live_preflight_with_env(
        &config,
        &options,
        env(BTreeMap::from([
            ("SAF_RUST_MINECRAFT", "azalea"),
            ("SAF_DISCORD_BOT_ENABLED", "1"),
            ("SAF_DISCORD_TOKEN", "redacted-discord-token"),
            ("SAF_DISCORD_ALLOWED_IDS", "not-a-user"),
            ("SAF_LIVE_SMOKE_COFL_SESSION", "redacted-cofl-session"),
            ("SAF_LIVE_SMOKE_IGN", "Main"),
        ])),
    );

    assert!(!report.ok);
    assert!(report.checks.iter().any(|check| {
        check.name == "discord.allowedUsers" && check.status == LivePreflightStatus::Pass
    }));
    assert!(report.checks.iter().any(|check| {
        check.name == "liveSmoke.discordAllowedUsers"
            && check.status == LivePreflightStatus::Fail
            && check.message.contains("numeric Discord user ID")
    }));

    let report = run_live_preflight_with_env(
        &config,
        &options,
        env(BTreeMap::from([
            ("SAF_RUST_MINECRAFT", "azalea"),
            ("SAF_DISCORD_BOT_ENABLED", "1"),
            ("SAF_DISCORD_TOKEN", "redacted-discord-token"),
            ("SAF_DISCORD_ALLOWED_IDS", "bad, 00123, 123"),
            ("SAF_LIVE_SMOKE_COFL_SESSION", "redacted-cofl-session"),
            ("SAF_LIVE_SMOKE_IGN", "Main"),
        ])),
    );

    assert!(report.checks.iter().any(|check| {
        check.name == "liveSmoke.discordAllowedUsers"
            && check.status == LivePreflightStatus::Pass
            && check.message.contains("1 numeric")
    }));
}

#[test]
fn full_preflight_accepts_configured_smoke_ign() {
    let options = LivePreflightOptions {
        full: true,
        ..LivePreflightOptions::default()
    };
    let mut config = config();
    config.session = "config-cofl-session".to_string();
    config.discord_bot.enabled = true;
    config.discord_bot.token = "config-discord-token".to_string();
    config.discord_bot.allowed_i_ds = vec!["123".to_string()];

    let report = run_live_preflight_with_env(
        &config,
        &options,
        env(BTreeMap::from([
            ("SAF_RUST_MINECRAFT", "azalea"),
            ("SAF_DISCORD_BOT_ENABLED", " true "),
            ("SAF_DISCORD_TOKEN", "redacted-discord-token"),
            ("SAF_DISCORD_ALLOWED_IDS", "123"),
            ("SAF_LIVE_SMOKE_COFL_SESSION", "redacted-cofl-session"),
            ("SAF_LIVE_SMOKE_IGN", "Main"),
        ])),
    );

    assert!(report.checks.iter().any(|check| {
        check.name == "liveSmoke.ign" && check.status == LivePreflightStatus::Pass
    }));
    assert!(
        !serde_json::to_string(&report)
            .unwrap()
            .contains("redacted-discord-token")
    );
    assert!(
        !serde_json::to_string(&report)
            .unwrap()
            .contains("redacted-cofl-session")
    );
}

#[test]
fn full_preflight_passes_microsoft_auth_when_cache_contains_startup_account() {
    let temp = tempfile::tempdir().unwrap();
    let cache_path = temp
        .path()
        .join("Library")
        .join("Application Support")
        .join("minecraft");
    std::fs::create_dir_all(&cache_path).unwrap();
    std::fs::write(
        cache_path.join("azalea-auth.json"),
        serde_json::to_string(&serde_json::json!([
            {
                "cache_key": "Main",
                "mca": {
                    "expires_at": future_expiry()
                },
                "profile": {
                    "name": "Main",
                    "id": "00000000-0000-0000-0000-000000000000"
                }
            }
        ]))
        .unwrap(),
    )
    .unwrap();
    let options = LivePreflightOptions {
        full: true,
        ..LivePreflightOptions::default()
    };
    let report = run_live_preflight_with_env(
        &config(),
        &options,
        env_owned(BTreeMap::from([
            (
                "HOME".to_string(),
                temp.path().to_string_lossy().to_string(),
            ),
            ("SAF_RUST_MINECRAFT".to_string(), "azalea".to_string()),
        ])),
    );

    assert!(report.checks.iter().any(|check| {
        check.name == "minecraft.microsoftAuth"
            && check.status == LivePreflightStatus::Pass
            && check.message.contains("Main")
    }));
    assert!(
        !serde_json::to_string(&report)
            .unwrap()
            .contains("azalea-auth")
    );
}

#[test]
fn full_preflight_accepts_object_keyed_microsoft_auth_cache_entry() {
    let temp = tempfile::tempdir().unwrap();
    let cache_path = temp.path().join(".minecraft");
    std::fs::create_dir_all(&cache_path).unwrap();
    std::fs::write(
        cache_path.join("azalea-auth.json"),
        serde_json::to_string(&serde_json::json!({
            "Main": {
                "mca": {
                    "expires_at": future_expiry()
                },
                "profile": {
                    "name": "Main",
                    "id": "00000000-0000-0000-0000-000000000000"
                }
            }
        }))
        .unwrap(),
    )
    .unwrap();
    let options = LivePreflightOptions {
        full: true,
        ..LivePreflightOptions::default()
    };
    let report = run_live_preflight_with_env(
        &config(),
        &options,
        env_owned(BTreeMap::from([
            (
                "HOME".to_string(),
                temp.path().to_string_lossy().to_string(),
            ),
            ("SAF_RUST_MINECRAFT".to_string(), "azalea".to_string()),
        ])),
    );

    assert!(report.checks.iter().any(|check| {
        check.name == "minecraft.microsoftAuth"
            && check.status == LivePreflightStatus::Pass
            && check.message.contains("Main")
    }));
}

#[test]
fn full_preflight_warns_when_microsoft_auth_cache_entry_is_expired() {
    let temp = tempfile::tempdir().unwrap();
    let cache_path = temp
        .path()
        .join("Library")
        .join("Application Support")
        .join("minecraft");
    std::fs::create_dir_all(&cache_path).unwrap();
    std::fs::write(
        cache_path.join("azalea-auth.json"),
        serde_json::to_string(&serde_json::json!([
            {
                "cache_key": "Main",
                "mca": {
                    "expires_at": 1
                },
                "profile": {
                    "name": "Main",
                    "id": "00000000-0000-0000-0000-000000000000"
                }
            }
        ]))
        .unwrap(),
    )
    .unwrap();
    let options = LivePreflightOptions {
        full: true,
        ..LivePreflightOptions::default()
    };
    let report = run_live_preflight_with_env(
        &config(),
        &options,
        env_owned(BTreeMap::from([
            (
                "HOME".to_string(),
                temp.path().to_string_lossy().to_string(),
            ),
            ("SAF_RUST_MINECRAFT".to_string(), "azalea".to_string()),
        ])),
    );

    assert!(report.checks.iter().any(|check| {
        check.name == "minecraft.microsoftAuth"
            && check.status == LivePreflightStatus::Warn
            && check.message.contains("Main")
    }));
}

#[test]
fn preflight_redacts_secret_values() {
    let options = LivePreflightOptions {
        require_discord: true,
        require_cofl: true,
        ..LivePreflightOptions::default()
    };
    let report = run_live_preflight_with_env(
        &config(),
        &options,
        env(BTreeMap::from([
            ("SAF_DISCORD_BOT_ENABLED", "1"),
            ("SAF_DISCORD_TOKEN", "redacted-discord-token"),
            ("SAF_DISCORD_ALLOWED_IDS", "123"),
            ("SAF_COFL_SESSION", "redacted-cofl-session"),
        ])),
    );

    let rendered = serde_json::to_string(&report).unwrap();
    assert!(!rendered.contains("redacted-discord-token"));
    assert!(!rendered.contains("redacted-cofl-session"));
    assert!(rendered.contains("without exposing its value"));
    assert!(!rendered.contains("SAF_DISCORD_TOKEN=redacted-discord-token"));
    assert!(!rendered.contains("SAF_COFL_SESSION=redacted-cofl-session"));
}

#[test]
fn required_actions_use_placeholders_not_secret_values() {
    let options = LivePreflightOptions {
        full: true,
        ..LivePreflightOptions::default()
    };
    let report = run_live_preflight_with_env(
        &config(),
        &options,
        env(BTreeMap::from([
            ("SAF_RUST_MINECRAFT", "azalea"),
            ("SAF_DISCORD_BOT_ENABLED", "1"),
            ("SAF_DISCORD_TOKEN", "redacted-discord-token"),
            ("SAF_DISCORD_ALLOWED_IDS", "bad"),
            ("SAF_LIVE_SMOKE_IGN", "Main"),
        ])),
    );

    let rendered = serde_json::to_string(&report).unwrap();
    assert!(!rendered.contains("redacted-discord-token"));
    assert!(rendered.contains("SAF_DISCORD_TOKEN=<bot-token>"));
    assert!(rendered.contains("SAF_LIVE_SMOKE_COFL_SESSION=<cofl-session>"));
}

#[test]
fn discord_preflight_counts_only_numeric_allowed_user_ids() {
    let options = LivePreflightOptions::default();
    let mut config = config();
    config.discord_bot.enabled = true;
    config.discord_bot.token = "config-discord-token".to_string();
    config.allowed_i_ds = vec!["not-a-user".to_string(), " 00123 ".to_string()];
    config.discord_bot.allowed_i_ds = vec!["also-bad".to_string(), "123".to_string()];

    let report = run_live_preflight_with_env(
        &config,
        &options,
        env(BTreeMap::from([(
            "SAF_DISCORD_ALLOWED_IDS",
            "bad-env, 456, 00123",
        )])),
    );

    assert!(report.ok);
    assert!(report.checks.iter().any(|check| {
        check.name == "discord.allowedUsers"
            && check.status == LivePreflightStatus::Pass
            && check.message.contains("2 allowed")
    }));

    config.allowed_i_ds = vec!["not-a-user".to_string()];
    config.discord_bot.allowed_i_ds = vec!["also-bad".to_string()];
    let report = run_live_preflight_with_env(&config, &options, env(BTreeMap::new()));

    assert!(report.ok);
    assert!(report.checks.iter().any(|check| {
        check.name == "discord.allowedUsers" && check.status == LivePreflightStatus::Warn
    }));
}

#[test]
fn preflight_accepts_account_socket_with_embedded_cofl_session() {
    let options = LivePreflightOptions {
        require_cofl: true,
        ..LivePreflightOptions::default()
    };
    let report = run_live_preflight_with_env(
        &config(),
        &options,
        env(BTreeMap::from([(
            "SAF_COFL_SOCKET_MAIN",
            "wss://sky-us.coflnet.com/modsocket?SId=fake",
        )])),
    );

    assert!(report.checks.iter().any(|check| {
        check.name == "cofl.session" && check.status == LivePreflightStatus::Pass
    }));
    assert!(!serde_json::to_string(&report).unwrap().contains("fake"));

    let report = run_live_preflight_with_env(
        &config(),
        &options,
        env(BTreeMap::from([(
            "SAF_COFL_SOCKET_MAIN",
            "wss://sky-us.coflnet.com/modsocket?region=us",
        )])),
    );

    assert!(report.checks.iter().any(|check| {
        check.name == "cofl.session"
            && check.status == LivePreflightStatus::Pass
            && check.message.contains("generate")
    }));
}

#[test]
fn preflight_accepts_generated_session_for_mixed_socket_coverage() {
    let options = LivePreflightOptions {
        require_cofl: true,
        ..LivePreflightOptions::default()
    };
    let mut config = config();
    config.igns = vec!["Main".to_string(), "Alt".to_string()];
    config.default_ign = "Main".to_string();

    let report = run_live_preflight_with_env(
        &config,
        &options,
        env(BTreeMap::from([(
            "SAF_COFL_SOCKET_MAIN",
            "wss://sky-us.coflnet.com/modsocket?SId=fake",
        )])),
    );

    assert!(report.checks.iter().any(|check| {
        check.name == "cofl.session"
            && check.status == LivePreflightStatus::Pass
            && check.message.contains("generate")
    }));
}

#[test]
fn full_preflight_accepts_socket_auth_for_cofl_smoke() {
    let options = LivePreflightOptions {
        full: true,
        ..LivePreflightOptions::default()
    };
    let mut config = config();
    config.discord_bot.enabled = true;
    config.discord_bot.token = "config-discord-token".to_string();
    config.discord_bot.allowed_i_ds = vec!["123".to_string()];

    let report = run_live_preflight_with_env(
        &config,
        &options,
        env(BTreeMap::from([
            ("SAF_RUST_MINECRAFT", "azalea"),
            ("SAF_DISCORD_BOT_ENABLED", "1"),
            ("SAF_DISCORD_TOKEN", "redacted-discord-token"),
            ("SAF_DISCORD_ALLOWED_IDS", "123"),
            ("SAF_LIVE_SMOKE_IGN", "Main"),
            (
                "SAF_COFL_SOCKET_MAIN",
                "wss://sky-us.coflnet.com/modsocket?SId=fake",
            ),
        ])),
    );

    assert!(report.checks.iter().any(|check| {
        check.name == "cofl.session" && check.status == LivePreflightStatus::Pass
    }));
    assert!(report.checks.iter().any(|check| {
        check.name == "liveSmoke.coflSession" && check.status == LivePreflightStatus::Pass
    }));
    assert!(!serde_json::to_string(&report).unwrap().contains("fake"));
}

#[test]
fn full_preflight_trims_smoke_ign_before_socket_auth_lookup() {
    let options = LivePreflightOptions {
        full: true,
        ..LivePreflightOptions::default()
    };
    let mut config = config();
    config.discord_bot.enabled = true;
    config.discord_bot.token = "config-discord-token".to_string();
    config.discord_bot.allowed_i_ds = vec!["123".to_string()];

    let report = run_live_preflight_with_env(
        &config,
        &options,
        env(BTreeMap::from([
            ("SAF_RUST_MINECRAFT", "azalea"),
            ("SAF_DISCORD_BOT_ENABLED", "1"),
            ("SAF_DISCORD_TOKEN", "redacted-discord-token"),
            ("SAF_DISCORD_ALLOWED_IDS", "123"),
            ("SAF_LIVE_SMOKE_IGN", " Main "),
            (
                "SAF_COFL_SOCKET_MAIN",
                "wss://sky-us.coflnet.com/modsocket?SId=fake",
            ),
        ])),
    );

    assert!(report.checks.iter().any(|check| {
        check.name == "liveSmoke.ign" && check.status == LivePreflightStatus::Pass
    }));
    assert!(report.checks.iter().any(|check| {
        check.name == "liveSmoke.coflSession" && check.status == LivePreflightStatus::Pass
    }));
}
