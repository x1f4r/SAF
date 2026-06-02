use super::super::command_planning::{
    external_buy_action, tracked_list_flip_action, tracked_list_flip_error,
    unknown_terminal_command_message, validate_tracked_list_flip_price,
};
use super::*;

impl RuntimeSession {
    pub async fn execute_directive(
        &self,
        directive: RuntimeDirective,
    ) -> Result<RuntimeOutcome, RuntimeError> {
        match &directive {
            RuntimeDirective::StartAccounts { accounts } => {
                if let Some(supervisor) = &self.supervisor {
                    for account in accounts {
                        supervisor.start(account).await?;
                    }
                    self.mark_accounts_started(accounts)?;
                    Ok(RuntimeOutcome::Executed { directive })
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::StopAccounts { account } => {
                if let Some(supervisor) = &self.supervisor {
                    supervisor.stop(account.clone()).await?;
                    self.mark_accounts_stopped(account.as_ref())?;
                    Ok(RuntimeOutcome::Executed { directive })
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::TransferCoins {
                from,
                to,
                amount,
                stop_source,
            } => {
                self.queue_transfer_source_deposit(
                    from,
                    to,
                    amount,
                    *stop_source,
                    directive.clone(),
                )
                .await
            }
            RuntimeDirective::SendCoflCommand { account, command } => {
                if let Some(cofl) = self.cofl_client(account) {
                    cofl.send_command(account, command).await?;
                    Ok(RuntimeOutcome::Executed { directive })
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::SendMinecraftChat { account, message } => {
                let client = self.minecraft_client(account)?;
                client
                    .perform(MinecraftAction::Chat(message.clone()))
                    .await?;
                Ok(RuntimeOutcome::Executed { directive })
            }
            RuntimeDirective::QueueState {
                account,
                action,
                state,
                priority,
            } => {
                self.queue_action(
                    account,
                    action.clone(),
                    state.clone(),
                    *priority,
                    directive.clone(),
                )
                .await
            }
            RuntimeDirective::CheckBids { account } => {
                self.queue_action(
                    account,
                    serde_json::json!({}),
                    BotState::Custom("bids".to_string()),
                    5,
                    directive.clone(),
                )
                .await
            }
            RuntimeDirective::Bank { account, request } => {
                let action = self.bank_action(account, request).await?;
                self.queue_action(
                    account,
                    action,
                    BotState::Custom("bank".to_string()),
                    5,
                    directive.clone(),
                )
                .await
            }
            RuntimeDirective::ExternalBuy {
                account,
                auction_id,
            } => {
                let metadata = if let Some(provider) = &self.auction_metadata_provider {
                    provider.lookup(auction_id).await?
                } else {
                    None
                };
                let fetched = metadata.is_some();
                self.queue_action(
                    account,
                    external_buy_action(auction_id, metadata),
                    if fetched {
                        BotState::Custom("externalBuying".to_string())
                    } else {
                        BotState::Buying
                    },
                    5,
                    directive.clone(),
                )
                .await
            }
            RuntimeDirective::TrackedListFlip {
                account,
                auction_id,
                time_hours,
            } => {
                if let Some(provider) = self.tracked_flip_provider(account) {
                    let tracked = provider
                        .lookup(account, auction_id)
                        .await?
                        .ok_or_else(|| tracked_list_flip_error(auction_id))?;
                    validate_tracked_list_flip_price(auction_id, tracked.target_price)?;
                    self.queue_action(
                        account,
                        tracked_list_flip_action(auction_id, *time_hours, tracked),
                        BotState::ListingNoName,
                        4,
                        directive.clone(),
                    )
                    .await
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::ShowQueue { account } => {
                let queue = self.queue_store(account)?.snapshot(account).await?;
                Ok(RuntimeOutcome::QueueSnapshot {
                    account: account.clone(),
                    queue,
                })
            }
            RuntimeDirective::ClearQueue { account } => {
                let removed = self.queue_store(account)?.clear(account).await?;
                Ok(RuntimeOutcome::QueueCleared {
                    account: account.clone(),
                    removed,
                })
            }
            RuntimeDirective::ClearAllQueues => {
                let mut accounts = Vec::new();
                for account in self.all_configured_accounts()? {
                    let removed = self.queue_store(&account)?.clear(&account).await?;
                    accounts.push(QueueClearSnapshot { account, removed });
                }
                Ok(RuntimeOutcome::QueuesCleared { accounts })
            }
            RuntimeDirective::ClearData { account } => {
                let result = self
                    .saved_data_store(account)?
                    .clear_saved_data(account)
                    .await?;
                Ok(RuntimeOutcome::SavedDataCleared {
                    account: account.clone(),
                    queue_removed: result.queue_removed,
                    bid_data_cleared: result.bid_data_cleared,
                })
            }
            RuntimeDirective::BlacklistCommand { account, message } => {
                if let Some(store) = self.blacklist_store(account) {
                    let request = parse_blacklist_request(message)
                        .map_err(|error| RuntimeError::Invalid(error.to_string()))?;
                    let result = store.apply(account, request).await?;
                    Ok(RuntimeOutcome::BlacklistApplied {
                        account: account.clone(),
                        result,
                    })
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::ShowStats { account } => {
                if let Some(provider) = self.stats_provider(account) {
                    let stats = provider.stats(account).await?;
                    Ok(RuntimeOutcome::StatsSnapshot {
                        account: account.clone(),
                        stats,
                    })
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::ShowPing { account } => {
                self.request_ping_samples(account).await;
                if let Some(provider) = self.stats_provider(account) {
                    let ping = provider.ping(account).await?;
                    Ok(RuntimeOutcome::PingSnapshot {
                        account: account.clone(),
                        ping,
                    })
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::ShowProfit { account } => {
                if let Some(provider) = self.stats_provider(account) {
                    let stats = provider.stats(account).await?;
                    Ok(RuntimeOutcome::ProfitSnapshot {
                        account: account.clone(),
                        stats,
                    })
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::ShowUsers => Ok(RuntimeOutcome::UsersSnapshot {
                configured: self.runtime.selector.configured.clone(),
                running: self.running_accounts(),
                default: self.runtime.selector.default_ign.clone(),
            }),
            RuntimeDirective::ShowGlobalStats => {
                let running_accounts = self
                    .running_accounts()
                    .into_iter()
                    .filter_map(AccountId::new)
                    .collect::<Vec<_>>();
                if running_accounts.is_empty() {
                    return Ok(RuntimeOutcome::GlobalStatsSnapshot {
                        accounts: Vec::new(),
                        total_profit: 0.0,
                        bought: 0,
                        sold: 0,
                    });
                }

                let mut snapshots = Vec::new();
                for account in running_accounts {
                    if let Some(provider) = self.stats_provider(&account) {
                        snapshots.push(AccountStatsSnapshot {
                            account: account.clone(),
                            stats: provider.stats(&account).await?,
                        });
                    }
                }

                if snapshots.is_empty() {
                    Ok(RuntimeOutcome::Planned { directive })
                } else {
                    let total_profit = snapshots
                        .iter()
                        .map(|snapshot| snapshot.stats.total_profit)
                        .sum();
                    let bought = snapshots.iter().map(|snapshot| snapshot.stats.bought).sum();
                    let sold = snapshots.iter().map(|snapshot| snapshot.stats.sold).sum();
                    Ok(RuntimeOutcome::GlobalStatsSnapshot {
                        accounts: snapshots,
                        total_profit,
                        bought,
                        sold,
                    })
                }
            }
            RuntimeDirective::ShowConnections => {
                let running_accounts = self
                    .running_accounts()
                    .into_iter()
                    .filter_map(AccountId::new)
                    .collect::<Vec<_>>();
                if running_accounts.is_empty() {
                    return Ok(RuntimeOutcome::ConnectionsSnapshot {
                        connections: Vec::new(),
                    });
                }

                let mut connections = Vec::new();
                for account in running_accounts {
                    if let Some(provider) = self.connection_provider(&account) {
                        connections.push(AccountConnection {
                            account: account.clone(),
                            connection_id: provider.connection_id(&account).await?,
                        });
                    }
                }

                if connections.is_empty() {
                    Ok(RuntimeOutcome::Planned { directive })
                } else {
                    Ok(RuntimeOutcome::ConnectionsSnapshot { connections })
                }
            }
            RuntimeDirective::ShowLogs { lines } => {
                if let Some(reader) = &self.log_reader {
                    Ok(RuntimeOutcome::LogSnapshot {
                        snapshot: reader.latest(*lines).await?,
                    })
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::ShowInventory { account } => {
                if let Some(provider) = self.inventory_provider(account) {
                    Ok(RuntimeOutcome::InventorySnapshot {
                        snapshot: provider.snapshot(account).await?,
                    })
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::SellInventory {
                account,
                include_hotbar,
            } => {
                if let Some(provider) = self.inventory_provider(account) {
                    let snapshot = provider.snapshot(account).await?;
                    let queued = self
                        .queue_inventory_listings(account, &snapshot.items, *include_hotbar)
                        .await?;
                    Ok(RuntimeOutcome::InventoryListingsQueued {
                        account: account.clone(),
                        queued,
                    })
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::QueueDelistAll { account } => {
                if let Some(provider) = self.active_auction_provider(account) {
                    let auctions = provider.active_auctions(account).await?;
                    let queued = self.queue_delist_all(account, &auctions).await?;
                    Ok(RuntimeOutcome::DelistAllQueued {
                        account: account.clone(),
                        queued,
                    })
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::ScheduleAccount {
                account,
                action,
                delay_ms,
            } => {
                if let Some(scheduler) = self.account_scheduler(account) {
                    let result = scheduler
                        .schedule(AccountScheduleRequest {
                            account: account.clone(),
                            action: action.clone(),
                            delay_ms: *delay_ms,
                        })
                        .await?;
                    Ok(RuntimeOutcome::AccountScheduled { result })
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::DiagnoseSlots { account, target } => {
                if let Some(provider) = self.gui_diagnostics_provider(account) {
                    Ok(RuntimeOutcome::GuiSlotDiagnostics {
                        diagnostics: provider.diagnose_slots(account, target.as_deref()).await?,
                    })
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::TestWebhook { account } => {
                self.notify(Notification {
                    title: "SAF test".to_string(),
                    body: "Webhook notifier path is connected.".to_string(),
                    account: Some(account.clone()),
                    ..Notification::default()
                })
                .await?;
                Ok(RuntimeOutcome::Executed { directive })
            }
            RuntimeDirective::Cookie { account } => {
                if let Some(forcer) = self.cookie_forcer() {
                    forcer.force_cookie(account).await?;
                    Ok(RuntimeOutcome::Executed { directive })
                } else {
                    Ok(RuntimeOutcome::Planned { directive })
                }
            }
            RuntimeDirective::UnknownTerminalCommand {
                command, message, ..
            } => Err(RuntimeError::Invalid(unknown_terminal_command_message(
                command, message,
            ))),
        }
    }
}
