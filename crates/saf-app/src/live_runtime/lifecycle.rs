use super::{
    Arc, AtomicBool, BTreeMap, BTreeSet, BankCooldownStore, BotRuntime, CommandInboxCursor,
    Context, DEFAULT_BAD_MOD_KICK_BACKOFF_MS, DEFAULT_IDLE_AUCTION_RECONCILE_MS,
    DEFAULT_LOCRAW_DELAY_MS, DEFAULT_SOLD_AUCTION_POLL_MS, FileBlacklistStore, FileLogReader,
    FileQueueStore, HypixelCookiePriceProvider, LiveAccountScheduler, LiveAccountSupervisor,
    LiveActiveAuctionProvider, LiveAuctionReconcilePoller, LiveIslandState, LiveRuntime,
    LiveSoldTracker, LiveStatsProvider, LiveTrackedFlipProvider, MarketActionQueueStore, Mutex,
    Ordering, Result, RunLiveOptions, RuntimeSession, SafConfig, add_cofl_clients,
    add_minecraft_clients, auto_rotate_schedules, configured_startup_runtime_accounts,
    default_inventory_price_lookup, default_notifier, env_duration_ms, env_optional_duration_ms,
    native_minecraft_enabled, runtime_accounts, sleep, start_auto_rotate_tasks,
    startup_runtime_accounts_with_rotation,
};

#[cfg(feature = "live-discord")]
use super::{
    DEFAULT_DISCORD_GATEWAY_RESTART_MS, Instant, PlayerHeadRefresher, start_discord_gateway,
};

impl LiveRuntime {
    pub async fn start(config: SafConfig, options: RunLiveOptions) -> Result<Self> {
        let accounts = runtime_accounts(&config);
        let configured_startup_accounts = configured_startup_runtime_accounts(&config);
        let startup_paused = startup_paused();
        let active_startup_accounts = if startup_paused {
            tracing::info!("starting Rust controller in paused mode");
            Vec::new()
        } else {
            configured_startup_accounts.clone()
        };
        let auto_rotate_schedules = if startup_paused {
            Vec::new()
        } else {
            auto_rotate_schedules(&config, &configured_startup_accounts)
        };
        let startup_accounts = startup_runtime_accounts_with_rotation(
            &active_startup_accounts,
            &auto_rotate_schedules,
        );
        let running = startup_accounts
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let mut session = RuntimeSession::new(BotRuntime::from_config(&config, running));

        let file_store = FileQueueStore::new(&options.state_base_dir);
        let queue = Arc::new(MarketActionQueueStore::new(
            file_store.clone(),
            options.market_actions,
        ));
        session.set_fallback_queue_store(queue.clone());
        session.set_fallback_saved_data_store(Arc::new(file_store.clone()));
        session.set_fallback_blacklist_store(Arc::new(FileBlacklistStore::new(
            options.config_path.clone(),
            session.blacklist_handle(),
        )));
        let stats = Arc::new(LiveStatsProvider::new(accounts.clone()));
        stats
            .apply_auction_slot_max_overrides(|key| std::env::var(key).ok())
            .context("applying configured auction slot capacity overrides")?;
        let tracked_flips = Arc::new(LiveTrackedFlipProvider::with_saved(file_store));
        session.set_fallback_stats_provider(stats.clone());
        session.set_fallback_connection_provider(stats.clone());
        session.set_fallback_tracked_flip_provider(tracked_flips.clone());
        session.set_log_reader(Arc::new(FileLogReader::new(
            options.state_base_dir.join("logs/latest.log"),
        )));
        session.set_notifier(default_notifier(&config));

        let managed_minecraft = add_minecraft_clients(
            &mut session,
            &accounts,
            &startup_accounts,
            options.market_actions,
            default_inventory_price_lookup(),
        )
        .await?;
        let minecraft_clients = managed_minecraft.clients.clone();
        let managed_minecraft_handles = managed_minecraft.handles.clone();
        let active_windows = Arc::new(Mutex::new(BTreeMap::new()));
        let deferred_minecraft_events = Arc::new(Mutex::new(BTreeMap::new()));
        let locraw_delay = env_duration_ms("SAF_LOCRAW_DELAY_MS", DEFAULT_LOCRAW_DELAY_MS);
        let bad_mod_backoff = env_duration_ms(
            "SAF_BAD_MOD_KICK_BACKOFF_MS",
            DEFAULT_BAD_MOD_KICK_BACKOFF_MS,
        );
        let island_states = accounts
            .iter()
            .map(|account| {
                (
                    account.clone(),
                    LiveIslandState::new(
                        config.use_cookie,
                        config.visit_friend.clone(),
                        native_minecraft_enabled(),
                        locraw_delay,
                        bad_mod_backoff,
                    ),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let cookie_prices = Arc::new(HypixelCookiePriceProvider::default());
        let auction_reconcile_poller = (config.use_cookie && config.relist)
            .then(|| {
                env_optional_duration_ms("SAF_SOLD_AUCTION_POLL_MS", DEFAULT_SOLD_AUCTION_POLL_MS)
                    .map(|poll_interval| {
                        LiveAuctionReconcilePoller::new(
                            &accounts,
                            poll_interval,
                            env_optional_duration_ms(
                                "SAF_IDLE_AUCTION_RECONCILE_MS",
                                DEFAULT_IDLE_AUCTION_RECONCILE_MS,
                            ),
                        )
                    })
            })
            .flatten();
        let bank_cooldowns = BankCooldownStore::for_base_dir(&options.state_base_dir);
        let gui_provider = Arc::new(LiveActiveAuctionProvider::new(
            minecraft_clients.clone(),
            active_windows.clone(),
            deferred_minecraft_events.clone(),
            stats.clone(),
            options.market_actions,
        ));
        session.set_fallback_active_auction_provider(gui_provider.clone());
        session.set_fallback_gui_diagnostics_provider(gui_provider);
        let shutdown = Arc::new(AtomicBool::new(false));
        #[cfg(feature = "live-cofl")]
        let pending_live_buys = Arc::new(Mutex::new(BTreeMap::new()));
        #[cfg(feature = "live-cofl")]
        let (cofl_connections, cofl_streams) = add_cofl_clients(
            &mut session,
            &accounts,
            &config,
            &options,
            tracked_flips.clone(),
            pending_live_buys.clone(),
        )
        .await?;
        #[cfg(not(feature = "live-cofl"))]
        let cofl_connections = add_cofl_clients(&mut session, &accounts, &config, &options).await?;

        #[cfg(feature = "live-cofl")]
        session.set_account_supervisor(Arc::new(LiveAccountSupervisor::new(
            accounts.clone(),
            startup_accounts.clone(),
            managed_minecraft.handles,
            cofl_streams
                .iter()
                .map(|stream| (stream.account.clone(), stream.client.clone()))
                .collect(),
        )));
        #[cfg(not(feature = "live-cofl"))]
        session.set_account_supervisor(Arc::new(LiveAccountSupervisor::new(
            accounts.clone(),
            startup_accounts.clone(),
            managed_minecraft.handles,
        )));

        let scheduler_shutdown = shutdown.clone();
        let session = Arc::new_cyclic(move |weak_session| {
            session.set_fallback_account_scheduler(Arc::new(LiveAccountScheduler::new(
                weak_session.clone(),
                scheduler_shutdown.clone(),
            )));
            session
        });
        let auto_rotate_tasks = if options.once {
            Vec::new()
        } else {
            start_auto_rotate_tasks(auto_rotate_schedules, session.clone(), shutdown.clone())
        };
        #[cfg(feature = "live-discord")]
        let head_refresher = Arc::new(PlayerHeadRefresher::live(
            &config,
            options.config_path.clone(),
        ));
        #[cfg(feature = "live-discord")]
        let discord_task = if options.once {
            None
        } else {
            start_discord_gateway(session.clone(), &config, head_refresher.clone()).await?
        };
        #[cfg(feature = "live-discord")]
        let discord_started = discord_task.is_some();
        #[cfg(feature = "live-discord")]
        let discord_restart_delay = env_duration_ms(
            "SAF_DISCORD_GATEWAY_RESTART_MS",
            DEFAULT_DISCORD_GATEWAY_RESTART_MS,
        );
        #[cfg(not(feature = "live-discord"))]
        let discord_started = false;

        let inbox = if options.once {
            CommandInboxCursor::new(options.command_inbox.clone())
        } else {
            CommandInboxCursor::new_at_end(options.command_inbox.clone()).await
        };

        Ok(Self {
            session,
            config,
            accounts,
            inbox,
            options,
            queue,
            stats,
            tracked_flips,
            sold_tracker: LiveSoldTracker::default(),
            minecraft_clients,
            managed_minecraft: managed_minecraft_handles,
            minecraft_ready_accounts: BTreeSet::new(),
            active_windows,
            active_window_received_at: Mutex::new(BTreeMap::new()),
            active_window_observed_at: Mutex::new(BTreeMap::new()),
            deferred_minecraft_events,
            island_states,
            cookie_prices,
            pending_auto_cookies: BTreeMap::new(),
            auction_reconcile_poller,
            bank_cooldowns,
            pending_market_steps: BTreeMap::new(),
            pending_open_auction_retries: BTreeMap::new(),
            pending_missing_listing_inventory_retries: BTreeMap::new(),
            pending_listing_price_mismatch_retries: BTreeMap::new(),
            deferred_queue_entries: Vec::new(),
            pending_purchase_relists: Vec::new(),
            pending_transfer_followups: Vec::new(),
            pending_claimed_bid_relists: Vec::new(),
            pending_completed_entries: Vec::new(),
            pending_listing_confirmations: BTreeMap::new(),
            auto_rotate_tasks,
            #[cfg(feature = "live-cofl")]
            pending_live_buys,
            #[cfg(feature = "live-cofl")]
            cofl_streams,
            #[cfg(feature = "live-discord")]
            discord_task,
            #[cfg(feature = "live-discord")]
            discord_head_refresher: head_refresher,
            #[cfg(feature = "live-discord")]
            discord_restart_at: None,
            #[cfg(feature = "live-discord")]
            discord_restart_delay,
            shutdown,
            cofl_connections,
            cofl_connected: 0,
            discord_started,
            processed_commands: 0,
            processed_cofl_envelopes: 0,
            processed_minecraft_events: 0,
            processed_queue_steps: 0,
            completed_queue_entries: 0,
        })
    }

    pub async fn run_until_shutdown(&mut self) -> Result<()> {
        let result = self.run_loop_until_shutdown().await;
        self.shutdown_runtime().await;
        result
    }

    #[cfg(feature = "live-discord")]
    pub(super) async fn poll_discord_gateway_once(&mut self) -> Result<()> {
        if self.options.once || self.shutdown.load(Ordering::SeqCst) {
            return Ok(());
        }

        if let Some(task) = self.discord_task.as_ref()
            && !task.is_finished()
        {
            return Ok(());
        }

        if let Some(task) = self.discord_task.take() {
            match task.await {
                Ok(()) => {
                    tracing::warn!("Discord gateway task exited; scheduling restart");
                }
                Err(error) => {
                    tracing::warn!(error = %error, "Discord gateway task failed; scheduling restart");
                }
            }
            self.discord_started = false;
            self.discord_restart_at = Some(Instant::now() + self.discord_restart_delay);
        }

        if let Some(restart_at) = self.discord_restart_at {
            if Instant::now() < restart_at {
                return Ok(());
            }
        } else {
            return Ok(());
        }

        match start_discord_gateway(
            self.session.clone(),
            &self.config,
            self.discord_head_refresher.clone(),
        )
        .await
        {
            Ok(Some(task)) => {
                self.discord_task = Some(task);
                self.discord_started = true;
                self.discord_restart_at = None;
                tracing::info!("Discord gateway restarted");
            }
            Ok(None) => {
                self.discord_started = false;
                self.discord_restart_at = None;
                tracing::warn!(
                    "Discord gateway restart skipped because gateway config is disabled"
                );
            }
            Err(error) => {
                self.discord_started = false;
                self.discord_restart_at = Some(Instant::now() + self.discord_restart_delay);
                tracing::warn!(error = %error, "Discord gateway restart failed; will retry");
            }
        }

        Ok(())
    }

    #[cfg(not(feature = "live-discord"))]
    pub(super) async fn poll_discord_gateway_once(&mut self) -> Result<()> {
        Ok(())
    }

    async fn run_loop_until_shutdown(&mut self) -> Result<()> {
        let mut shutdown_signal = ShutdownSignal::new()?;
        loop {
            tokio::select! {
                result = shutdown_signal.recv() => {
                    result?;
                    break;
                }
                result = self.poll_once() => {
                    result?;
                }
            }
            tokio::select! {
                result = shutdown_signal.recv() => {
                    result?;
                    break;
                }
                _ = sleep(self.options.poll_interval) => {}
            }
        }
        Ok(())
    }

    pub(super) async fn shutdown_runtime(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        #[cfg(feature = "live-discord")]
        if let Some(task) = &self.discord_task {
            task.abort();
        }
        for task in &self.auto_rotate_tasks {
            task.abort();
        }
        self.shutdown_live_clients().await;
    }

    pub(super) async fn shutdown_live_clients(&self) {
        for (account, client) in &self.managed_minecraft {
            if let Err(error) = client.disconnect().await {
                tracing::warn!(account = %account, error = %error, "failed to disconnect Minecraft client during shutdown");
            }
        }
        #[cfg(feature = "live-cofl")]
        for stream in &self.cofl_streams {
            stream.client.disconnect_for_stop().await;
        }
    }
}

fn startup_paused() -> bool {
    startup_paused_with_env(
        |name| std::env::var(name).ok(),
        |path| std::path::Path::new(path).exists(),
    )
}

fn startup_paused_with_env(
    env: impl Fn(&str) -> Option<String>,
    exists: impl Fn(&str) -> bool,
) -> bool {
    if env("SAF_START_PAUSED").is_some_and(|value| {
        let value = value.trim();
        value == "1" || value.eq_ignore_ascii_case("true")
    }) {
        return true;
    }

    let pause_file = env("SAF_PAUSE_FILE")
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| ".saf-paused".to_string());
    exists(&pause_file)
}

struct ShutdownSignal {
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
}

impl ShutdownSignal {
    fn new() -> Result<Self> {
        #[cfg(unix)]
        {
            Ok(Self {
                terminate:
                    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                        .context("installing SIGTERM handler")?,
            })
        }

        #[cfg(not(unix))]
        {
            Ok(Self {})
        }
    }

    async fn recv(&mut self) -> Result<()> {
        shutdown_signal(self).await
    }
}

async fn shutdown_signal(signal: &mut ShutdownSignal) -> Result<()> {
    #[cfg(unix)]
    {
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result.context("waiting for Ctrl-C shutdown signal")?;
            }
            _ = signal.terminate.recv() => {}
        }
        Ok(())
    }

    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .context("waiting for Ctrl-C shutdown signal")
    }
}

#[cfg(test)]
mod tests {
    use super::startup_paused_with_env;

    #[test]
    fn startup_pause_matches_node_pause_controls() {
        assert!(startup_paused_with_env(
            |name| (name == "SAF_START_PAUSED").then(|| "1".to_string()),
            |_| false,
        ));
        assert!(startup_paused_with_env(
            |name| (name == "SAF_PAUSE_FILE").then(|| "/tmp/saf-paused".to_string()),
            |path| path == "/tmp/saf-paused",
        ));
        assert!(!startup_paused_with_env(|_| None, |_| false));
    }
}
