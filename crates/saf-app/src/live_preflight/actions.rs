use super::{
    LivePreflightCheck, LivePreflightOptions, LivePreflightRequiredAction, LivePreflightStatus,
};

pub(super) fn build_required_actions(
    checks: &[LivePreflightCheck],
    options: &LivePreflightOptions,
    startup_accounts: &[String],
) -> Vec<LivePreflightRequiredAction> {
    let mut actions = Vec::new();

    let account_failures = check_names_with_status(
        checks,
        &[
            "config.accounts",
            "config.startupAccounts",
            "config.defaultAccount",
        ],
        LivePreflightStatus::Fail,
    );
    if !account_failures.is_empty() {
        actions.push(required_action(
            "configure-accounts",
            "config",
            "Configure Minecraft accounts",
            "Set at least one non-empty account and a resolvable startup/default account in config.json5, or narrow the process with the supported account environment overrides.",
            account_failures,
            vec![
                "SAF_ONLY_IGNS=<ign>[,<ign>]".to_string(),
                "SAF_DEFAULT_IGN=<ign>".to_string(),
                "SAF_START_DEFAULT_ONLY=1".to_string(),
            ],
            vec!["./saf.sh rust-live-setup".to_string()],
        ));
    }

    let path_failures = check_names_with_status(
        checks,
        &["paths.stateBaseDir", "paths.commandInbox"],
        LivePreflightStatus::Fail,
    );
    if !path_failures.is_empty() {
        actions.push(required_action(
            "fix-runtime-paths",
            "paths",
            "Create runtime paths",
            "Create the state base directory and command inbox parent directory before starting the Rust live runtime.",
            path_failures,
            Vec::new(),
            vec![
                "mkdir -p <state-base-dir>".to_string(),
                "mkdir -p <command-inbox-parent>".to_string(),
                "./saf.sh rust-live-setup".to_string(),
            ],
        ));
    }

    let feature_failures = check_names_with_status(
        checks,
        &[
            "features.productionRuntime",
            "features.liveDiscord",
            "features.liveCofl",
            "features.liveMinecraft",
        ],
        LivePreflightStatus::Fail,
    );
    if !feature_failures.is_empty() {
        actions.push(required_action(
            "build-production-runtime",
            "features",
            "Build the production runtime feature bundle",
            "Run the full Rust live runtime through the production-runtime feature bundle so Discord, Cofl, and native Minecraft are compiled together.",
            feature_failures,
            Vec::new(),
            vec![
                "./saf.sh rust-production-check".to_string(),
                "./saf.sh rust-live-setup".to_string(),
            ],
        ));
    }

    let discord_failures = check_names_with_status(
        checks,
        &[
            "discord.token",
            "discord.enabled",
            "discord.allowedUsers",
            "liveSmoke.discordEnabled",
            "liveSmoke.discordToken",
            "liveSmoke.discordAllowedUsers",
        ],
        LivePreflightStatus::Fail,
    );
    if !discord_failures.is_empty() {
        actions.push(required_action(
            "configure-discord-gateway",
            "discord",
            "Configure Discord gateway access",
            "Provide the Discord bot token, enable the gateway, and configure at least one numeric allowed Discord user ID for live smoke and production control.",
            discord_failures,
            vec![
                "SAF_DISCORD_BOT_ENABLED=1".to_string(),
                "SAF_DISCORD_TOKEN=<bot-token>".to_string(),
                "SAF_DISCORD_ALLOWED_IDS=<discord-user-id>[,<discord-user-id>]".to_string(),
            ],
            vec!["./saf.sh rust-live-setup".to_string()],
        ));
    }

    let cofl_failures = check_names_with_status(
        checks,
        &["cofl.session", "liveSmoke.coflSession"],
        LivePreflightStatus::Fail,
    );
    if !cofl_failures.is_empty() {
        let mut commands = if startup_accounts.is_empty() {
            vec!["./saf.sh rust-cofl-auth-link <ign>".to_string()]
        } else {
            startup_accounts
                .iter()
                .map(|account| format!("./saf.sh rust-cofl-auth-link {account}"))
                .collect()
        };
        commands.push("./saf.sh rust-live-setup".to_string());
        actions.push(required_action(
            "configure-cofl-auth",
            "cofl",
            "Configure Cofl session or socket auth",
            "Run the Cofl auth-link helper for a configured account, complete the SkyCofl browser login, then provide a global Cofl session, a smoke-test Cofl session, or an account-specific socket URL with an SId query value for the live smoke account.",
            cofl_failures,
            vec![
                "SAF_COFL_SESSION=<cofl-session>".to_string(),
                "SAF_LIVE_SMOKE_COFL_SESSION=<cofl-session>".to_string(),
                "SAF_COFL_SOCKET_<IGN>=wss://.../modsocket?SId=<session>".to_string(),
            ],
            commands,
        ));
    }

    let minecraft_failures =
        check_names_with_status(checks, &["minecraft.azalea"], LivePreflightStatus::Fail);
    if !minecraft_failures.is_empty() {
        actions.push(required_action(
            "enable-native-minecraft",
            "minecraft",
            "Enable native Minecraft runtime",
            "Set the native Minecraft runtime flag so live smoke and Rust production mode use Azalea instead of recorded clients.",
            minecraft_failures,
            vec!["SAF_RUST_MINECRAFT=azalea".to_string()],
            vec![
                "SAF_RUST_MINECRAFT=azalea ./saf.sh rust-live-setup".to_string(),
                "SAF_RUST_MINECRAFT=azalea ./saf.sh rust-minecraft-auth <ign>".to_string(),
            ],
        ));
    }

    let smoke_ign_failures =
        check_names_with_status(checks, &["liveSmoke.ign"], LivePreflightStatus::Fail);
    if !smoke_ign_failures.is_empty() {
        let suggested_ign = startup_accounts
            .first()
            .cloned()
            .unwrap_or_else(|| "<configured-ign>".to_string());
        actions.push(required_action(
            "set-live-smoke-account",
            "smoke",
            "Choose the live smoke account",
            "Set the configured Minecraft account that the env-gated live smoke harness is allowed to use.",
            smoke_ign_failures,
            vec![format!("SAF_LIVE_SMOKE_IGN={suggested_ign}")],
            vec!["./saf.sh rust-live-setup".to_string()],
        ));
    }

    let microsoft_auth_warnings = check_names_with_status(
        checks,
        &["minecraft.microsoftAuth"],
        LivePreflightStatus::Warn,
    );
    if (options.full || options.require_minecraft) && !microsoft_auth_warnings.is_empty() {
        let mut commands = if startup_accounts.is_empty() {
            vec!["SAF_RUST_MINECRAFT=azalea ./saf.sh rust-minecraft-auth <ign>".to_string()]
        } else {
            startup_accounts
                .iter()
                .map(|account| {
                    format!("SAF_RUST_MINECRAFT=azalea ./saf.sh rust-minecraft-auth {account}")
                })
                .collect()
        };
        commands.push("./saf.sh rust-live-setup".to_string());
        actions.push(required_action(
            "warm-microsoft-auth",
            "minecraft",
            "Complete Microsoft auth cache warmup",
            "Run the Azalea Microsoft auth helper for each startup account before live smoke. The helper may print a Microsoft device-login URL and code when the cache is cold.",
            microsoft_auth_warnings,
            vec!["SAF_MICROSOFT_CACHE_<IGN>=<cache-key>".to_string()],
            commands,
        ));
    }

    actions
}

fn check_names_with_status(
    checks: &[LivePreflightCheck],
    names: &[&str],
    status: LivePreflightStatus,
) -> Vec<String> {
    checks
        .iter()
        .filter(|check| check.status == status && names.contains(&check.name.as_str()))
        .map(|check| check.name.clone())
        .collect()
}

fn required_action(
    id: impl Into<String>,
    category: impl Into<String>,
    title: impl Into<String>,
    detail: impl Into<String>,
    checks: Vec<String>,
    env: Vec<String>,
    commands: Vec<String>,
) -> LivePreflightRequiredAction {
    LivePreflightRequiredAction {
        id: id.into(),
        category: category.into(),
        title: title.into(),
        detail: detail.into(),
        checks,
        env,
        commands,
    }
}
