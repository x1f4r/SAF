use anyhow::Result;
use saf_app::config_session::ensure_cofl_session;
use saf_app::live_preflight::{LivePreflightOptions, LivePreflightStatus, run_live_preflight};
use saf_app::live_runtime::{MarketActionMode, RunLiveOptions};
use saf_core::{AccountId, SafConfig};
#[cfg(feature = "discord-webhook")]
use saf_discord::DiscordWebhookIdentity;

pub(crate) fn prepare_cofl_session(config: &mut SafConfig, options: &RunLiveOptions) -> Result<()> {
    if !cfg!(feature = "live-cofl") || !options.connect_cofl {
        return Ok(());
    }
    let accounts = config
        .configured_igns()
        .into_iter()
        .filter_map(AccountId::new)
        .collect::<Vec<_>>();
    if ensure_cofl_session(config, &accounts, &options.config_path, |name| {
        std::env::var(name).ok()
    })?
    .is_some()
    {
        tracing::info!(
            path = %options.config_path.display(),
            "generated missing Cofl session in config"
        );
    }
    Ok(())
}

pub(crate) fn validate_run_live_safety(config: &SafConfig, options: &RunLiveOptions) -> Result<()> {
    if options.market_actions != MarketActionMode::Live {
        return Ok(());
    }

    let report = run_live_preflight(
        config,
        &LivePreflightOptions {
            command_inbox: options.command_inbox.clone(),
            state_base_dir: options.state_base_dir.clone(),
            require_discord: false,
            require_cofl: options.connect_cofl,
            require_minecraft: true,
            full: false,
        },
    );
    if report.ok {
        return Ok(());
    }

    let failed = report
        .checks
        .iter()
        .filter(|check| check.status == LivePreflightStatus::Fail)
        .map(|check| check.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::bail!("--market-actions live failed live preflight checks: {failed}")
}

#[cfg(feature = "discord-webhook")]
pub(crate) fn webhook_identity(config: &SafConfig) -> DiscordWebhookIdentity {
    DiscordWebhookIdentity {
        username: non_empty(config.branding.name.clone()),
        avatar_url: non_empty(config.branding.icon_url.clone()),
    }
}

#[cfg(feature = "discord-webhook")]
fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use saf_app::live_runtime::RunLiveOptions;

    #[test]
    fn dry_run_market_mode_skips_live_action_preflight() {
        let config = SafConfig::default();
        let options = RunLiveOptions::new(".saf-commands.jsonl", ".");

        validate_run_live_safety(&config, &options).unwrap();
    }

    #[test]
    fn live_market_mode_requires_live_action_preflight() {
        let config = SafConfig::default();
        let mut options = RunLiveOptions::new(".saf-commands.jsonl", ".");
        options.market_actions = MarketActionMode::Live;

        let error = validate_run_live_safety(&config, &options).unwrap_err();
        let message = error.to_string();

        assert!(message.contains("config.accounts"));
        assert!(message.contains("minecraft.azalea"));
    }

    #[test]
    fn prepare_cofl_session_skips_disabled_cofl() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("config.json5");
        std::fs::write(&config_path, r#"{ igns: ["Main"], session: "" }"#).unwrap();
        let mut config = SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        };
        let mut options = RunLiveOptions::new(".saf-commands.jsonl", ".");
        options.config_path = config_path.clone();
        options.connect_cofl = false;

        prepare_cofl_session(&mut config, &options).unwrap();

        assert!(config.session.is_empty());
        assert_eq!(
            std::fs::read_to_string(config_path).unwrap(),
            r#"{ igns: ["Main"], session: "" }"#
        );
    }

    #[cfg(feature = "live-cofl")]
    #[test]
    fn prepare_cofl_session_generates_when_cofl_is_enabled() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("config.json5");
        std::fs::write(&config_path, r#"{ igns: ["Main"], session: "" }"#).unwrap();
        let mut config = SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        };
        let mut options = RunLiveOptions::new(".saf-commands.jsonl", ".");
        options.config_path = config_path.clone();
        options.connect_cofl = true;

        prepare_cofl_session(&mut config, &options).unwrap();

        assert!(uuid::Uuid::parse_str(&config.session).is_ok());
        assert!(
            std::fs::read_to_string(config_path)
                .unwrap()
                .contains(&config.session)
        );
    }
}
