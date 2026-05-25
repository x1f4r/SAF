use saf_app::live_runtime::{LiveRuntime, MarketActionMode, RunLiveOptions, run_live};
use saf_app::state_cli::FileQueueStore;
use saf_core::gui::{WindowSlot, WindowSnapshot};
use saf_core::ports::QueueStore;
use saf_core::{
    AccountEnvironment, AccountId, BotState, RuntimeOutcome, SafConfig,
    config_with_account_environment,
};
use serde_json::json;
#[cfg(feature = "live-discord")]
use std::collections::BTreeSet;
use std::path::PathBuf;
use std::time::Duration;

fn smoke_enabled() -> bool {
    env_truthy("SAF_LIVE_SMOKE")
}

fn smoke_probe_enabled(name: &str) -> bool {
    smoke_enabled() && env_truthy(name)
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name).is_ok_and(|value| {
        let value = value.trim();
        value == "1" || value.eq_ignore_ascii_case("true")
    })
}

fn smoke_ign() -> String {
    std::env::var("SAF_LIVE_SMOKE_IGN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "SmokeAccount".to_string())
}

fn configured_smoke_required() -> bool {
    std::env::var("SAF_LIVE_SMOKE_CONFIG")
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
        || smoke_probe_enabled("SAF_LIVE_SMOKE_COFL")
        || smoke_probe_enabled("SAF_LIVE_SMOKE_DISCORD_GATEWAY")
        || smoke_probe_enabled("SAF_LIVE_SMOKE_LOGIN")
        || smoke_probe_enabled("SAF_LIVE_SMOKE_INVENTORY")
        || smoke_probe_enabled("SAF_LIVE_SMOKE_FULL")
        || std::env::var("SAF_RUST_MINECRAFT")
            .is_ok_and(|value| value.trim().eq_ignore_ascii_case("azalea"))
}

#[cfg(any(feature = "live-cofl", feature = "live-minecraft"))]
fn require_non_empty_env(name: &str, probe: &str) {
    assert!(
        !std::env::var(name).unwrap_or_default().trim().is_empty(),
        "{probe} requires {name}"
    );
}

#[cfg(feature = "live-cofl")]
fn require_cofl_smoke_auth(probe: &str) {
    assert!(
        cofl_smoke_auth_configured(),
        "{probe} requires SAF_LIVE_SMOKE_COFL_SESSION, SAF_COFL_SESSION, or SAF_COFL_SOCKET_<SAF_LIVE_SMOKE_IGN> with SId"
    );
}

#[cfg(feature = "live-cofl")]
fn cofl_smoke_auth_configured() -> bool {
    if ["SAF_LIVE_SMOKE_COFL_SESSION", "SAF_COFL_SESSION"]
        .into_iter()
        .any(|name| !std::env::var(name).unwrap_or_default().trim().is_empty())
    {
        return true;
    }
    let Some(account) = AccountId::new(smoke_ign()) else {
        return false;
    };
    let key = smoke_account_env_key("SAF_COFL_SOCKET", &account);
    std::env::var(key)
        .ok()
        .and_then(|link| saf_cofl::cofl_socket_session_id(&link))
        .is_some()
}

#[cfg(feature = "live-cofl")]
fn smoke_account_env_key(prefix: &str, account: &AccountId) -> String {
    let suffix = account
        .as_str()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{prefix}_{suffix}")
}

#[cfg(feature = "live-minecraft")]
fn require_azalea_minecraft(probe: &str) {
    assert!(
        rust_minecraft_is_azalea(),
        "{probe} requires SAF_RUST_MINECRAFT=azalea"
    );
}

#[cfg(feature = "live-minecraft")]
fn rust_minecraft_is_azalea() -> bool {
    std::env::var("SAF_RUST_MINECRAFT")
        .is_ok_and(|value| value.trim().eq_ignore_ascii_case("azalea"))
}

#[cfg(feature = "live-discord")]
fn discord_allowed_user_env_count() -> usize {
    std::env::var("SAF_DISCORD_ALLOWED_IDS")
        .ok()
        .into_iter()
        .flat_map(|raw| {
            raw.split(',')
                .filter_map(normalize_discord_user_id)
                .collect::<Vec<_>>()
        })
        .collect::<BTreeSet<_>>()
        .len()
}

#[cfg(feature = "live-discord")]
fn normalize_discord_user_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<u64>().ok().map(|id| id.to_string())
}

fn smoke_config() -> SafConfig {
    if configured_smoke_required() {
        configured_smoke_config()
    } else {
        synthetic_smoke_config()
    }
}

fn synthetic_smoke_config() -> SafConfig {
    let ign = smoke_ign();
    let mut config = SafConfig {
        igns: vec![ign.clone()],
        default_ign: ign,
        ..SafConfig::default()
    };
    apply_live_smoke_cofl_session(&mut config);
    config
}

fn configured_smoke_config() -> SafConfig {
    let path = std::env::var("SAF_LIVE_SMOKE_CONFIG")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("config.json5"));
    let config = SafConfig::from_path(&path).unwrap_or_else(|error| {
        panic!(
            "live smoke config {} failed to load: {error}",
            path.display()
        )
    });
    let config = config_with_account_environment(&config, &AccountEnvironment::current());
    let requested = std::env::var("SAF_LIVE_SMOKE_IGN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let mut config = scoped_smoke_config(config, requested.as_deref(), "live smoke")
        .unwrap_or_else(|error| panic!("{error}"));
    apply_live_smoke_cofl_session(&mut config);
    config
}

fn scoped_smoke_config(
    mut config: SafConfig,
    requested_ign: Option<&str>,
    context: &str,
) -> Result<SafConfig, String> {
    let configured = config.configured_igns();
    if configured.is_empty() {
        return Err(format!(
            "{context} requires config.json5 to define at least one account"
        ));
    }

    let account = if let Some(requested) = requested_ign
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        configured
            .iter()
            .find(|account| account.eq_ignore_ascii_case(requested))
            .cloned()
            .ok_or_else(|| {
                format!(
                    "{context} requires SAF_LIVE_SMOKE_IGN `{requested}` to be present in configured accounts: {}",
                    configured.join(", ")
                )
            })?
    } else {
        config
            .default_account()
            .or_else(|| configured.first().cloned())
            .ok_or_else(|| format!("{context} could not resolve a default configured account"))?
    };

    config.igns = vec![account.clone()];
    config.default_ign = account;
    config.start_default_only = true;
    Ok(config)
}

fn apply_live_smoke_cofl_session(config: &mut SafConfig) {
    if let Ok(session) = std::env::var("SAF_LIVE_SMOKE_COFL_SESSION") {
        let session = session.trim();
        if !session.is_empty() {
            config.session = session.to_string();
        }
    }
}

fn smoke_options(temp: &tempfile::TempDir, inbox: impl Into<std::path::PathBuf>) -> RunLiveOptions {
    let mut options = RunLiveOptions::new(inbox, temp.path());
    options.once = true;
    options.market_actions = MarketActionMode::DryRun;
    options.connect_cofl = false;
    options.poll_interval = Duration::from_millis(100);
    options
}

#[test]
fn configured_smoke_config_scopes_to_requested_configured_account() {
    let config = SafConfig {
        igns: vec!["Main".to_string(), "Alt".to_string()],
        default_ign: "Main".to_string(),
        start_default_only: false,
        session: "redacted-existing-session".to_string(),
        ..SafConfig::default()
    };

    let scoped = scoped_smoke_config(config, Some("alt"), "test smoke").unwrap();

    assert_eq!(scoped.igns, vec!["Alt".to_string()]);
    assert_eq!(scoped.default_ign, "Alt");
    assert!(scoped.start_default_only);
    assert_eq!(scoped.session, "redacted-existing-session");
}

#[test]
fn configured_smoke_config_rejects_unconfigured_requested_account() {
    let config = SafConfig {
        igns: vec!["Main".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };

    let error = scoped_smoke_config(config, Some("Alt"), "test smoke").unwrap_err();

    assert!(error.contains("SAF_LIVE_SMOKE_IGN `Alt`"));
    assert!(error.contains("Main"));
}

#[tokio::test]
async fn safe_command_inbox_smoke_is_env_gated() {
    if !smoke_enabled() {
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands.jsonl");
    tokio::fs::write(
        &inbox,
        concat!(
            "{\"type\":\"terminal\",\"line\":\"users\"}\n",
            "{\"type\":\"terminal\",\"line\":\"list_item smoke-item-uuid 1m\"}\n"
        ),
    )
    .await
    .unwrap();

    let report = run_live(smoke_config(), smoke_options(&temp, &inbox))
        .await
        .unwrap();

    assert_eq!(report.processed_commands, 2);
    assert_eq!(report.dry_run_market_actions, 1);
}

#[tokio::test]
async fn dry_run_market_queue_smoke_is_env_gated() {
    if !smoke_enabled() {
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new(smoke_ign()).unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"auctionID": "smoke-auction"}),
            BotState::Buying,
            5,
        )
        .await
        .unwrap();

    let inbox = temp.path().join("commands.jsonl");
    let report = run_live(smoke_config(), smoke_options(&temp, &inbox))
        .await
        .unwrap();

    assert_eq!(report.processed_queue_steps, 1);
    assert_eq!(report.dry_run_market_actions, 1);
}

#[tokio::test]
async fn dry_run_window_click_smoke_is_env_gated() {
    if !smoke_enabled() {
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let account = AccountId::new(smoke_ign()).unwrap();
    FileQueueStore::new(temp.path())
        .add(
            &account,
            json!({"auctionID": "smoke-auction"}),
            BotState::Buying,
            5,
        )
        .await
        .unwrap();

    let inbox = temp.path().join("commands.jsonl");
    let mut runtime = LiveRuntime::start(smoke_config(), smoke_options(&temp, &inbox))
        .await
        .unwrap();
    runtime
        .remember_window_snapshot(
            account,
            WindowSnapshot {
                title: "BIN Auction View".to_string(),
                slots: vec![WindowSlot {
                    slot: 33,
                    name: "gold_nugget".to_string(),
                    display_name: "Buy Item Right Now".to_string(),
                    lore: vec!["Click to buy".to_string()],
                    item_uuid: None,
                }],
            },
        )
        .unwrap();
    runtime.poll_once().await.unwrap();

    let report = runtime.report();
    assert_eq!(report.processed_queue_steps, 1);
    assert_eq!(report.dry_run_market_actions, 1);
}

#[tokio::test]
async fn discord_command_planning_smoke_is_env_gated() {
    if !smoke_probe_enabled("SAF_LIVE_SMOKE_DISCORD") {
        return;
    }

    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands.jsonl");
    let runtime = LiveRuntime::start(smoke_config(), smoke_options(&temp, &inbox))
        .await
        .unwrap();
    let plan = saf_discord::plan_invocation(&saf_discord::CommandInvocation::new("users"))
        .expect("users command should plan");
    let saf_discord::DiscordCommandPlan::LocalCommand { command } = plan else {
        panic!("users command should route to a local runtime command");
    };

    let outcome = runtime
        .session()
        .process_local_command(command)
        .await
        .unwrap();

    assert!(matches!(outcome, RuntimeOutcome::UsersSnapshot { .. }));
}

#[tokio::test]
async fn transfer_command_dry_run_records_source_deposit_without_saved_mutation() {
    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands.jsonl");
    let config = SafConfig {
        igns: vec!["Main".to_string(), "Alt".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let runtime = LiveRuntime::start(config, smoke_options(&temp, &inbox))
        .await
        .unwrap();
    let command = match saf_discord::plan_invocation(
        &saf_discord::CommandInvocation::new("transfer_coins")
            .with_string("from", "Main")
            .with_string("to", "Alt")
            .with_string("amount", "1m"),
    )
    .unwrap()
    {
        saf_discord::DiscordCommandPlan::LocalCommand { command } => command,
        other => panic!("transfer should route to a local command, got {other:?}"),
    };

    let outcome = runtime
        .session()
        .process_local_command(command)
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::Queued { changed: true, .. }
    ));
    assert!(
        FileQueueStore::new(temp.path())
            .snapshot(&AccountId::new("Main").unwrap())
            .await
            .unwrap()
            .is_empty()
    );
    let records = runtime.dry_run_records();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].account, AccountId::new("Main").unwrap());
    assert_eq!(records[0].state, BotState::Custom("bank".to_string()));
    assert_eq!(records[0].action["amount"], json!(1_000_000));
    assert_eq!(records[0].action["withdraw"], json!(false));
    assert_eq!(records[0].action["personal"], json!(false));
    assert_eq!(records[0].action["transfer"]["to"], json!("Alt"));
    assert_eq!(records[0].action["transfer"]["stopSource"], json!(true));
}

#[tokio::test]
async fn transfer_command_live_mode_queues_source_deposit_from_public_route() {
    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands.jsonl");
    let config = SafConfig {
        igns: vec!["Main".to_string(), "Alt".to_string()],
        default_ign: "Main".to_string(),
        ..SafConfig::default()
    };
    let mut options = smoke_options(&temp, &inbox);
    options.market_actions = MarketActionMode::Live;
    let runtime = LiveRuntime::start(config, options).await.unwrap();

    let outcome = runtime
        .session()
        .process_local_command(saf_core::LocalCommand::Terminal {
            line: "transfer_coins Main Alt 1m".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::Queued { changed: true, .. }
    ));
    let queue = FileQueueStore::new(temp.path())
        .snapshot(&AccountId::new("Main").unwrap())
        .await
        .unwrap();
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].state, BotState::Custom("bank".to_string()));
    assert_eq!(queue[0].priority, 5);
    assert_eq!(queue[0].action["amount"], json!(1_000_000));
    assert_eq!(queue[0].action["withdraw"], json!(false));
    assert_eq!(queue[0].action["personal"], json!(false));
    assert_eq!(queue[0].action["transfer"]["to"], json!("Alt"));
    assert_eq!(queue[0].action["transfer"]["stopSource"], json!(true));
    assert!(runtime.dry_run_records().is_empty());
}

#[cfg(feature = "live-discord")]
#[tokio::test]
async fn discord_gateway_start_smoke_is_env_gated() {
    if !smoke_probe_enabled("SAF_LIVE_SMOKE_DISCORD_GATEWAY") {
        return;
    }

    assert!(
        env_truthy("SAF_DISCORD_BOT_ENABLED"),
        "Discord gateway smoke requires SAF_DISCORD_BOT_ENABLED=1"
    );
    let missing = ["SAF_DISCORD_TOKEN"]
        .into_iter()
        .filter(|name| std::env::var(name).unwrap_or_default().trim().is_empty())
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "Discord gateway smoke requires {}",
        missing.join(", ")
    );
    assert!(
        discord_allowed_user_env_count() > 0,
        "Discord gateway smoke requires SAF_DISCORD_ALLOWED_IDS with at least one numeric Discord user ID"
    );

    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands.jsonl");
    let mut options = smoke_options(&temp, &inbox);
    options.once = false;
    options.connect_cofl = false;
    let runtime = LiveRuntime::start(smoke_config(), options).await.unwrap();

    assert!(runtime.report().discord_started);
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(!runtime.discord_gateway_task_finished());
    runtime.shutdown().await;
}

#[cfg(not(feature = "live-discord"))]
#[tokio::test]
async fn discord_gateway_start_smoke_is_env_gated() {
    assert!(
        !smoke_probe_enabled("SAF_LIVE_SMOKE_DISCORD_GATEWAY"),
        "SAF_LIVE_SMOKE_DISCORD_GATEWAY requires the live-discord feature"
    );
}

#[cfg(feature = "live-cofl")]
#[tokio::test]
async fn cofl_connect_smoke_is_env_gated() {
    if !smoke_probe_enabled("SAF_LIVE_SMOKE_COFL") {
        return;
    }
    require_cofl_smoke_auth("Cofl smoke");
    require_non_empty_env("SAF_LIVE_SMOKE_IGN", "Cofl smoke");

    let temp = tempfile::tempdir().unwrap();
    let inbox = temp.path().join("commands.jsonl");
    let mut options = smoke_options(&temp, &inbox);
    options.connect_cofl = true;
    let mut runtime = LiveRuntime::start(smoke_config(), options).await.unwrap();

    for _ in 0..20 {
        runtime.poll_once().await.unwrap();
        if runtime.cofl_connected_count().await > 0 {
            runtime.shutdown().await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    runtime.shutdown().await;
    panic!("Cofl websocket did not connect during the live smoke window");
}

#[cfg(not(feature = "live-cofl"))]
#[tokio::test]
async fn cofl_connect_smoke_is_env_gated() {
    assert!(
        !smoke_probe_enabled("SAF_LIVE_SMOKE_COFL"),
        "SAF_LIVE_SMOKE_COFL requires the live-cofl feature"
    );
}

#[cfg(feature = "live-minecraft")]
#[tokio::test]
async fn minecraft_login_readiness_smoke_is_env_gated() {
    if !smoke_probe_enabled("SAF_LIVE_SMOKE_LOGIN") {
        return;
    }
    require_azalea_minecraft("Minecraft login smoke");
    require_non_empty_env("SAF_LIVE_SMOKE_IGN", "Minecraft login smoke");

    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = tempfile::tempdir().unwrap();
            let inbox = temp.path().join("commands.jsonl");
            let mut runtime = LiveRuntime::start(smoke_config(), smoke_options(&temp, &inbox))
                .await
                .unwrap();

            for _ in 0..30 {
                runtime.poll_once().await.unwrap();
                if runtime.report().processed_minecraft_events > 0 {
                    runtime.shutdown().await;
                    return;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }

            runtime.shutdown().await;
            panic!("Minecraft client did not emit readiness events during the live smoke window");
        })
        .await;
}

#[cfg(not(feature = "live-minecraft"))]
#[tokio::test]
async fn minecraft_login_readiness_smoke_is_env_gated() {
    assert!(
        !smoke_probe_enabled("SAF_LIVE_SMOKE_LOGIN"),
        "SAF_LIVE_SMOKE_LOGIN requires the live-minecraft feature"
    );
}

#[cfg(feature = "live-minecraft")]
#[tokio::test]
async fn inventory_snapshot_smoke_is_env_gated() {
    if !smoke_probe_enabled("SAF_LIVE_SMOKE_INVENTORY") {
        return;
    }
    require_azalea_minecraft("inventory snapshot smoke");
    require_non_empty_env("SAF_LIVE_SMOKE_IGN", "inventory snapshot smoke");

    tokio::task::LocalSet::new()
        .run_until(async {
            let temp = tempfile::tempdir().unwrap();
            let inbox = temp.path().join("commands.jsonl");
            let runtime = LiveRuntime::start(smoke_config(), smoke_options(&temp, &inbox))
                .await
                .unwrap();
            let mut runtime = runtime;
            wait_for_minecraft_readiness(&mut runtime).await;
            let outcome = runtime
                .session()
                .process_local_command(saf_core::LocalCommand::Terminal {
                    line: "inventory".to_string(),
                    created_at: None,
                })
                .await
                .unwrap();

            assert!(matches!(outcome, RuntimeOutcome::InventorySnapshot { .. }));
            runtime.shutdown().await;
        })
        .await;
}

#[cfg(not(feature = "live-minecraft"))]
#[tokio::test]
async fn inventory_snapshot_smoke_is_env_gated() {
    assert!(
        !smoke_probe_enabled("SAF_LIVE_SMOKE_INVENTORY"),
        "SAF_LIVE_SMOKE_INVENTORY requires the live-minecraft feature"
    );
}

#[cfg(all(
    feature = "live-cofl",
    feature = "live-discord",
    feature = "live-minecraft"
))]
#[tokio::test]
async fn full_runtime_acceptance_smoke_is_env_gated() {
    if !smoke_probe_enabled("SAF_LIVE_SMOKE_FULL") {
        return;
    }

    assert!(
        env_truthy("SAF_DISCORD_BOT_ENABLED"),
        "full smoke requires SAF_DISCORD_BOT_ENABLED=1"
    );
    let missing = ["SAF_DISCORD_TOKEN", "SAF_LIVE_SMOKE_IGN"]
        .into_iter()
        .filter(|name| std::env::var(name).unwrap_or_default().trim().is_empty())
        .collect::<Vec<_>>();
    assert!(
        missing.is_empty(),
        "full smoke requires {}",
        missing.join(", ")
    );
    assert!(
        discord_allowed_user_env_count() > 0,
        "full smoke requires SAF_DISCORD_ALLOWED_IDS with at least one numeric Discord user ID"
    );
    require_cofl_smoke_auth("full smoke");
    assert!(
        rust_minecraft_is_azalea(),
        "full smoke requires SAF_RUST_MINECRAFT=azalea"
    );

    tokio::task::LocalSet::new()
        .run_until(async {
            let dry_run = tempfile::tempdir().unwrap();
            let dry_inbox = dry_run.path().join("commands.jsonl");
            let mut dry_options = smoke_options(&dry_run, dry_inbox.clone());
            dry_options.once = false;
            dry_options.connect_cofl = true;
            dry_options.market_actions = MarketActionMode::DryRun;
            let mut runtime = LiveRuntime::start(smoke_config(), dry_options)
                .await
                .unwrap();
            assert!(runtime.report().discord_started);
            wait_for_full_runtime_readiness(&mut runtime).await;
            assert_live_inventory_snapshot(&runtime).await;
            assert_discord_command_path(&runtime).await;
            assert_command_inbox_path(&mut runtime, &dry_inbox).await;
            runtime.shutdown().await;

            let live = tempfile::tempdir().unwrap();
            let live_inbox = live.path().join("commands.jsonl");
            let mut live_options = smoke_options(&live, live_inbox.clone());
            live_options.once = false;
            live_options.connect_cofl = true;
            live_options.market_actions = MarketActionMode::Live;
            let mut runtime = LiveRuntime::start(smoke_config(), live_options)
                .await
                .unwrap();
            assert!(runtime.report().discord_started);
            wait_for_full_runtime_readiness(&mut runtime).await;
            assert_live_inventory_snapshot(&runtime).await;
            assert_discord_command_path(&runtime).await;
            assert_command_inbox_path(&mut runtime, &live_inbox).await;
            assert_eq!(runtime.report().dry_run_market_actions, 0);
            runtime.shutdown().await;
        })
        .await;
}
#[cfg(all(
    feature = "live-cofl",
    feature = "live-discord",
    feature = "live-minecraft"
))]
async fn wait_for_full_runtime_readiness(runtime: &mut LiveRuntime) {
    for _ in 0..60 {
        runtime.poll_once().await.unwrap();
        if runtime.cofl_connected_count().await > 0
            && runtime.report().processed_minecraft_events > 0
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    panic!("full runtime smoke did not observe Cofl and Minecraft readiness");
}

#[cfg(all(
    feature = "live-cofl",
    feature = "live-discord",
    feature = "live-minecraft"
))]
async fn assert_live_inventory_snapshot(runtime: &LiveRuntime) {
    let outcome = runtime
        .session()
        .process_local_command(saf_core::LocalCommand::Terminal {
            line: "inventory".to_string(),
            created_at: None,
        })
        .await
        .expect("full runtime smoke should read a live inventory snapshot");

    assert!(
        matches!(outcome, RuntimeOutcome::InventorySnapshot { .. }),
        "full runtime smoke should return an inventory snapshot"
    );
}

#[cfg(all(
    feature = "live-cofl",
    feature = "live-discord",
    feature = "live-minecraft"
))]
#[derive(Clone, Copy)]
enum ExpectedDiscordOutcome {
    Connections,
    GlobalStats,
    Inventory,
    Logs,
    Ping,
    Profit,
    Queue,
    Stats,
    Users,
}

#[cfg(all(
    feature = "live-cofl",
    feature = "live-discord",
    feature = "live-minecraft"
))]
async fn assert_discord_command_path(runtime: &LiveRuntime) {
    for (invocation, expected) in [
        (
            saf_discord::CommandInvocation::new("connections"),
            ExpectedDiscordOutcome::Connections,
        ),
        (
            saf_discord::CommandInvocation::new("global_stats"),
            ExpectedDiscordOutcome::GlobalStats,
        ),
        (
            saf_discord::CommandInvocation::new("inventory"),
            ExpectedDiscordOutcome::Inventory,
        ),
        (
            saf_discord::CommandInvocation::new("logs"),
            ExpectedDiscordOutcome::Logs,
        ),
        (
            saf_discord::CommandInvocation::new("messages").with_integer("lines", 5),
            ExpectedDiscordOutcome::Logs,
        ),
        (
            saf_discord::CommandInvocation::new("ping"),
            ExpectedDiscordOutcome::Ping,
        ),
        (
            saf_discord::CommandInvocation::new("profit"),
            ExpectedDiscordOutcome::Profit,
        ),
        (
            saf_discord::CommandInvocation::new("queue"),
            ExpectedDiscordOutcome::Queue,
        ),
        (
            saf_discord::CommandInvocation::new("stats"),
            ExpectedDiscordOutcome::Stats,
        ),
        (
            saf_discord::CommandInvocation::new("users"),
            ExpectedDiscordOutcome::Users,
        ),
    ] {
        assert_discord_local_command_outcome(runtime, invocation, expected).await;
    }
}

#[cfg(all(
    feature = "live-cofl",
    feature = "live-discord",
    feature = "live-minecraft"
))]
async fn assert_discord_local_command_outcome(
    runtime: &LiveRuntime,
    invocation: saf_discord::CommandInvocation,
    expected: ExpectedDiscordOutcome,
) {
    let command_name = invocation.name.clone();
    let plan = saf_discord::plan_invocation(&invocation)
        .unwrap_or_else(|error| panic!("full runtime smoke should plan /{command_name}: {error}"));
    let saf_discord::DiscordCommandPlan::LocalCommand { command } = plan else {
        panic!("full runtime smoke should route /{command_name} to a local runtime command");
    };
    let outcome = runtime
        .session()
        .process_local_command(command)
        .await
        .unwrap_or_else(|error| {
            panic!(
                "full runtime smoke should execute safe Discord command /{command_name}: {error}"
            )
        });

    assert!(
        discord_outcome_matches(&outcome, expected),
        "full runtime smoke should return the expected outcome for /{command_name}: {outcome:?}"
    );
}

#[cfg(all(
    feature = "live-cofl",
    feature = "live-discord",
    feature = "live-minecraft"
))]
fn discord_outcome_matches(outcome: &RuntimeOutcome, expected: ExpectedDiscordOutcome) -> bool {
    matches!(
        (expected, outcome),
        (
            ExpectedDiscordOutcome::Connections,
            RuntimeOutcome::ConnectionsSnapshot { .. }
        ) | (
            ExpectedDiscordOutcome::GlobalStats,
            RuntimeOutcome::GlobalStatsSnapshot { .. }
        ) | (
            ExpectedDiscordOutcome::Inventory,
            RuntimeOutcome::InventorySnapshot { .. }
        ) | (
            ExpectedDiscordOutcome::Logs,
            RuntimeOutcome::LogSnapshot { .. }
        ) | (
            ExpectedDiscordOutcome::Ping,
            RuntimeOutcome::PingSnapshot { .. }
        ) | (
            ExpectedDiscordOutcome::Profit,
            RuntimeOutcome::ProfitSnapshot { .. }
        ) | (
            ExpectedDiscordOutcome::Queue,
            RuntimeOutcome::QueueSnapshot { .. }
        ) | (
            ExpectedDiscordOutcome::Stats,
            RuntimeOutcome::StatsSnapshot { .. }
        ) | (
            ExpectedDiscordOutcome::Users,
            RuntimeOutcome::UsersSnapshot { .. }
        )
    )
}

#[cfg(all(
    feature = "live-cofl",
    feature = "live-discord",
    feature = "live-minecraft"
))]
async fn assert_command_inbox_path(runtime: &mut LiveRuntime, inbox: &std::path::Path) {
    tokio::fs::write(inbox, "{\"type\":\"terminal\",\"line\":\"users\"}\n")
        .await
        .expect("full runtime smoke should be able to write the command inbox");

    let results = runtime
        .poll_once()
        .await
        .expect("full runtime smoke should process the command inbox");

    assert_eq!(
        results.len(),
        1,
        "full runtime smoke should process exactly one inbox command"
    );
    let Some(saf_app::inbox::InboxCommandResult::Processed {
        outcome: RuntimeOutcome::UsersSnapshot { .. },
        ..
    }) = results.first()
    else {
        panic!("full runtime smoke should process a safe users command from the inbox");
    };
}

#[cfg(feature = "live-minecraft")]
async fn wait_for_minecraft_readiness(runtime: &mut LiveRuntime) {
    for _ in 0..30 {
        runtime.poll_once().await.unwrap();
        if runtime.report().processed_minecraft_events > 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    panic!("Minecraft client did not emit readiness events during the live smoke window");
}

#[cfg(not(all(
    feature = "live-cofl",
    feature = "live-discord",
    feature = "live-minecraft"
)))]
#[tokio::test]
async fn full_runtime_acceptance_smoke_is_env_gated() {
    assert!(
        !smoke_probe_enabled("SAF_LIVE_SMOKE_FULL"),
        "SAF_LIVE_SMOKE_FULL requires the production-runtime feature bundle"
    );
}
