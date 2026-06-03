use crate::ids::AccountId;
use crate::ports::{
    AccountConnection, AccountPing, AccountScheduleResult, AccountStats, GuiSlotDiagnostics,
    InventorySnapshot, LogSnapshot, PortError, ScheduledAccountAction,
};
use crate::{BlacklistApplyResult, BotState, FlipOutcome};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RuntimeDirective {
    StartAccounts {
        accounts: Vec<AccountId>,
    },
    StopAccounts {
        account: Option<AccountId>,
    },
    TransferCoins {
        from: AccountId,
        to: AccountId,
        amount: String,
        stop_source: bool,
    },
    SendMinecraftChat {
        account: AccountId,
        message: String,
    },
    SendCoflCommand {
        account: AccountId,
        command: String,
    },
    ShowStats {
        account: AccountId,
    },
    ShowProfit {
        account: AccountId,
    },
    ShowPing {
        account: AccountId,
    },
    ShowUsers,
    ShowGlobalStats,
    ShowConnections,
    ShowLogs {
        lines: usize,
    },
    ShowQueue {
        account: AccountId,
    },
    ClearQueue {
        account: AccountId,
    },
    CancelQueueEntry {
        account: AccountId,
        index: usize,
    },
    ClearAllQueues,
    ClearData {
        account: AccountId,
    },
    BlacklistCommand {
        account: AccountId,
        message: String,
    },
    CheckBids {
        account: AccountId,
    },
    Bank {
        account: AccountId,
        request: BankRequest,
    },
    QueueState {
        account: AccountId,
        action: serde_json::Value,
        state: BotState,
        priority: u8,
    },
    ExternalBuy {
        account: AccountId,
        auction_id: String,
    },
    TrackedListFlip {
        account: AccountId,
        auction_id: String,
        time_hours: f64,
    },
    ShowInventory {
        account: AccountId,
    },
    SellInventory {
        account: AccountId,
        include_hotbar: bool,
    },
    QueueDelistAll {
        account: AccountId,
    },
    DiagnoseSlots {
        account: AccountId,
        target: Option<String>,
    },
    TestWebhook {
        account: AccountId,
    },
    Cookie {
        account: AccountId,
    },
    ScheduleAccount {
        account: AccountId,
        action: ScheduledAccountAction,
        delay_ms: u64,
    },
    UnknownTerminalCommand {
        account: AccountId,
        command: String,
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BankRequest {
    pub amount: Option<String>,
    pub withdraw: bool,
    pub personal: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatsSnapshot {
    pub account: AccountId,
    pub stats: AccountStats,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum RuntimeOutcome {
    Planned {
        directive: RuntimeDirective,
    },
    Executed {
        directive: RuntimeDirective,
    },
    FlipProcessed {
        account: AccountId,
        outcome: FlipOutcome,
    },
    Queued {
        directive: RuntimeDirective,
        changed: bool,
    },
    QueueSnapshot {
        account: AccountId,
        queue: Vec<crate::QueueEntry>,
    },
    QueueCleared {
        account: AccountId,
        removed: usize,
    },
    QueueEntryCancelled {
        account: AccountId,
        entry: Option<crate::QueueEntry>,
        index: usize,
    },
    QueuesCleared {
        accounts: Vec<QueueClearSnapshot>,
    },
    SavedDataCleared {
        account: AccountId,
        queue_removed: usize,
        bid_data_cleared: bool,
    },
    BlacklistApplied {
        account: AccountId,
        result: BlacklistApplyResult,
    },
    StatsSnapshot {
        account: AccountId,
        stats: AccountStats,
    },
    ProfitSnapshot {
        account: AccountId,
        stats: AccountStats,
    },
    PingSnapshot {
        account: AccountId,
        ping: AccountPing,
    },
    UsersSnapshot {
        configured: Vec<String>,
        running: Vec<String>,
        default: Option<String>,
    },
    GlobalStatsSnapshot {
        accounts: Vec<AccountStatsSnapshot>,
        total_profit: f64,
        bought: usize,
        sold: usize,
    },
    ConnectionsSnapshot {
        connections: Vec<AccountConnection>,
    },
    LogSnapshot {
        snapshot: LogSnapshot,
    },
    InventorySnapshot {
        snapshot: InventorySnapshot,
    },
    InventoryListingsQueued {
        account: AccountId,
        queued: usize,
    },
    DelistAllQueued {
        account: AccountId,
        queued: usize,
    },
    GuiSlotDiagnostics {
        diagnostics: GuiSlotDiagnostics,
    },
    AccountScheduled {
        result: AccountScheduleResult,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueClearSnapshot {
    pub account: AccountId,
    pub removed: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error("{0}")]
    Invalid(String),
    #[error("{0}")]
    Route(String),
    #[error("port operation failed: {0}")]
    Port(String),
    #[error(transparent)]
    PortError(#[from] PortError),
}
