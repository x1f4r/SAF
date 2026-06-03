use crate::buy::{BuyThresholds, SkipPolicy};
use crate::command::{AccountSelector, RoutedCommand};
use crate::config::SafConfig;
use crate::gui::WindowSnapshot;
use crate::ids::AccountId;
use crate::local_command::LocalCommand;
use crate::market::{MarketStep, MarketWorkflow};
use crate::numbers::parse_compact_number;
use crate::ports::{
    AccountConnection, AccountConnectionProvider, AccountScheduleRequest, AccountScheduler,
    AccountStatsProvider, AccountSupervisor, ActiveAuction, ActiveAuctionProvider, AuctionMetadata,
    AuctionMetadataProvider, BlacklistStore, CoflClient, CookieForcer, GuiDiagnosticsProvider,
    InventoryItem, InventoryProvider, LogReader, MinecraftAction, MinecraftClient, Notification,
    Notifier, QueueStore, SavedDataStore, ScheduledAccountAction, TrackedFlip, TrackedFlipProvider,
};
use crate::time::{duration_to_hours, normal_time};
use crate::{
    BlacklistPolicy, BlacklistPolicyHandle, BotState, FlipEvent, FlipProcessor, ItemContext,
    parse_blacklist_request, parse_lore_enchantments,
};
mod types;
pub use types::{
    AccountStatsSnapshot, BankRequest, QueueClearSnapshot, RuntimeDirective, RuntimeError,
    RuntimeOutcome,
};
mod command_planning;
mod session;
mod session_registration;
use command_planning::{
    account_id, build_ask_prefixes, control_account, external_buy_flip, join_command,
    parse_log_line_count, queue_delist, queue_list_flip, queue_list_item, resolve_account,
    schedule_account, unknown_terminal_command_message,
};

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug)]
pub struct BotRuntime {
    selector: AccountSelector,
    processor: FlipProcessor,
}

impl BotRuntime {
    pub fn from_config(config: &SafConfig, running: Vec<String>) -> Self {
        let configured = config.configured_igns();
        let selector = AccountSelector {
            ask_prefixes: build_ask_prefixes(&configured),
            configured,
            running,
            default_ign: config.default_account(),
        };

        Self {
            selector,
            processor: FlipProcessor::new(
                BlacklistPolicy::from_config(&config.do_not_buy, &config.do_not_relist),
                SkipPolicy::from_config(&config.skip),
                BuyThresholds::default(),
            ),
        }
    }

    pub fn selector(&self) -> &AccountSelector {
        &self.selector
    }

    pub fn blacklist_handle(&self) -> BlacklistPolicyHandle {
        self.processor.blacklist_handle()
    }

    pub fn with_running(&self, running: Vec<String>) -> Self {
        let mut runtime = self.clone();
        runtime.selector.running = running;
        runtime
    }

    pub fn plan_local_command(
        &self,
        command: LocalCommand,
    ) -> Result<RuntimeDirective, RuntimeError> {
        match command {
            LocalCommand::Transfer {
                from,
                to,
                amount,
                stop_source,
                ..
            } => Ok(RuntimeDirective::TransferCoins {
                from: account_id(from, "transfer source")?,
                to: account_id(to, "transfer target")?,
                amount,
                stop_source,
            }),
            LocalCommand::Terminal { line, .. } => self.plan_terminal_line(&line),
        }
    }

    pub fn plan_terminal_line(&self, line: &str) -> Result<RuntimeDirective, RuntimeError> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Err(RuntimeError::Invalid("No command provided.".to_string()));
        }

        let mut parts = trimmed.split_whitespace();
        let command = parts.next().unwrap_or_default();
        let args = parts.collect::<Vec<_>>();
        match command
            .trim_start_matches('/')
            .to_ascii_lowercase()
            .as_str()
        {
            "start" | "start_bot" => {
                let requested = args.first().copied();
                let accounts = self
                    .selector
                    .resolve_start_targets(requested)
                    .into_iter()
                    .filter_map(AccountId::new)
                    .collect::<Vec<_>>();
                if accounts.is_empty() {
                    return Err(RuntimeError::Invalid(
                        "No configured account matched the start request.".to_string(),
                    ));
                }
                Ok(RuntimeDirective::StartAccounts { accounts })
            }
            "stop" | "stop_bot" => {
                let account = args
                    .first()
                    .map(|value| resolve_account(value, &self.selector))
                    .transpose()?;
                Ok(RuntimeDirective::StopAccounts { account })
            }
            "transfer" | "transfer_coins" => {
                let from = args.first().ok_or_else(|| {
                    RuntimeError::Invalid("Transfer source account is required.".to_string())
                })?;
                let to = args.get(1).ok_or_else(|| {
                    RuntimeError::Invalid("Transfer target account is required.".to_string())
                })?;
                Ok(RuntimeDirective::TransferCoins {
                    from: resolve_account(from, &self.selector)?,
                    to: resolve_account(to, &self.selector)?,
                    amount: args.get(2).copied().unwrap_or("all").to_string(),
                    stop_source: true,
                })
            }
            "timeout" | "set_timeout" => schedule_account(
                &self.selector,
                ScheduledAccountAction::Stop,
                args.first().copied(),
                args.get(1).copied(),
            ),
            "start_in" | "set_time_in" => schedule_account(
                &self.selector,
                ScheduledAccountAction::Start,
                args.first().copied(),
                args.get(1).copied(),
            ),
            "users" | "get_users" => Ok(RuntimeDirective::ShowUsers),
            "global_stats" | "get_global_stats" => Ok(RuntimeDirective::ShowGlobalStats),
            "connections" | "get_connections" => Ok(RuntimeDirective::ShowConnections),
            "logs" | "get_log" => Ok(RuntimeDirective::ShowLogs { lines: 8 }),
            "messages" | "get_messages" => Ok(RuntimeDirective::ShowLogs {
                lines: parse_log_line_count(args.first().copied(), 40)?,
            }),
            "clear_queue_all" | "clear_all_queues" => Ok(RuntimeDirective::ClearAllQueues),
            "blacklist" => Ok(RuntimeDirective::BlacklistCommand {
                account: control_account(&self.selector)?,
                message: args.join(" "),
            }),
            _ => {
                let routed =
                    RoutedCommand::parse(trimmed, &self.selector).map_err(RuntimeError::Route)?;
                self.directive_from_routed(routed)
            }
        }
    }

    pub async fn process_flip<C: MinecraftClient + ?Sized>(
        &self,
        client: &C,
        flip: FlipEvent,
    ) -> Result<RuntimeOutcome, RuntimeError> {
        let account = client.account().await;
        let outcome = self.processor.process(client, flip).await?;
        Ok(RuntimeOutcome::FlipProcessed { account, outcome })
    }

    pub async fn execute_directive(
        &self,
        directive: RuntimeDirective,
        minecraft: Option<&dyn MinecraftClient>,
        cofl: &dyn CoflClient,
    ) -> Result<RuntimeOutcome, RuntimeError> {
        match &directive {
            RuntimeDirective::SendCoflCommand { account, command } => {
                cofl.send_command(account, command).await?;
                Ok(RuntimeOutcome::Executed { directive })
            }
            RuntimeDirective::SendMinecraftChat { account, message } => {
                let client = minecraft.ok_or_else(|| {
                    RuntimeError::Port(
                        "Minecraft client is required for chat commands.".to_string(),
                    )
                })?;
                let actual = client.account().await;
                if &actual != account {
                    return Err(RuntimeError::Port(format!(
                        "Minecraft client account {actual} does not match directive account {account}."
                    )));
                }
                client
                    .perform(MinecraftAction::Chat(message.clone()))
                    .await?;
                Ok(RuntimeOutcome::Executed { directive })
            }
            _ => Ok(RuntimeOutcome::Planned { directive }),
        }
    }

    fn directive_from_routed(
        &self,
        routed: RoutedCommand,
    ) -> Result<RuntimeDirective, RuntimeError> {
        let account = account_id(routed.account.unwrap_or_default(), "routed command account")?;
        let command = routed.command.trim();
        let message = routed.message.trim();
        match command
            .trim_start_matches('/')
            .to_ascii_lowercase()
            .as_str()
        {
            "chat" => Ok(RuntimeDirective::SendMinecraftChat {
                account,
                message: message.to_string(),
            }),
            "cofl" | "saf" | "icymacro" => Ok(RuntimeDirective::SendCoflCommand {
                account,
                command: join_command("/cofl", message),
            }),
            "fc" => Ok(RuntimeDirective::SendCoflCommand {
                account,
                command: join_command("/cofl chat", message),
            }),
            "stats" | "get_stats" => Ok(RuntimeDirective::ShowStats { account }),
            "profit" | "get_profit" => Ok(RuntimeDirective::ShowProfit { account }),
            "ping" | "get_ping" => Ok(RuntimeDirective::ShowPing { account }),
            "queue" | "get_queue" => Ok(RuntimeDirective::ShowQueue { account }),
            "clear_queue" => Ok(RuntimeDirective::ClearQueue { account }),
            "cancel_queue" => {
                let index = message
                    .split_whitespace()
                    .next()
                    .ok_or_else(|| {
                        RuntimeError::Invalid("Queue entry index is required.".to_string())
                    })?
                    .parse::<usize>()
                    .map_err(|_| {
                        RuntimeError::Invalid("Queue entry index must be a number.".to_string())
                    })?;
                Ok(RuntimeDirective::CancelQueueEntry { account, index })
            }
            "clear_data" => Ok(RuntimeDirective::ClearData { account }),
            "blacklist" => Ok(RuntimeDirective::BlacklistCommand {
                account,
                message: message.to_string(),
            }),
            "bids" | "checkbids" => Ok(RuntimeDirective::CheckBids { account }),
            "bank" | "coins" => {
                let request = BankRequest::from_message(account.as_str(), message)?;
                Ok(RuntimeDirective::Bank { account, request })
            }
            "buy_flip" | "buyflip" => external_buy_flip(account, message),
            "list_flip" | "listflip" => queue_list_flip(account, message),
            "list_item" | "listitem" => queue_list_item(account, message),
            "inventory" => Ok(RuntimeDirective::ShowInventory { account }),
            "sell_inventory" => Ok(RuntimeDirective::SellInventory {
                account,
                include_hotbar: message.split_whitespace().any(|arg| {
                    matches!(
                        arg.to_ascii_lowercase().as_str(),
                        "include_hotbar" | "include-hotbar" | "hotbar"
                    )
                }),
            }),
            "delist_everything" | "delist_all" => Ok(RuntimeDirective::QueueDelistAll { account }),
            "delist" | "delist_item" => queue_delist(account, message),
            "claim_sold" | "claimsold" => Ok(RuntimeDirective::QueueState {
                account,
                action: serde_json::json!({"reason": "manual-discord"}),
                state: BotState::Custom("claimSold".to_string()),
                priority: 2,
            }),
            "reconcile" | "reconcile_auctions" => Ok(RuntimeDirective::QueueState {
                account,
                action: serde_json::json!({"reason": "manual-discord"}),
                state: BotState::Custom("reconcileAuctions".to_string()),
                priority: 2,
            }),
            "diagslots" | "slotdiag" => Ok(RuntimeDirective::DiagnoseSlots {
                account,
                target: (!message.is_empty()).then(|| message.to_string()),
            }),
            "test" | "test_webhook" => Ok(RuntimeDirective::TestWebhook { account }),
            "cookie" | "buy_cookie" | "get_cookie" => Ok(RuntimeDirective::Cookie { account }),
            _ => Err(RuntimeError::Invalid(unknown_terminal_command_message(
                command, message,
            ))),
        }
    }
}

pub struct RuntimeSession {
    runtime: BotRuntime,
    running_accounts: Mutex<Vec<String>>,
    minecraft_clients: BTreeMap<AccountId, Arc<dyn MinecraftClient>>,
    cofl_clients: BTreeMap<AccountId, Arc<dyn CoflClient>>,
    queue_stores: BTreeMap<AccountId, Arc<dyn QueueStore>>,
    saved_data_stores: BTreeMap<AccountId, Arc<dyn SavedDataStore>>,
    blacklist_stores: BTreeMap<AccountId, Arc<dyn BlacklistStore>>,
    stats_providers: BTreeMap<AccountId, Arc<dyn AccountStatsProvider>>,
    account_schedulers: BTreeMap<AccountId, Arc<dyn AccountScheduler>>,
    inventory_providers: BTreeMap<AccountId, Arc<dyn InventoryProvider>>,
    active_auction_providers: BTreeMap<AccountId, Arc<dyn ActiveAuctionProvider>>,
    auction_metadata_provider: Option<Arc<dyn AuctionMetadataProvider>>,
    fallback_tracked_flip_provider: Option<Arc<dyn TrackedFlipProvider>>,
    fallback_connection_provider: Option<Arc<dyn AccountConnectionProvider>>,
    fallback_inventory_provider: Option<Arc<dyn InventoryProvider>>,
    fallback_active_auction_provider: Option<Arc<dyn ActiveAuctionProvider>>,
    fallback_gui_diagnostics_provider: Option<Arc<dyn GuiDiagnosticsProvider>>,
    log_reader: Option<Arc<dyn LogReader>>,
    fallback_cofl: Option<Arc<dyn CoflClient>>,
    fallback_queue_store: Option<Arc<dyn QueueStore>>,
    fallback_saved_data_store: Option<Arc<dyn SavedDataStore>>,
    fallback_blacklist_store: Option<Arc<dyn BlacklistStore>>,
    fallback_stats_provider: Option<Arc<dyn AccountStatsProvider>>,
    fallback_account_scheduler: Option<Arc<dyn AccountScheduler>>,
    notifier: Option<Arc<dyn Notifier>>,
    supervisor: Option<Arc<dyn AccountSupervisor>>,
    cookie_forcer: Option<Arc<dyn CookieForcer>>,
}

#[cfg(test)]
mod tests;
