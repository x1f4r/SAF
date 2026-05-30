use saf_core::ports::{AccountScheduleRequest, MinecraftAction, Notification};
use saf_core::{AccountId, BlacklistRequest, BotState, QueueEntry, RuntimeOutcome};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxProcessingReport {
    pub results: Vec<InboxCommandResult>,
    pub cofl_commands: Vec<CoflCommandRecord>,
    pub queue_entries: Vec<QueueRecord>,
    pub saved_data_clears: Vec<SavedDataClearRecord>,
    pub blacklist_requests: Vec<BlacklistRecord>,
    pub stats_requests: Vec<StatsRequestRecord>,
    pub scheduled_accounts: Vec<AccountScheduleRecord>,
    pub lifecycle_actions: Vec<LifecycleRecord>,
    pub notifications: Vec<Notification>,
    pub minecraft_actions: Vec<MinecraftActionRecord>,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum InboxCommandResult {
    Processed {
        index: usize,
        outcome: RuntimeOutcome,
    },
    Failed {
        index: usize,
        error: String,
    },
    Invalid {
        index: usize,
        line: String,
        error: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoflCommandRecord {
    pub account: AccountId,
    pub command: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftActionRecord {
    pub account: AccountId,
    pub actions: Vec<MinecraftAction>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueRecord {
    pub account: AccountId,
    pub action: Value,
    pub state: BotState,
    pub priority: u8,
}

impl From<QueueRecord> for QueueEntry {
    fn from(record: QueueRecord) -> Self {
        Self {
            action: record.action,
            state: record.state,
            priority: record.priority,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedDataClearRecord {
    pub account: AccountId,
    pub queue_removed: usize,
    pub bid_data_cleared: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlacklistRecord {
    pub account: AccountId,
    pub request: BlacklistRequest,
    pub changed: bool,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatsRequestRecord {
    pub account: AccountId,
    pub kind: StatsRequestKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum StatsRequestKind {
    Stats,
    Ping,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountScheduleRecord {
    pub request: AccountScheduleRequest,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleRecord {
    pub action: LifecycleAction,
    pub account: Option<AccountId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LifecycleAction {
    Start,
    Stop,
}
