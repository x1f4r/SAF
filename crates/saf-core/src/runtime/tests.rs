use super::*;
use crate::ports::{
    AccountPing, AccountScheduleResult, AccountStats, GuiSlotDiagnostics, InventorySnapshot,
    LogSnapshot, MinecraftAction, PortError,
};
use crate::{BlacklistApplyResult, LocalCommand, MarketInstruction, QueueEntry, SafConfig};
use async_trait::async_trait;
use serde_json::json;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
struct FakeMinecraft {
    account: AccountId,
    actions: Arc<Mutex<Vec<MinecraftAction>>>,
}

#[async_trait]
impl MinecraftClient for FakeMinecraft {
    async fn account(&self) -> AccountId {
        self.account.clone()
    }

    async fn perform(&self, action: MinecraftAction) -> Result<(), PortError> {
        self.actions.lock().unwrap().push(action);
        Ok(())
    }
}

#[derive(Default)]
struct FakeCofl {
    commands: Arc<Mutex<Vec<(AccountId, String)>>>,
}

#[async_trait]
impl CoflClient for FakeCofl {
    async fn send_command(&self, account: &AccountId, command: &str) -> Result<(), PortError> {
        self.commands
            .lock()
            .unwrap()
            .push((account.clone(), command.to_string()));
        Ok(())
    }
}

#[derive(Default)]
struct FakeNotifier {
    notifications: Arc<Mutex<Vec<Notification>>>,
}

#[async_trait]
impl Notifier for FakeNotifier {
    async fn notify(&self, notification: Notification) -> Result<(), PortError> {
        self.notifications.lock().unwrap().push(notification);
        Ok(())
    }
}

type QueueStoreEntry = (AccountId, serde_json::Value, BotState, u8);

#[derive(Default)]
struct FakeQueueStore {
    entries: Arc<Mutex<Vec<QueueStoreEntry>>>,
}

#[async_trait]
impl QueueStore for FakeQueueStore {
    async fn add(
        &self,
        account: &AccountId,
        action: serde_json::Value,
        state: BotState,
        priority: u8,
    ) -> Result<bool, PortError> {
        self.entries
            .lock()
            .unwrap()
            .push((account.clone(), action, state, priority));
        Ok(true)
    }

    async fn snapshot(&self, account: &AccountId) -> Result<Vec<crate::QueueEntry>, PortError> {
        Ok(self
            .entries
            .lock()
            .unwrap()
            .iter()
            .filter(|(entry_account, _, _, _)| entry_account == account)
            .map(|(_, action, state, priority)| crate::QueueEntry {
                action: action.clone(),
                state: state.clone(),
                priority: *priority,
            })
            .collect())
    }

    async fn clear(&self, account: &AccountId) -> Result<usize, PortError> {
        let mut entries = self.entries.lock().unwrap();
        let before = entries.len();
        entries.retain(|(entry_account, _, _, _)| entry_account != account);
        Ok(before - entries.len())
    }
}

#[derive(Default)]
struct FakeSavedDataStore {
    clear_results: Arc<Mutex<Vec<(AccountId, crate::SavedDataClear)>>>,
}

#[async_trait]
impl SavedDataStore for FakeSavedDataStore {
    async fn clear_saved_data(
        &self,
        account: &AccountId,
    ) -> Result<crate::SavedDataClear, PortError> {
        let result = crate::SavedDataClear {
            queue_removed: 2,
            bid_data_cleared: true,
        };
        self.clear_results
            .lock()
            .unwrap()
            .push((account.clone(), result.clone()));
        Ok(result)
    }
}

#[derive(Default)]
struct FakeBlacklistStore {
    requests: Arc<Mutex<Vec<(AccountId, crate::BlacklistRequest)>>>,
}

#[async_trait]
impl BlacklistStore for FakeBlacklistStore {
    async fn apply(
        &self,
        account: &AccountId,
        request: crate::BlacklistRequest,
    ) -> Result<BlacklistApplyResult, PortError> {
        self.requests
            .lock()
            .unwrap()
            .push((account.clone(), request.clone()));
        Ok(BlacklistApplyResult {
            request,
            changed: true,
            summary: "recorded blacklist request".to_string(),
        })
    }
}

#[derive(Default)]
struct FakeStatsProvider {
    requests: Arc<Mutex<Vec<(AccountId, &'static str)>>>,
}

#[async_trait]
impl AccountStatsProvider for FakeStatsProvider {
    async fn stats(&self, account: &AccountId) -> Result<AccountStats, PortError> {
        self.requests
            .lock()
            .unwrap()
            .push((account.clone(), "stats"));
        Ok(AccountStats {
            bought: 3,
            sold: 2,
            total_profit: 42_000_000.0,
            user_finder_flips: 1,
            profit_per_hour: Some(84_000_000.0),
            purse: Some(10_000_000.0),
            started_at_ms: Some(1_700_000_000_000),
            cofl_delay_ms: Some(20),
            cofl_ping_ms: Some(30),
            cofl_tier: Some("Premium Plus".to_string()),
            cofl_expires_at: Some(1_792_025_640),
            cookie_expires_at: Some(1_792_000_000),
            hypixel_ping_ms: Some(40),
            auction_slots_used: Some(3),
            auction_slots_max: Some(14),
        })
    }

    async fn ping(&self, account: &AccountId) -> Result<AccountPing, PortError> {
        self.requests
            .lock()
            .unwrap()
            .push((account.clone(), "ping"));
        Ok(AccountPing {
            cofl_delay_ms: Some(20),
            cofl_ping_ms: Some(30),
            hypixel_ping_ms: Some(40),
        })
    }
}

#[derive(Default)]
struct FakeAccountScheduler {
    requests: Arc<Mutex<Vec<AccountScheduleRequest>>>,
}

#[async_trait]
impl AccountScheduler for FakeAccountScheduler {
    async fn schedule(
        &self,
        request: AccountScheduleRequest,
    ) -> Result<AccountScheduleResult, PortError> {
        self.requests.lock().unwrap().push(request.clone());
        Ok(AccountScheduleResult {
            account: request.account,
            action: request.action,
            delay_ms: request.delay_ms,
        })
    }
}

#[derive(Default)]
struct FakeAuctionMetadataProvider {
    requests: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl AuctionMetadataProvider for FakeAuctionMetadataProvider {
    async fn lookup(&self, auction_id: &str) -> Result<Option<AuctionMetadata>, PortError> {
        self.requests.lock().unwrap().push(auction_id.to_string());
        Ok(Some(AuctionMetadata {
            auction_id: auction_id.to_string(),
            item_name: Some("Hyperion".to_string()),
            starting_bid: Some(30_000_000.0),
            tag: Some("HYPERION".to_string()),
        }))
    }
}

#[derive(Default)]
struct FakeTrackedFlipProvider {
    requests: Arc<Mutex<Vec<(AccountId, String)>>>,
}

#[async_trait]
impl TrackedFlipProvider for FakeTrackedFlipProvider {
    async fn lookup(
        &self,
        account: &AccountId,
        auction_id: &str,
    ) -> Result<Option<TrackedFlip>, PortError> {
        self.requests
            .lock()
            .unwrap()
            .push((account.clone(), auction_id.to_string()));
        Ok(Some(TrackedFlip {
            auction_id: auction_id.to_string(),
            target_price: 56_300_000.0,
            weird_item_name: Some("Ancient Necron's Leggings".to_string()),
            tag: Some("NECRON_LEGGINGS".to_string()),
            price_paid: Some(31_000_000.0),
            finder: Some("USER".to_string()),
            volume: None,
            profit_percentage: None,
            buy_kind: None,
            seen_at_ms: None,
        }))
    }
}

#[derive(Default)]
struct FakeConnectionProvider {
    requests: Arc<Mutex<Vec<AccountId>>>,
}

#[async_trait]
impl AccountConnectionProvider for FakeConnectionProvider {
    async fn connection_id(&self, account: &AccountId) -> Result<Option<String>, PortError> {
        self.requests.lock().unwrap().push(account.clone());
        Ok(Some(format!("connection-{account}")))
    }
}

#[derive(Default)]
struct FakeLogReader {
    requests: Arc<Mutex<Vec<usize>>>,
}

#[async_trait]
impl LogReader for FakeLogReader {
    async fn latest(&self, lines: usize) -> Result<LogSnapshot, PortError> {
        self.requests.lock().unwrap().push(lines);
        Ok(LogSnapshot {
            path: "logs/latest.log".to_string(),
            exists: true,
            lines: vec!["one".to_string(), "two".to_string()],
        })
    }
}

#[derive(Default)]
struct FakeInventoryProvider {
    requests: Arc<Mutex<Vec<AccountId>>>,
}

#[async_trait]
impl InventoryProvider for FakeInventoryProvider {
    async fn snapshot(&self, account: &AccountId) -> Result<InventorySnapshot, PortError> {
        self.requests.lock().unwrap().push(account.clone());
        Ok(InventorySnapshot {
            account: account.clone(),
            items: vec![
                InventoryItem {
                    uuid: Some("main-item".to_string()),
                    item_name: "Aspect of the Dragons".to_string(),
                    lore: vec!["Legendary Sword".to_string()],
                    price: Some(5_000_000.0),
                    tag: Some("ASPECT_OF_THE_DRAGON".to_string()),
                    slot: Some(10),
                    in_hotbar: false,
                },
                InventoryItem {
                    uuid: Some("hotbar-item".to_string()),
                    item_name: "Grappling Hook".to_string(),
                    lore: Vec::new(),
                    price: Some(25_000.0),
                    tag: Some("GRAPPLING_HOOK".to_string()),
                    slot: Some(36),
                    in_hotbar: true,
                },
                InventoryItem {
                    uuid: None,
                    item_name: "Untracked Item".to_string(),
                    lore: Vec::new(),
                    price: Some(10_000.0),
                    tag: None,
                    slot: Some(12),
                    in_hotbar: false,
                },
            ],
        })
    }
}

#[derive(Default)]
struct FixedItemsInventoryProvider {
    items: Vec<InventoryItem>,
}

#[async_trait]
impl InventoryProvider for FixedItemsInventoryProvider {
    async fn snapshot(&self, account: &AccountId) -> Result<InventorySnapshot, PortError> {
        Ok(InventorySnapshot {
            account: account.clone(),
            items: self.items.clone(),
        })
    }
}

#[derive(Default)]
struct FakeActiveAuctionProvider {
    requests: Arc<Mutex<Vec<AccountId>>>,
}

#[async_trait]
impl ActiveAuctionProvider for FakeActiveAuctionProvider {
    async fn active_auctions(&self, account: &AccountId) -> Result<Vec<ActiveAuction>, PortError> {
        self.requests.lock().unwrap().push(account.clone());
        Ok(vec![
            ActiveAuction {
                auction_id: "auction-1".to_string(),
                item_uuid: "item-1".to_string(),
                name: Some("Necron's Handle".to_string()),
            },
            ActiveAuction {
                auction_id: String::new(),
                item_uuid: "missing-auction".to_string(),
                name: None,
            },
        ])
    }
}

type GuiDiagnosticsRequest = (AccountId, Option<String>);

#[derive(Default)]
struct FakeGuiDiagnosticsProvider {
    requests: Arc<Mutex<Vec<GuiDiagnosticsRequest>>>,
}

#[async_trait]
impl GuiDiagnosticsProvider for FakeGuiDiagnosticsProvider {
    async fn diagnose_slots(
        &self,
        account: &AccountId,
        target: Option<&str>,
    ) -> Result<GuiSlotDiagnostics, PortError> {
        self.requests
            .lock()
            .unwrap()
            .push((account.clone(), target.map(ToString::to_string)));
        Ok(GuiSlotDiagnostics {
            account: account.clone(),
            target: target.map(ToString::to_string),
            windows: vec![WindowSnapshot {
                title: "Bank".to_string(),
                slots: vec![crate::gui::WindowSlot {
                    slot: 11,
                    name: "gold_ingot".to_string(),
                    display_name: "Shared Account".to_string(),
                    lore: Vec::new(),
                    item_uuid: None,
                }],
            }],
        })
    }
}

#[derive(Default)]
struct FakeSupervisor {
    actions: Arc<Mutex<Vec<SupervisorAction>>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum SupervisorAction {
    Start(AccountId),
    Stop(Option<AccountId>),
}

#[async_trait]
impl AccountSupervisor for FakeSupervisor {
    async fn start(&self, account: &AccountId) -> Result<(), PortError> {
        self.actions
            .lock()
            .unwrap()
            .push(SupervisorAction::Start(account.clone()));
        Ok(())
    }

    async fn stop(&self, account: Option<AccountId>) -> Result<(), PortError> {
        self.actions
            .lock()
            .unwrap()
            .push(SupervisorAction::Stop(account));
        Ok(())
    }
}

fn runtime() -> BotRuntime {
    let config = SafConfig {
        igns: vec!["MainAccount".to_string(), "MainAlt".to_string()],
        default_ign: "MainAccount".to_string(),
        ..SafConfig::default()
    };
    BotRuntime::from_config(
        &config,
        vec!["MainAccount".to_string(), "MainAlt".to_string()],
    )
}

#[test]
fn plans_cofl_terminal_commands_against_default_account() {
    let directive = runtime()
        .plan_terminal_line("/cofl s minProfit 30m")
        .unwrap();

    assert_eq!(
        directive,
        RuntimeDirective::SendCoflCommand {
            account: AccountId::new("MainAccount").unwrap(),
            command: "/cofl s minProfit 30m".to_string()
        }
    );
}

#[test]
fn plans_account_prefixes_without_collisions() {
    let directive = runtime()
        .plan_terminal_line("mainal /fc hello")
        .expect("unique generated prefix should route to MainAlt");

    assert_eq!(
        directive,
        RuntimeDirective::SendCoflCommand {
            account: AccountId::new("MainAlt").unwrap(),
            command: "/cofl chat hello".to_string()
        }
    );
}

#[test]
fn plans_start_command_against_default_account() {
    assert_eq!(
        runtime().plan_terminal_line("start").unwrap(),
        RuntimeDirective::StartAccounts {
            accounts: vec![AccountId::new("MainAccount").unwrap()],
        }
    );
    assert_eq!(
        runtime().plan_terminal_line("start MainAlt").unwrap(),
        RuntimeDirective::StartAccounts {
            accounts: vec![AccountId::new("MainAlt").unwrap()],
        }
    );
}

#[test]
fn plans_local_transfer_commands() {
    let directive = runtime()
        .plan_local_command(LocalCommand::Transfer {
            from: "MainAccount".to_string(),
            to: "MainAlt".to_string(),
            amount: "all".to_string(),
            stop_source: true,
            created_at: Some(1),
        })
        .unwrap();

    assert_eq!(
        directive,
        RuntimeDirective::TransferCoins {
            from: AccountId::new("MainAccount").unwrap(),
            to: AccountId::new("MainAlt").unwrap(),
            amount: "all".to_string(),
            stop_source: true
        }
    );
}

#[test]
fn plans_discord_style_queue_commands() {
    assert_eq!(
        runtime().plan_terminal_line("get_stats").unwrap(),
        RuntimeDirective::ShowStats {
            account: AccountId::new("MainAccount").unwrap(),
        }
    );
    assert_eq!(
        runtime().plan_terminal_line("get_ping").unwrap(),
        RuntimeDirective::ShowPing {
            account: AccountId::new("MainAccount").unwrap(),
        }
    );
    assert_eq!(
        runtime().plan_terminal_line("profit").unwrap(),
        RuntimeDirective::ShowProfit {
            account: AccountId::new("MainAccount").unwrap(),
        }
    );
    assert_eq!(
        runtime().plan_terminal_line("get_profit").unwrap(),
        RuntimeDirective::ShowProfit {
            account: AccountId::new("MainAccount").unwrap(),
        }
    );
    assert_eq!(
        runtime().plan_terminal_line("users").unwrap(),
        RuntimeDirective::ShowUsers
    );
    assert_eq!(
        runtime().plan_terminal_line("global_stats").unwrap(),
        RuntimeDirective::ShowGlobalStats
    );
    assert_eq!(
        runtime().plan_terminal_line("connections").unwrap(),
        RuntimeDirective::ShowConnections
    );
    assert_eq!(
        runtime().plan_terminal_line("logs").unwrap(),
        RuntimeDirective::ShowLogs { lines: 8 }
    );
    assert_eq!(
        runtime().plan_terminal_line("messages 200").unwrap(),
        RuntimeDirective::ShowLogs { lines: 80 }
    );
    assert_eq!(
        runtime().plan_terminal_line("clear_queue_all").unwrap(),
        RuntimeDirective::ClearAllQueues
    );
    assert_eq!(
        runtime().plan_terminal_line("blacklist list").unwrap(),
        RuntimeDirective::BlacklistCommand {
            account: AccountId::new("MainAccount").unwrap(),
            message: "list".to_string(),
        }
    );
    assert_eq!(
        runtime().plan_terminal_line("inventory").unwrap(),
        RuntimeDirective::ShowInventory {
            account: AccountId::new("MainAccount").unwrap(),
        }
    );
    assert_eq!(
        runtime()
            .plan_terminal_line("sell_inventory hotbar")
            .unwrap(),
        RuntimeDirective::SellInventory {
            account: AccountId::new("MainAccount").unwrap(),
            include_hotbar: true,
        }
    );
    assert_eq!(
        runtime().plan_terminal_line("delist_everything").unwrap(),
        RuntimeDirective::QueueDelistAll {
            account: AccountId::new("MainAccount").unwrap(),
        }
    );
    assert_eq!(
        runtime().plan_terminal_line("delist_all").unwrap(),
        RuntimeDirective::QueueDelistAll {
            account: AccountId::new("MainAccount").unwrap(),
        }
    );
    assert_eq!(
        runtime().plan_terminal_line("claim_sold").unwrap(),
        RuntimeDirective::QueueState {
            account: AccountId::new("MainAccount").unwrap(),
            action: json!({"reason": "manual-discord"}),
            state: BotState::Custom("claimSold".to_string()),
            priority: 2,
        }
    );
    assert_eq!(
        runtime().plan_terminal_line("buy_flip auction-1").unwrap(),
        RuntimeDirective::ExternalBuy {
            account: AccountId::new("MainAccount").unwrap(),
            auction_id: "auction-1".to_string(),
        }
    );
    assert_eq!(
        runtime()
            .plan_terminal_line("list_flip auction-1 25m 12h")
            .unwrap(),
        RuntimeDirective::QueueState {
            account: AccountId::new("MainAccount").unwrap(),
            action: json!({
                "auctionID": "auction-1",
                "price": 25_000_000.0,
                "time": 12.0,
                "weirdItemName": "auction-1",
                "pricePaid": 0
            }),
            state: BotState::ListingNoName,
            priority: 4,
        }
    );
    assert_eq!(
        runtime()
            .plan_terminal_line("list_flip auction-1 --time 12h")
            .unwrap(),
        RuntimeDirective::TrackedListFlip {
            account: AccountId::new("MainAccount").unwrap(),
            auction_id: "auction-1".to_string(),
            time_hours: 12.0,
        }
    );
    assert_eq!(
        runtime()
            .plan_terminal_line("delist auction-1 item-uuid")
            .unwrap(),
        RuntimeDirective::QueueState {
            account: AccountId::new("MainAccount").unwrap(),
            action: json!({
                "auctionID": "auction-1",
                "itemUuid": "item-uuid"
            }),
            state: BotState::Delisting,
            priority: 3,
        }
    );
    assert_eq!(
        runtime().plan_terminal_line("test_webhook").unwrap(),
        RuntimeDirective::TestWebhook {
            account: AccountId::new("MainAccount").unwrap(),
        }
    );
}

#[test]
fn plans_blacklist_without_running_accounts() {
    let config = SafConfig {
        igns: vec!["MainAccount".to_string(), "MainAlt".to_string()],
        default_ign: "MainAlt".to_string(),
        ..SafConfig::default()
    };
    let runtime = BotRuntime::from_config(&config, Vec::new());

    assert_eq!(
        runtime
            .plan_terminal_line("blacklist add buy tag SPEED_RELIC --duration 7d")
            .unwrap(),
        RuntimeDirective::BlacklistCommand {
            account: AccountId::new("MainAlt").unwrap(),
            message: "add buy tag SPEED_RELIC --duration 7d".to_string(),
        }
    );
}

#[test]
fn plans_timed_account_controls() {
    assert_eq!(
        runtime().plan_terminal_line("timeout 30m MainAlt").unwrap(),
        RuntimeDirective::ScheduleAccount {
            account: AccountId::new("MainAlt").unwrap(),
            action: ScheduledAccountAction::Stop,
            delay_ms: 1_800_000,
        }
    );
    assert_eq!(
        runtime().plan_terminal_line("start_in 1h").unwrap(),
        RuntimeDirective::ScheduleAccount {
            account: AccountId::new("MainAccount").unwrap(),
            action: ScheduledAccountAction::Start,
            delay_ms: 3_600_000,
        }
    );
}

#[test]
fn rejects_unknown_terminal_commands_during_planning() {
    let error = runtime()
        .plan_terminal_line("wat even is this")
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "Unknown terminal command: wat even is this"
    );
}

#[test]
fn rejects_invalid_bank_commands_during_planning() {
    for line in ["bank", "bank nope", "bank all withdraw"] {
        let error = runtime().plan_terminal_line(line).unwrap_err();
        assert!(
            error.to_string().contains("Usage: bank <amount|all>"),
            "{line} should report bank usage, got {error}"
        );
    }
}

#[tokio::test]
async fn executes_port_backed_directives() {
    let runtime = runtime();
    let cofl = FakeCofl::default();
    let minecraft = FakeMinecraft {
        account: AccountId::new("MainAccount").unwrap(),
        actions: Arc::new(Mutex::new(Vec::new())),
    };

    let cofl_directive = RuntimeDirective::SendCoflCommand {
        account: AccountId::new("MainAccount").unwrap(),
        command: "/cofl ping".to_string(),
    };
    runtime
        .execute_directive(
            cofl_directive,
            Some(&minecraft as &dyn MinecraftClient),
            &cofl,
        )
        .await
        .unwrap();

    let chat_directive = RuntimeDirective::SendMinecraftChat {
        account: AccountId::new("MainAccount").unwrap(),
        message: "/is".to_string(),
    };
    runtime
        .execute_directive(
            chat_directive,
            Some(&minecraft as &dyn MinecraftClient),
            &cofl,
        )
        .await
        .unwrap();

    assert_eq!(
        cofl.commands.lock().unwrap().as_slice(),
        &[(
            AccountId::new("MainAccount").unwrap(),
            "/cofl ping".to_string()
        )]
    );
    assert_eq!(
        minecraft.actions.lock().unwrap().as_slice(),
        &[MinecraftAction::Chat("/is".to_string())]
    );
}

#[tokio::test]
async fn processes_flip_for_runtime_account() {
    let runtime = runtime();
    let actions = Arc::new(Mutex::new(Vec::new()));
    let minecraft = FakeMinecraft {
        account: AccountId::new("MainAccount").unwrap(),
        actions: actions.clone(),
    };

    let outcome = runtime
        .process_flip(
            &minecraft,
            FlipEvent::from_payload(&json!({
                "id": "auction-1",
                "itemName": "Hyperion",
                "startingBid": "30m",
                "target": "50m"
            })),
        )
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::FlipProcessed { account, .. } if account == AccountId::new("MainAccount").unwrap()
    ));
    assert_eq!(actions.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn session_executes_planned_directives_through_registered_ports() {
    let mut session = RuntimeSession::new(runtime());
    let minecraft = Arc::new(FakeMinecraft {
        account: AccountId::new("MainAccount").unwrap(),
        actions: Arc::new(Mutex::new(Vec::new())),
    });
    let cofl = Arc::new(FakeCofl::default());
    session
        .add_minecraft_client(AccountId::new("MainAccount").unwrap(), minecraft.clone())
        .add_cofl_client(AccountId::new("MainAccount").unwrap(), cofl.clone());

    session
        .process_local_command(LocalCommand::Terminal {
            line: "/cofl ping".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    session
        .process_local_command(LocalCommand::Terminal {
            line: "chat /is".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert_eq!(
        cofl.commands.lock().unwrap().as_slice(),
        &[(
            AccountId::new("MainAccount").unwrap(),
            "/cofl ping".to_string()
        )]
    );
    assert_eq!(
        minecraft.actions.lock().unwrap().as_slice(),
        &[MinecraftAction::Chat("/is".to_string())]
    );
}

#[tokio::test]
async fn session_reports_cofl_commands_as_planned_without_live_client() {
    let session = RuntimeSession::new(runtime());

    let outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "/cofl ping".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::Planned {
            directive: RuntimeDirective::SendCoflCommand { .. }
        }
    ));
}

#[tokio::test]
async fn session_executes_market_instructions_through_minecraft_port() {
    let mut session = RuntimeSession::new(runtime());
    let actions = Arc::new(Mutex::new(Vec::new()));
    let minecraft = Arc::new(FakeMinecraft {
        account: AccountId::new("MainAccount").unwrap(),
        actions: actions.clone(),
    });
    session.add_minecraft_client(AccountId::new("MainAccount").unwrap(), minecraft);

    session
        .execute_market_instruction(
            &AccountId::new("MainAccount").unwrap(),
            &MarketInstruction::TypeText {
                text: "12345678".to_string(),
            },
        )
        .await
        .unwrap();

    assert_eq!(
        actions.lock().unwrap().as_slice(),
        &[MinecraftAction::TypeText("12345678".to_string())]
    );
}

#[tokio::test]
async fn session_executes_click_then_type_market_instruction_in_order() {
    let mut session = RuntimeSession::new(runtime());
    let actions = Arc::new(Mutex::new(Vec::new()));
    let minecraft = Arc::new(FakeMinecraft {
        account: AccountId::new("MainAccount").unwrap(),
        actions: actions.clone(),
    });
    session.add_minecraft_client(AccountId::new("MainAccount").unwrap(), minecraft);

    session
        .execute_market_instruction(
            &AccountId::new("MainAccount").unwrap(),
            &MarketInstruction::ClickSlotThenType {
                slot: 15,
                text: "\"50000000\"".to_string(),
            },
        )
        .await
        .unwrap();

    assert_eq!(
        actions.lock().unwrap().as_slice(),
        &[
            MinecraftAction::ClickSlot(15),
            MinecraftAction::TypeText("\"50000000\"".to_string())
        ]
    );
}

#[tokio::test]
async fn session_processes_flips_and_notifications_through_registered_ports() {
    let mut session = RuntimeSession::new(runtime());
    let minecraft = Arc::new(FakeMinecraft {
        account: AccountId::new("MainAccount").unwrap(),
        actions: Arc::new(Mutex::new(Vec::new())),
    });
    let notifier = Arc::new(FakeNotifier::default());
    session
        .add_minecraft_client(AccountId::new("MainAccount").unwrap(), minecraft.clone())
        .set_notifier(notifier.clone());

    let outcome = session
        .process_flip(
            &AccountId::new("MainAccount").unwrap(),
            FlipEvent::from_payload(&json!({
                "id": "auction-1",
                "itemName": "Hyperion",
                "startingBid": "30m",
                "target": "50m"
            })),
        )
        .await
        .unwrap();
    session
        .notify(Notification {
            title: "Bought".to_string(),
            body: "Hyperion".to_string(),
            account: Some(AccountId::new("MainAccount").unwrap()),
            ..Notification::default()
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::FlipProcessed { account, .. } if account == AccountId::new("MainAccount").unwrap()
    ));
    assert_eq!(minecraft.actions.lock().unwrap().len(), 1);
    assert_eq!(notifier.notifications.lock().unwrap().len(), 1);
}

#[tokio::test]
async fn session_executes_test_webhook_through_notifier() {
    let mut session = RuntimeSession::new(runtime());
    let notifier = Arc::new(FakeNotifier::default());
    session.set_notifier(notifier.clone());

    let outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "test".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::Executed {
            directive: RuntimeDirective::TestWebhook { .. }
        }
    ));
    assert_eq!(
        notifier.notifications.lock().unwrap().as_slice(),
        &[Notification {
            kind: crate::ports::NotificationKind::Info,
            title: "SAF test".to_string(),
            body: "Webhook notifier path is connected.".to_string(),
            account: Some(AccountId::new("MainAccount").unwrap()),
            fields: Vec::new(),
            thumbnail_url: None,
        }]
    );
}

#[tokio::test]
async fn session_executes_gui_slot_diagnostics_through_provider() {
    let mut session = RuntimeSession::new(runtime());
    let diagnostics = Arc::new(FakeGuiDiagnosticsProvider::default());
    session.set_fallback_gui_diagnostics_provider(diagnostics.clone());

    let outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "diagslots bank".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::GuiSlotDiagnostics {
            diagnostics: GuiSlotDiagnostics { ref windows, .. }
        } if windows[0].title == "Bank"
    ));
    assert_eq!(
        diagnostics.requests.lock().unwrap().as_slice(),
        &[(
            AccountId::new("MainAccount").unwrap(),
            Some("bank".to_string())
        )]
    );
}

#[tokio::test]
async fn session_rejects_unknown_terminal_commands() {
    let session = RuntimeSession::new(runtime());

    let error = session
        .process_local_command(LocalCommand::Terminal {
            line: "wat even is this".to_string(),
            created_at: None,
        })
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "Unknown terminal command: wat even is this"
    );
}

#[tokio::test]
async fn session_rejects_serialized_unknown_terminal_directives() {
    let session = RuntimeSession::new(runtime());

    let error = session
        .execute_directive(RuntimeDirective::UnknownTerminalCommand {
            account: AccountId::new("MainAccount").unwrap(),
            command: "wat".to_string(),
            message: "even is this".to_string(),
        })
        .await
        .unwrap_err();

    assert_eq!(
        error.to_string(),
        "Unknown terminal command: wat even is this"
    );
}

#[tokio::test]
async fn session_queues_state_backed_directives_through_registered_ports() {
    let mut session = RuntimeSession::new(runtime());
    let queue = Arc::new(FakeQueueStore::default());
    session.add_queue_store(AccountId::new("MainAccount").unwrap(), queue.clone());

    let bids = session
        .process_local_command(LocalCommand::Terminal {
            line: "checkbids".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    let bank = session
        .process_local_command(LocalCommand::Terminal {
            line: "bank 50m withdraw personal".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    let buy = session
        .process_local_command(LocalCommand::Terminal {
            line: "buy_flip auction-1".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        bids,
        RuntimeOutcome::Queued {
            changed: true,
            directive: RuntimeDirective::CheckBids { .. }
        }
    ));
    assert!(matches!(
        bank,
        RuntimeOutcome::Queued {
            changed: true,
            directive: RuntimeDirective::Bank { .. }
        }
    ));
    assert!(matches!(
        buy,
        RuntimeOutcome::Queued {
            changed: true,
            directive: RuntimeDirective::ExternalBuy { .. }
        }
    ));
    assert_eq!(
        queue.entries.lock().unwrap().as_slice(),
        &[
            (
                AccountId::new("MainAccount").unwrap(),
                json!({}),
                BotState::Custom("bids".to_string()),
                5
            ),
            (
                AccountId::new("MainAccount").unwrap(),
                json!({
                    "amount": 50_000_000.0,
                    "withdraw": true,
                    "personal": true
                }),
                BotState::Custom("bank".to_string()),
                5
            ),
            (
                AccountId::new("MainAccount").unwrap(),
                json!({
                    "finder": "EXTERNAL",
                    "profit": 0,
                    "itemName": "auction-1",
                    "auctionID": "auction-1"
                }),
                BotState::Buying,
                5
            )
        ]
    );

    let snapshot = session
        .process_local_command(LocalCommand::Terminal {
            line: "queue".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    let cleared = session
        .process_local_command(LocalCommand::Terminal {
            line: "clear_queue".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        snapshot,
        RuntimeOutcome::QueueSnapshot { ref queue, .. } if queue.len() == 3
    ));
    assert!(matches!(
        cleared,
        RuntimeOutcome::QueueCleared { removed: 3, .. }
    ));
    assert!(queue.entries.lock().unwrap().is_empty());
}

#[tokio::test]
async fn session_clears_all_configured_queues_for_discord_parity() {
    let mut session = RuntimeSession::new(runtime());
    let queue = Arc::new(FakeQueueStore::default());
    session.set_fallback_queue_store(queue.clone());
    queue.entries.lock().unwrap().extend([
        (
            AccountId::new("MainAccount").unwrap(),
            json!({"auctionID": "main-auction"}),
            BotState::Buying,
            5,
        ),
        (
            AccountId::new("MainAlt").unwrap(),
            json!({"auctionID": "alt-auction"}),
            BotState::Delisting,
            3,
        ),
    ]);

    let cleared = session
        .process_local_command(LocalCommand::Terminal {
            line: "clear_queue_all".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert_eq!(
        cleared,
        RuntimeOutcome::QueuesCleared {
            accounts: vec![
                QueueClearSnapshot {
                    account: AccountId::new("MainAccount").unwrap(),
                    removed: 1
                },
                QueueClearSnapshot {
                    account: AccountId::new("MainAlt").unwrap(),
                    removed: 1
                },
            ]
        }
    );
    assert!(queue.entries.lock().unwrap().is_empty());
}

#[tokio::test]
async fn session_queues_transfer_source_deposit_with_resolved_amount() {
    let queue = Arc::new(FakeQueueStore::default());
    let stats = Arc::new(FakeStatsProvider::default());
    let supervisor = Arc::new(FakeSupervisor::default());
    let mut session = RuntimeSession::new(runtime());
    session
        .set_fallback_queue_store(queue.clone())
        .set_fallback_stats_provider(stats)
        .set_account_supervisor(supervisor.clone());
    let from = AccountId::new("MainAccount").unwrap();
    let to = AccountId::new("MainAlt").unwrap();

    let outcome = session
        .execute_directive(RuntimeDirective::TransferCoins {
            from: from.clone(),
            to: to.clone(),
            amount: "all".to_string(),
            stop_source: true,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::Queued { changed: true, .. }
    ));
    let entries = queue.entries.lock().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, from);
    assert_eq!(entries[0].2, BotState::Custom("bank".to_string()));
    assert_eq!(entries[0].3, 5);
    assert_eq!(entries[0].1["amount"], json!(10_000_000));
    assert_eq!(entries[0].1["withdraw"], json!(false));
    assert_eq!(entries[0].1["personal"], json!(false));
    assert_eq!(entries[0].1["transfer"]["to"], json!("MainAlt"));
    assert_eq!(entries[0].1["transfer"]["stopSource"], json!(true));
    assert_eq!(
        supervisor.actions.lock().unwrap().as_slice(),
        &[SupervisorAction::Start(
            AccountId::new("MainAccount").unwrap()
        )]
    );
}

#[tokio::test]
async fn session_resolves_bank_all_deposit_from_live_purse() {
    let queue = Arc::new(FakeQueueStore::default());
    let stats = Arc::new(FakeStatsProvider::default());
    let mut session = RuntimeSession::new(runtime());
    session
        .set_fallback_queue_store(queue.clone())
        .set_fallback_stats_provider(stats);

    let outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "bank all".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::Queued {
            changed: true,
            directive: RuntimeDirective::Bank { .. }
        }
    ));
    let entries = queue.entries.lock().unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].0, AccountId::new("MainAccount").unwrap());
    assert_eq!(entries[0].1["amount"], json!(10_000_000));
    assert_eq!(entries[0].1["withdraw"], json!(false));
    assert_eq!(entries[0].1["personal"], json!(false));
    assert_eq!(entries[0].2, BotState::Custom("bank".to_string()));
}

#[test]
fn session_plans_market_steps_from_queue_entries() {
    let session = RuntimeSession::new(runtime());
    let entry = QueueEntry {
        action: json!({"auctionID": "auction-1"}),
        state: BotState::Buying,
        priority: 5,
    };

    let step = session.plan_market_queue_entry(&entry, None).unwrap();

    assert_eq!(
        step.instruction,
        MarketInstruction::OpenAuction {
            auction_id: crate::AuctionId::new("auction-1").unwrap()
        }
    );
}

#[tokio::test]
async fn external_buy_can_use_auction_metadata_provider() {
    let mut session = RuntimeSession::new(runtime());
    let queue = Arc::new(FakeQueueStore::default());
    let metadata = Arc::new(FakeAuctionMetadataProvider::default());
    session
        .add_queue_store(AccountId::new("MainAccount").unwrap(), queue.clone())
        .set_auction_metadata_provider(metadata.clone());

    let outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "buy_flip auction-1".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::Queued {
            directive: RuntimeDirective::ExternalBuy { .. },
            changed: true,
        }
    ));
    assert_eq!(
        metadata.requests.lock().unwrap().as_slice(),
        &["auction-1".to_string()]
    );
    assert_eq!(
        queue.entries.lock().unwrap().as_slice(),
        &[(
            AccountId::new("MainAccount").unwrap(),
            json!({
                "finder": "EXTERNAL",
                "profit": 0,
                "itemName": "Hyperion",
                "auctionID": "auction-1",
                "startingBid": 30_000_000.0,
                "tag": "HYPERION"
            }),
            BotState::Custom("externalBuying".to_string()),
            5
        )]
    );
}

#[tokio::test]
async fn tracked_list_flip_can_use_target_provider() {
    let mut session = RuntimeSession::new(runtime());
    let queue = Arc::new(FakeQueueStore::default());
    let tracked = Arc::new(FakeTrackedFlipProvider::default());
    session
        .add_queue_store(AccountId::new("MainAccount").unwrap(), queue.clone())
        .set_fallback_tracked_flip_provider(tracked.clone());

    let outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "list_flip auction-1 --time 12h".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::Queued {
            directive: RuntimeDirective::TrackedListFlip { .. },
            changed: true,
        }
    ));
    assert_eq!(
        tracked.requests.lock().unwrap().as_slice(),
        &[(
            AccountId::new("MainAccount").unwrap(),
            "auction-1".to_string()
        )]
    );
    assert_eq!(
        queue.entries.lock().unwrap().as_slice(),
        &[(
            AccountId::new("MainAccount").unwrap(),
            json!({
                "auctionID": "auction-1",
                "price": 56_300_000.0,
                "time": 12.0,
                "weirdItemName": "Ancient Necron's Leggings",
                "itemName": "Ancient Necron's Leggings",
                "inv": "NECRON_LEGGINGS",
                "inventory": "NECRON_LEGGINGS",
                "tag": "NECRON_LEGGINGS",
                "pricePaid": 31_000_000.0
            }),
            BotState::ListingNoName,
            4
        )]
    );
}

#[tokio::test]
async fn inventory_provider_can_snapshot_and_queue_listings() {
    let mut session = RuntimeSession::new(runtime());
    let inventory = Arc::new(FakeInventoryProvider::default());
    let queue = Arc::new(FakeQueueStore::default());
    session
        .add_queue_store(AccountId::new("MainAccount").unwrap(), queue.clone())
        .set_fallback_inventory_provider(inventory.clone());

    let snapshot = session
        .process_local_command(LocalCommand::Terminal {
            line: "inventory".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    let queued = session
        .process_local_command(LocalCommand::Terminal {
            line: "sell_inventory".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        snapshot,
        RuntimeOutcome::InventorySnapshot {
            snapshot: InventorySnapshot { ref items, .. },
        } if items.len() == 3
    ));
    assert!(matches!(
        queued,
        RuntimeOutcome::InventoryListingsQueued { queued: 1, .. }
    ));
    assert_eq!(
        inventory.requests.lock().unwrap().as_slice(),
        &[
            AccountId::new("MainAccount").unwrap(),
            AccountId::new("MainAccount").unwrap(),
        ]
    );
    assert_eq!(
        queue.entries.lock().unwrap().as_slice(),
        &[(
            AccountId::new("MainAccount").unwrap(),
            json!({
                "auctionID": "main-item",
                "inv": "main-item",
                "price": 5_000_000.0,
                "weirdItemName": "Aspect of the Dragons",
                "tag": "ASPECT_OF_THE_DRAGON",
                "time": 48
            }),
            BotState::ListingNoName,
            4
        )]
    );
}

#[tokio::test]
async fn sell_inventory_respects_do_not_relist_item_enchantments() {
    let mut session = RuntimeSession::new(runtime());
    let inventory = Arc::new(FixedItemsInventoryProvider {
        items: vec![
            InventoryItem {
                uuid: Some("blocked-lava-shell".to_string()),
                item_name: "Waxed Lava Shell Necklace".to_string(),
                lore: vec!["§dThe One V".to_string()],
                price: Some(9_000_000.0),
                tag: Some("LAVA_SHELL_NECKLACE".to_string()),
                slot: Some(37),
                in_hotbar: true,
            },
            InventoryItem {
                uuid: Some("safe-item".to_string()),
                item_name: "Aspect of the Dragons".to_string(),
                lore: Vec::new(),
                price: Some(5_000_000.0),
                tag: Some("ASPECT_OF_THE_DRAGON".to_string()),
                slot: Some(38),
                in_hotbar: true,
            },
        ],
    });
    let queue = Arc::new(FakeQueueStore::default());
    session
        .add_queue_store(AccountId::new("MainAccount").unwrap(), queue.clone())
        .set_fallback_inventory_provider(inventory);

    let outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "sell_inventory hotbar".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::InventoryListingsQueued { queued: 1, .. }
    ));
    assert_eq!(
        queue.entries.lock().unwrap().as_slice(),
        &[(
            AccountId::new("MainAccount").unwrap(),
            json!({
                "auctionID": "safe-item",
                "inv": "safe-item",
                "price": 5_000_000.0,
                "weirdItemName": "Aspect of the Dragons",
                "tag": "ASPECT_OF_THE_DRAGON",
                "time": 48
            }),
            BotState::ListingNoName,
            4
        )]
    );
}

#[test]
fn inventory_listing_action_rejects_unlistable_inputs() {
    let mut item = InventoryItem {
        uuid: Some("main-item".to_string()),
        item_name: "Aspect of the Dragons".to_string(),
        lore: Vec::new(),
        price: Some(5_000_000.0),
        tag: Some("ASPECT_OF_THE_DRAGON".to_string()),
        slot: Some(10),
        in_hotbar: false,
    };

    assert_eq!(
        command_planning::inventory_listing_action(&item).unwrap(),
        json!({
            "auctionID": "main-item",
            "inv": "main-item",
            "price": 5_000_000.0,
            "weirdItemName": "Aspect of the Dragons",
            "tag": "ASPECT_OF_THE_DRAGON",
            "time": 48
        })
    );

    item.uuid = None;
    assert!(command_planning::inventory_listing_action(&item).is_none());

    item.uuid = Some("main-item".to_string());
    item.price = Some(f64::NAN);
    assert!(command_planning::inventory_listing_action(&item).is_none());

    item.price = Some(499.0);
    assert!(command_planning::inventory_listing_action(&item).is_none());
}

#[tokio::test]
async fn active_auction_provider_can_queue_delist_all() {
    let mut session = RuntimeSession::new(runtime());
    let auctions = Arc::new(FakeActiveAuctionProvider::default());
    let queue = Arc::new(FakeQueueStore::default());
    session
        .add_queue_store(AccountId::new("MainAccount").unwrap(), queue.clone())
        .set_fallback_active_auction_provider(auctions.clone());

    let outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "delist_everything".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::DelistAllQueued { queued: 1, .. }
    ));
    assert_eq!(
        auctions.requests.lock().unwrap().as_slice(),
        &[AccountId::new("MainAccount").unwrap()]
    );
    assert_eq!(
        queue.entries.lock().unwrap().as_slice(),
        &[(
            AccountId::new("MainAccount").unwrap(),
            json!({
                "auctionID": "auction-1",
                "itemUuid": "item-1"
            }),
            BotState::Delisting,
            3
        )]
    );
}

#[tokio::test]
async fn session_clears_saved_data_through_registered_ports() {
    let mut session = RuntimeSession::new(runtime());
    let saved_data = Arc::new(FakeSavedDataStore::default());
    session.add_saved_data_store(AccountId::new("MainAccount").unwrap(), saved_data.clone());

    let cleared = session
        .process_local_command(LocalCommand::Terminal {
            line: "clear_data".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        cleared,
        RuntimeOutcome::SavedDataCleared {
            queue_removed: 2,
            bid_data_cleared: true,
            ..
        }
    ));
    assert_eq!(
        saved_data.clear_results.lock().unwrap().as_slice(),
        &[(
            AccountId::new("MainAccount").unwrap(),
            crate::SavedDataClear {
                queue_removed: 2,
                bid_data_cleared: true,
            }
        )]
    );
}

#[tokio::test]
async fn session_applies_blacklist_commands_through_registered_ports() {
    let mut session = RuntimeSession::new(runtime());
    let blacklist = Arc::new(FakeBlacklistStore::default());
    session.add_blacklist_store(AccountId::new("MainAccount").unwrap(), blacklist.clone());

    let outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "blacklist add buy tag SPEED_RELIC --for 7d".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::BlacklistApplied {
            result: BlacklistApplyResult { changed: true, .. },
            ..
        }
    ));
    assert_eq!(
        blacklist.requests.lock().unwrap().as_slice(),
        &[(
            AccountId::new("MainAccount").unwrap(),
            crate::BlacklistRequest::Update(crate::BlacklistUpdate {
                action: crate::BlacklistAction::Add,
                scope: crate::BlacklistScope::Buy,
                field: crate::BlacklistField::Tag,
                value: "SPEED_RELIC".to_string(),
                duration: Some("7d".to_string()),
                until: None,
            })
        )]
    );
}

#[tokio::test]
async fn session_reads_stats_and_ping_through_registered_ports() {
    let mut session = RuntimeSession::new(runtime());
    let stats = Arc::new(FakeStatsProvider::default());
    session.add_stats_provider(AccountId::new("MainAccount").unwrap(), stats.clone());

    let stats_outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "stats".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    let ping_outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "ping".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    let profit_outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "profit".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        stats_outcome,
        RuntimeOutcome::StatsSnapshot {
            stats: AccountStats { bought: 3, .. },
            ..
        }
    ));
    assert!(matches!(
        ping_outcome,
        RuntimeOutcome::PingSnapshot {
            ping: AccountPing {
                cofl_ping_ms: Some(30),
                ..
            },
            ..
        }
    ));
    assert!(matches!(
        profit_outcome,
        RuntimeOutcome::ProfitSnapshot {
            stats: AccountStats {
                total_profit: 42_000_000.0,
                ..
            },
            ..
        }
    ));
    assert_eq!(
        stats.requests.lock().unwrap().as_slice(),
        &[
            (AccountId::new("MainAccount").unwrap(), "stats"),
            (AccountId::new("MainAccount").unwrap(), "ping"),
            (AccountId::new("MainAccount").unwrap(), "stats"),
        ]
    );
}

#[tokio::test]
async fn session_ping_requests_live_ping_samples_when_ports_exist() {
    let mut session = RuntimeSession::new(runtime());
    let account = AccountId::new("MainAccount").unwrap();
    let stats = Arc::new(FakeStatsProvider::default());
    let cofl = Arc::new(FakeCofl::default());
    let minecraft = Arc::new(FakeMinecraft {
        account: account.clone(),
        actions: Arc::new(Mutex::new(Vec::new())),
    });
    session
        .add_stats_provider(account.clone(), stats)
        .add_cofl_client(account.clone(), cofl.clone())
        .add_minecraft_client(account.clone(), minecraft.clone());

    let outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "ping".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(outcome, RuntimeOutcome::PingSnapshot { .. }));
    assert_eq!(
        cofl.commands.lock().unwrap().as_slice(),
        &[
            (account.clone(), "/cofl ping".to_string()),
            (account.clone(), "/cofl delay".to_string()),
        ]
    );
    assert_eq!(
        minecraft.actions.lock().unwrap().as_slice(),
        &[MinecraftAction::Chat("/social pingwars".to_string())]
    );
}

#[tokio::test]
async fn session_reports_users_and_global_stats() {
    let mut session = RuntimeSession::new(runtime());
    let stats = Arc::new(FakeStatsProvider::default());
    let connections = Arc::new(FakeConnectionProvider::default());
    let logs = Arc::new(FakeLogReader::default());
    session.set_fallback_stats_provider(stats.clone());
    session.set_fallback_connection_provider(connections.clone());
    session.set_log_reader(logs.clone());

    let users = session
        .process_local_command(LocalCommand::Terminal {
            line: "users".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    let global = session
        .process_local_command(LocalCommand::Terminal {
            line: "global_stats".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    let connection_outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "connections".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    let log_outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "messages 20".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        users,
        RuntimeOutcome::UsersSnapshot {
            ref configured,
            ref running,
            ref default,
        } if configured == &vec!["MainAccount".to_string(), "MainAlt".to_string()]
            && running == &vec!["MainAccount".to_string(), "MainAlt".to_string()]
            && default.as_deref() == Some("MainAccount")
    ));
    assert!(matches!(
        global,
        RuntimeOutcome::GlobalStatsSnapshot {
            total_profit: 84_000_000.0,
            bought: 6,
            sold: 4,
            ref accounts,
        } if accounts.len() == 2
    ));
    assert_eq!(
        stats.requests.lock().unwrap().as_slice(),
        &[
            (AccountId::new("MainAccount").unwrap(), "stats"),
            (AccountId::new("MainAlt").unwrap(), "stats"),
        ]
    );
    assert!(matches!(
        connection_outcome,
        RuntimeOutcome::ConnectionsSnapshot { ref connections }
            if connections.len() == 2
                && connections[0].connection_id.as_deref() == Some("connection-MainAccount")
    ));
    assert_eq!(
        connections.requests.lock().unwrap().as_slice(),
        &[
            AccountId::new("MainAccount").unwrap(),
            AccountId::new("MainAlt").unwrap(),
        ]
    );
    assert!(matches!(
        log_outcome,
        RuntimeOutcome::LogSnapshot {
            snapshot: LogSnapshot {
                exists: true,
                ref lines,
                ..
            },
        } if lines == &vec!["one".to_string(), "two".to_string()]
    ));
    assert_eq!(logs.requests.lock().unwrap().as_slice(), &[20]);
}

#[tokio::test]
async fn session_reports_empty_global_status_when_no_accounts_are_running() {
    let config = SafConfig {
        igns: vec!["MainAccount".to_string(), "MainAlt".to_string()],
        default_ign: "MainAccount".to_string(),
        ..SafConfig::default()
    };
    let mut session = RuntimeSession::new(BotRuntime::from_config(&config, Vec::new()));
    let stats = Arc::new(FakeStatsProvider::default());
    let connections = Arc::new(FakeConnectionProvider::default());
    session.set_fallback_stats_provider(stats.clone());
    session.set_fallback_connection_provider(connections.clone());

    let global = session
        .process_local_command(LocalCommand::Terminal {
            line: "global_stats".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    let connection_outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "connections".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        global,
        RuntimeOutcome::GlobalStatsSnapshot {
            total_profit: 0.0,
            bought: 0,
            sold: 0,
            ref accounts,
        } if accounts.is_empty()
    ));
    assert!(matches!(
        connection_outcome,
        RuntimeOutcome::ConnectionsSnapshot { ref connections } if connections.is_empty()
    ));
    assert!(stats.requests.lock().unwrap().is_empty());
    assert!(connections.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn session_schedules_account_actions_through_registered_ports() {
    let mut session = RuntimeSession::new(runtime());
    let scheduler = Arc::new(FakeAccountScheduler::default());
    session.add_account_scheduler(AccountId::new("MainAccount").unwrap(), scheduler.clone());

    let outcome = session
        .process_local_command(LocalCommand::Terminal {
            line: "timeout 15s MainAccount".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        outcome,
        RuntimeOutcome::AccountScheduled {
            result: AccountScheduleResult {
                delay_ms: 15_000,
                ..
            }
        }
    ));
    assert_eq!(
        scheduler.requests.lock().unwrap().as_slice(),
        &[AccountScheduleRequest {
            account: AccountId::new("MainAccount").unwrap(),
            action: ScheduledAccountAction::Stop,
            delay_ms: 15_000,
        }]
    );
}

#[tokio::test]
async fn session_executes_account_lifecycle_through_supervisor() {
    let mut session = RuntimeSession::new(runtime());
    let supervisor = Arc::new(FakeSupervisor::default());
    session.set_account_supervisor(supervisor.clone());

    let stop = session
        .process_local_command(LocalCommand::Terminal {
            line: "stop MainAlt".to_string(),
            created_at: None,
        })
        .await
        .unwrap();
    assert_eq!(session.running_accounts(), vec!["MainAccount".to_string()]);
    assert!(
        session
            .selector()
            .resolve_running(Some("MainAlt"))
            .is_none()
    );
    let start = session
        .process_local_command(LocalCommand::Terminal {
            line: "start MainAlt".to_string(),
            created_at: None,
        })
        .await
        .unwrap();

    assert!(matches!(
        start,
        RuntimeOutcome::Executed {
            directive: RuntimeDirective::StartAccounts { .. }
        }
    ));
    assert!(matches!(
        stop,
        RuntimeOutcome::Executed {
            directive: RuntimeDirective::StopAccounts { .. }
        }
    ));
    assert_eq!(
        supervisor.actions.lock().unwrap().as_slice(),
        &[
            SupervisorAction::Stop(Some(AccountId::new("MainAlt").unwrap())),
            SupervisorAction::Start(AccountId::new("MainAlt").unwrap())
        ]
    );
    assert_eq!(
        session.running_accounts(),
        vec!["MainAccount".to_string(), "MainAlt".to_string()]
    );
}
