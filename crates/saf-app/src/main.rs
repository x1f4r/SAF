use anyhow::{Context, Result};
use clap::Parser;
mod cli;
#[cfg(feature = "live-cofl")]
mod cofl_auth_link;
use cli::{Cli, Commands, EnqueueCommands, StateCommands};
mod live_cli;
#[cfg(feature = "discord-webhook")]
use live_cli::webhook_identity;
use live_cli::{prepare_cofl_session, validate_run_live_safety};
#[cfg(feature = "live-minecraft")]
use saf_app::config_session::account_env_key;
use saf_app::inbox::RecordedRuntimeSession;
use saf_app::live_preflight::{LivePreflightOptions, run_live_preflight};
use saf_app::live_runtime::{DEFAULT_RUN_LIVE_POLL_INTERVAL_MS, RunLiveOptions, run_live};
use saf_app::state_cli;
use saf_cofl::CoflEnvelope;
#[cfg(feature = "discord-webhook")]
use saf_core::ports::Notification;
use saf_core::{
    AccountEnvironment, AccountId, AccountSelector, BlacklistPolicy, BotRuntime, BotState,
    BuyThresholds, FlipEvent, FlipProcessor, LocalCommand, LocalCommandLine, RoutedCommand,
    RuntimeSession, SafConfig, SkipPolicy, config_with_account_environment,
    local_command::parse_jsonl,
};
#[cfg(feature = "discord-webhook")]
use saf_discord::webhook_notifier::DiscordWebhookNotifier;
use saf_discord::{CommandInvocation, plan_invocation};
use saf_minecraft::RecordedMinecraftClient;
use serde_json::Value;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "saf=info".into()),
        )
        .init();

    let cli = Cli::parse();
    match cli.command.unwrap_or(Commands::CheckConfig) {
        Commands::CheckConfig => {
            let config = SafConfig::from_path(&cli.config)
                .with_context(|| format!("loading {}", cli.config.display()))?;
            println!(
                "config ok: {} configured account(s), startup target(s): {}",
                config.configured_igns().len(),
                config.startup_igns().join(", ")
            );
        }
        Commands::ParseInbox { file } => {
            let raw = tokio::fs::read_to_string(&file)
                .await
                .with_context(|| format!("reading {}", file.display()))?;
            for line in parse_jsonl(&raw) {
                match line {
                    LocalCommandLine::Parsed(command) => println!("{command:?}"),
                    LocalCommandLine::Invalid { line, error } => {
                        println!("invalid: {error}: {line}")
                    }
                }
            }
        }
        Commands::ProcessInbox {
            file,
            running,
            state_base_dir,
            cofl_auction_api,
        } => {
            let config = SafConfig::from_path(&cli.config)
                .with_context(|| format!("loading {}", cli.config.display()))?;
            let raw = tokio::fs::read_to_string(&file)
                .await
                .with_context(|| format!("reading {}", file.display()))?;
            let mut session = if let Some(state_base_dir) = state_base_dir {
                RecordedRuntimeSession::from_config_with_file_queue(
                    &config,
                    running,
                    state_base_dir,
                )
            } else {
                RecordedRuntimeSession::from_config(&config, running)
            };
            if cofl_auction_api {
                enable_cofl_auction_metadata(&mut session)?;
            }
            let report = session.process_inbox_report(&raw).await;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::EnqueueCommand { file, command } => {
            let command = match command {
                EnqueueCommands::Terminal { line } => LocalCommand::Terminal {
                    line: line.join(" "),
                    created_at: Some(current_time_ms()?),
                },
                EnqueueCommands::Transfer { from, to, amount } => LocalCommand::Transfer {
                    from,
                    to,
                    amount,
                    stop_source: true,
                    created_at: Some(current_time_ms()?),
                },
            };
            append_inbox_command(&file, &command).await?;
            println!("{}", serde_json::to_string(&command)?);
        }
        Commands::RunLive {
            command_inbox,
            state_base_dir,
            market_actions,
            poll_interval_ms,
            once,
            disable_cofl,
        } => {
            let config = SafConfig::from_path(&cli.config)
                .with_context(|| format!("loading {}", cli.config.display()))?;
            let mut config =
                config_with_account_environment(&config, &AccountEnvironment::current());
            let mut options = RunLiveOptions::new(command_inbox, state_base_dir);
            options.config_path = cli.config.clone();
            options.market_actions = market_actions;
            options.poll_interval = std::time::Duration::from_millis(
                poll_interval_ms.max(DEFAULT_RUN_LIVE_POLL_INTERVAL_MS),
            );
            options.once = once;
            options.connect_cofl = !disable_cofl;
            prepare_cofl_session(&mut config, &options)?;
            validate_run_live_safety(&config, &options)?;
            let report = tokio::task::LocalSet::new()
                .run_until(run_live(config, options))
                .await
                .context("running Rust live runtime")?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Commands::LivePreflight {
            command_inbox,
            state_base_dir,
            require_discord,
            require_cofl,
            require_minecraft,
            full,
        } => {
            let config = SafConfig::from_path(&cli.config)
                .with_context(|| format!("loading {}", cli.config.display()))?;
            let report = run_live_preflight(
                &config,
                &LivePreflightOptions {
                    command_inbox,
                    state_base_dir,
                    require_discord,
                    require_cofl,
                    require_minecraft,
                    full,
                },
            );
            let ok = report.ok;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if !ok {
                std::process::exit(2);
            }
        }
        #[cfg(feature = "live-minecraft")]
        Commands::MinecraftAuth { account, cache_key } => {
            let config = SafConfig::from_path(&cli.config)
                .with_context(|| format!("loading {}", cli.config.display()))?;
            let account = account
                .or_else(|| config.default_account())
                .context("account is required; pass an account or configure defaultIgn")?;
            let account = AccountId::new(account).context("account is required")?;
            let env_key = account_env_key("SAF_MICROSOFT_CACHE", &account);
            let env_cache_key = std::env::var(&env_key)
                .ok()
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty());
            let (cache_key, cache_key_source) = match cache_key
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
            {
                Some(cache_key) => (cache_key, "argument".to_string()),
                None => match env_cache_key {
                    Some(cache_key) => (cache_key, env_key),
                    None => (account.to_string(), "account".to_string()),
                },
            };
            let report = saf_minecraft::azalea_native::authenticate_microsoft_cache(&cache_key)
                .await
                .context("authenticating Microsoft Minecraft account")?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "type": "minecraftAuthReady",
                    "account": account.as_str(),
                    "cacheKeySource": cache_key_source,
                    "username": report.username,
                    "uuid": report.uuid,
                }))?
            );
        }
        Commands::State { command } => match command {
            StateCommands::Snapshot {
                account_uuid,
                base_dir,
            } => {
                let snapshot = state_cli::snapshot(base_dir, &account_uuid)
                    .context("loading state snapshot")?;
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            }
            StateCommands::Add {
                account_uuid,
                state,
                priority,
                action,
                base_dir,
                save,
            } => {
                let action =
                    serde_json::from_str::<Value>(&action).context("parsing state action json")?;
                let report = state_cli::add_queue_entry(
                    base_dir,
                    &account_uuid,
                    action,
                    BotState::from(state.as_str()),
                    priority,
                    save,
                )
                .context("adding queue entry")?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            StateCommands::RemoveNext {
                account_uuid,
                base_dir,
                save,
            } => {
                let report = state_cli::remove_next(base_dir, &account_uuid, save)
                    .context("removing next queue entry")?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            StateCommands::Clear {
                account_uuid,
                base_dir,
                save,
            } => {
                let report = state_cli::clear_queue(base_dir, &account_uuid, save)
                    .context("clearing queue entries")?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
            StateCommands::ClearData {
                account_uuid,
                base_dir,
                save,
            } => {
                let report = state_cli::clear_saved_data(base_dir, &account_uuid, save)
                    .context("clearing saved queue and bid data")?;
                println!("{}", serde_json::to_string_pretty(&report)?);
            }
        },
        Commands::RouteCommand {
            line,
            configured,
            running,
            default_ign,
        } => {
            let selector = AccountSelector {
                configured,
                running,
                ask_prefixes: Vec::new(),
                default_ign,
            };
            match RoutedCommand::parse(&line, &selector) {
                Ok(command) => println!(
                    "account={} command={} message={}",
                    command.account.unwrap_or_default(),
                    command.command,
                    command.message
                ),
                Err(error) => println!("error={error}"),
            }
        }
        Commands::SimulateFlip { payload } => {
            let config = SafConfig::from_path(&cli.config)
                .with_context(|| format!("loading {}", cli.config.display()))?;
            let payload =
                serde_json::from_str::<Value>(&payload).context("parsing flip payload")?;
            let processor = FlipProcessor::new(
                BlacklistPolicy::from_config(&config.do_not_buy, &config.do_not_relist),
                SkipPolicy::from_config(&config.skip),
                BuyThresholds::default(),
            );
            let account = config
                .default_account()
                .unwrap_or_else(|| "offline".to_string());
            let account = AccountId::new(account).context("simulated account is required")?;
            let client = RecordedMinecraftClient::new(account);
            let outcome = processor
                .process(&client, FlipEvent::from_payload(&payload))
                .await
                .context("processing flip")?;
            println!("{outcome:?}");
        }
        Commands::PlanCommand {
            line,
            configured,
            running,
            default_ign,
        } => {
            let mut config = SafConfig::from_path(&cli.config)
                .with_context(|| format!("loading {}", cli.config.display()))?;
            if !configured.is_empty() {
                config.igns = configured;
            }
            if let Some(default_ign) = default_ign {
                config.default_ign = default_ign;
            }
            let running = if running.is_empty() {
                config.startup_igns()
            } else {
                running
            };
            let runtime = BotRuntime::from_config(&config, running);
            let directive = runtime
                .plan_terminal_line(&line)
                .context("planning terminal command")?;
            println!("{}", serde_json::to_string_pretty(&directive)?);
        }
        Commands::PlanDiscord { payload } => {
            let invocation = serde_json::from_str::<CommandInvocation>(&payload)
                .context("parsing Discord invocation json")?;
            let plan = plan_invocation(&invocation).context("planning Discord invocation")?;
            println!("{}", serde_json::to_string_pretty(&plan)?);
        }
        Commands::ProcessEnvelope { account, payload } => {
            let config = SafConfig::from_path(&cli.config)
                .with_context(|| format!("loading {}", cli.config.display()))?;
            let envelope =
                CoflEnvelope::from_wire(payload.as_bytes()).context("parsing envelope")?;
            let runtime = BotRuntime::from_config(&config, vec![account.clone()]);
            let account = AccountId::new(account).context("account is required")?;
            let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
            let mut session = RuntimeSession::new(runtime);
            session.add_minecraft_client(account.clone(), client);
            if let Some(flip) = envelope.parse_flip() {
                let outcome = session
                    .process_flip(&account, flip)
                    .await
                    .context("processing envelope flip")?;
                println!("{}", serde_json::to_string_pretty(&outcome)?);
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "type": "ignoredEnvelope",
                        "kind": envelope.kind
                    }))?
                );
            }
        }
        #[cfg(feature = "discord-webhook")]
        Commands::Notify {
            title,
            body,
            account,
        } => {
            let config = SafConfig::from_path(&cli.config)
                .with_context(|| format!("loading {}", cli.config.display()))?;
            let mut session =
                RuntimeSession::new(BotRuntime::from_config(&config, config.startup_igns()));
            let webhook_count = config.webhook.len();
            let notifier =
                DiscordWebhookNotifier::new(config.webhook.clone(), webhook_identity(&config));
            session.set_notifier(Arc::new(notifier));
            session
                .notify(Notification {
                    title,
                    body,
                    account: account
                        .map(|account| AccountId::new(account).context("account is required"))
                        .transpose()?,
                    ..Notification::default()
                })
                .await
                .context("sending notification")?;
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "type": "notificationSent",
                    "webhooks": webhook_count
                }))?
            );
        }
        #[cfg(feature = "live-cofl")]
        Commands::CoflAuthLink {
            account,
            socket_link,
            timeout_ms,
        } => {
            cofl_auth_link::print_cofl_auth_link(&cli.config, account, socket_link, timeout_ms)
                .await?;
        }
        #[cfg(feature = "live-cofl")]
        Commands::CoflOnce {
            account,
            socket_link,
        } => {
            let config = SafConfig::from_path(&cli.config)
                .with_context(|| format!("loading {}", cli.config.display()))?;
            let account = AccountId::new(account).context("account is required")?;
            let runtime = BotRuntime::from_config(&config, vec![account.to_string()]);
            let mut session = RuntimeSession::new(runtime);
            let cofl =
                saf_cofl::ws_client::CoflWebSocketClient::connect(account.clone(), &socket_link)
                    .await
                    .context("connecting cofl websocket")?;
            let cofl = Arc::new(cofl);
            let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
            session.add_minecraft_client(account.clone(), client);
            session.add_cofl_client(account.clone(), cofl.clone());
            if let Some(envelope) = cofl
                .next_envelope()
                .await
                .context("reading cofl envelope")?
            {
                if let Some(flip) = envelope.parse_flip() {
                    let outcome = session
                        .process_flip(&account, flip)
                        .await
                        .context("processing cofl flip")?;
                    println!("{}", serde_json::to_string_pretty(&outcome)?);
                } else {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "type": "ignoredEnvelope",
                            "kind": envelope.kind
                        }))?
                    );
                }
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({
                        "type": "socketClosed"
                    }))?
                );
            }
        }
    }

    Ok(())
}

fn current_time_ms() -> Result<u64> {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX epoch")?
        .as_millis();
    Ok(millis.min(u128::from(u64::MAX)) as u64)
}

async fn append_inbox_command(file: &std::path::Path, command: &LocalCommand) -> Result<()> {
    if let Some(parent) = file
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("creating {}", parent.display()))?;
    }

    let mut handle = tokio::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)
        .await
        .with_context(|| format!("opening {}", file.display()))?;
    let mut line = serde_json::to_vec(command).context("serializing inbox command")?;
    line.push(b'\n');
    handle
        .write_all(&line)
        .await
        .with_context(|| format!("writing {}", file.display()))?;
    Ok(())
}

#[cfg(feature = "live-cofl")]
fn enable_cofl_auction_metadata(session: &mut RecordedRuntimeSession) -> Result<()> {
    session
        .set_auction_metadata_provider(Arc::new(saf_cofl::http_client::CoflHttpClient::default()));
    Ok(())
}

#[cfg(not(feature = "live-cofl"))]
fn enable_cofl_auction_metadata(_session: &mut RecordedRuntimeSession) -> Result<()> {
    anyhow::bail!("--cofl-auction-api requires the live-cofl feature")
}
