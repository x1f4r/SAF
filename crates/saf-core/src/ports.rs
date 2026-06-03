use crate::blacklist::{BlacklistApplyResult, BlacklistRequest};
use crate::gui::WindowSnapshot;
use crate::ids::{AccountId, AuctionId};
use crate::state::{BotState, QueueEntry, SavedDataClear};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Discriminator describing the kind of operator event a [`Notification`]
/// represents. Output layers (Discord embeds, webhooks) use it to pick a
/// colour and icon so each event type reads at a glance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NotificationKind {
    FlipFound,
    Bought,
    Sold,
    Listed,
    Relisted,
    Blocked,
    Error,
    LoginRequired,
    Stopped,
    Started,
    #[default]
    Info,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notification {
    #[serde(default)]
    pub kind: NotificationKind,
    pub title: String,
    pub body: String,
    pub account: Option<AccountId>,
    /// Structured `(label, value)` rows rendered as embed fields. Empty for
    /// plain notifications.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<(String, String)>,
    /// Player-head (or other) image used as the embed thumbnail.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail_url: Option<String>,
}

impl Notification {
    /// Build a notification of the given kind with the standard text fields.
    /// Structured `fields` and `thumbnail_url` start empty and can be added
    /// with the builder methods.
    pub fn new(
        kind: NotificationKind,
        title: impl Into<String>,
        body: impl Into<String>,
        account: Option<AccountId>,
    ) -> Self {
        Self {
            kind,
            title: title.into(),
            body: body.into(),
            account,
            fields: Vec::new(),
            thumbnail_url: None,
        }
    }

    /// Attach structured `(label, value)` rows rendered as embed fields.
    #[must_use]
    pub fn with_fields(mut self, fields: Vec<(String, String)>) -> Self {
        self.fields = fields;
        self
    }

    /// Attach a thumbnail image (typically the account player head).
    #[must_use]
    pub fn with_thumbnail(mut self, thumbnail_url: Option<String>) -> Self {
        self.thumbnail_url = thumbnail_url;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MinecraftAction {
    Chat(String),
    OpenAuction(AuctionId),
    ClickSlot(usize),
    SwapSlotToHotbar {
        slot: usize,
        hotbar_slot: u8,
    },
    SetHeldHotbarSlot(u8),
    ActivateHeldItem,
    TypeText(String),
    /// Nudge the avatar's view by a small relative yaw/pitch delta (degrees).
    /// Used by the idle anti-AFK behaviour to look around without ever moving
    /// the body — it is strictly a rotation and never a positional translation.
    LookDelta {
        yaw_delta: f32,
        pitch_delta: f32,
    },
    /// Queue a single in-place jump for the next tick. Like `LookDelta`, this
    /// produces no horizontal translation, so it can never walk the avatar off
    /// its island.
    Jump,
    CloseWindow,
    Disconnect,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum MinecraftEvent {
    Ready { reason: String },
    ChatMessage { text: String },
    WindowOpen(WindowSnapshot),
    WindowClosed,
    Scoreboard { lines: Vec<String> },
    Kicked { reason: String },
    Disconnected { reason: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GuiSlotDiagnostics {
    pub account: AccountId,
    pub target: Option<String>,
    pub windows: Vec<WindowSnapshot>,
}

#[async_trait]
pub trait MinecraftClient: Send + Sync {
    async fn account(&self) -> AccountId;
    async fn perform(&self, action: MinecraftAction) -> Result<(), PortError>;
    async fn next_event(&self) -> Result<Option<MinecraftEvent>, PortError> {
        Ok(None)
    }
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn notify(&self, notification: Notification) -> Result<(), PortError>;
}

#[async_trait]
pub trait CoflClient: Send + Sync {
    async fn send_command(&self, account: &AccountId, command: &str) -> Result<(), PortError>;
}

#[async_trait]
pub trait GuiDiagnosticsProvider: Send + Sync {
    async fn diagnose_slots(
        &self,
        account: &AccountId,
        target: Option<&str>,
    ) -> Result<GuiSlotDiagnostics, PortError>;
}

#[async_trait]
pub trait QueueStore: Send + Sync {
    async fn add(
        &self,
        account: &AccountId,
        action: Value,
        state: BotState,
        priority: u8,
    ) -> Result<bool, PortError>;
    async fn snapshot(&self, account: &AccountId) -> Result<Vec<QueueEntry>, PortError>;
    async fn clear(&self, account: &AccountId) -> Result<usize, PortError>;
    async fn remove_at(
        &self,
        account: &AccountId,
        index: usize,
    ) -> Result<Option<QueueEntry>, PortError>;
}

#[async_trait]
pub trait SavedDataStore: Send + Sync {
    async fn clear_saved_data(&self, account: &AccountId) -> Result<SavedDataClear, PortError>;
}

#[async_trait]
pub trait BlacklistStore: Send + Sync {
    async fn apply(
        &self,
        account: &AccountId,
        request: BlacklistRequest,
    ) -> Result<BlacklistApplyResult, PortError>;
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStats {
    pub bought: usize,
    pub sold: usize,
    pub total_profit: f64,
    pub user_finder_flips: usize,
    pub profit_per_hour: Option<f64>,
    pub purse: Option<f64>,
    pub started_at_ms: Option<u64>,
    pub cofl_delay_ms: Option<u64>,
    pub cofl_ping_ms: Option<u64>,
    pub cofl_tier: Option<String>,
    pub cofl_expires_at: Option<u64>,
    pub cookie_expires_at: Option<u64>,
    pub hypixel_ping_ms: Option<u64>,
    pub auction_slots_used: Option<usize>,
    pub auction_slots_max: Option<usize>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountPing {
    pub cofl_delay_ms: Option<u64>,
    pub cofl_ping_ms: Option<u64>,
    pub hypixel_ping_ms: Option<u64>,
}

#[async_trait]
pub trait AccountStatsProvider: Send + Sync {
    async fn stats(&self, account: &AccountId) -> Result<AccountStats, PortError>;
    async fn ping(&self, account: &AccountId) -> Result<AccountPing, PortError>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ScheduledAccountAction {
    Start,
    Stop,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountScheduleRequest {
    pub account: AccountId,
    pub action: ScheduledAccountAction,
    pub delay_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountScheduleResult {
    pub account: AccountId,
    pub action: ScheduledAccountAction,
    pub delay_ms: u64,
}

#[async_trait]
pub trait AccountScheduler: Send + Sync {
    async fn schedule(
        &self,
        request: AccountScheduleRequest,
    ) -> Result<AccountScheduleResult, PortError>;
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuctionMetadata {
    pub auction_id: String,
    pub item_name: Option<String>,
    pub starting_bid: Option<f64>,
    pub tag: Option<String>,
}

#[async_trait]
pub trait AuctionMetadataProvider: Send + Sync {
    async fn lookup(&self, auction_id: &str) -> Result<Option<AuctionMetadata>, PortError>;
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackedFlip {
    pub auction_id: String,
    pub target_price: f64,
    pub weird_item_name: Option<String>,
    pub tag: Option<String>,
    pub price_paid: Option<f64>,
    #[serde(default)]
    pub finder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profit_percentage: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buy_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seen_at_ms: Option<u64>,
}

#[async_trait]
pub trait TrackedFlipProvider: Send + Sync {
    async fn lookup(
        &self,
        account: &AccountId,
        auction_id: &str,
    ) -> Result<Option<TrackedFlip>, PortError>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountConnection {
    pub account: AccountId,
    pub connection_id: Option<String>,
}

#[async_trait]
pub trait AccountConnectionProvider: Send + Sync {
    async fn connection_id(&self, account: &AccountId) -> Result<Option<String>, PortError>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogSnapshot {
    pub path: String,
    pub exists: bool,
    pub lines: Vec<String>,
}

#[async_trait]
pub trait LogReader: Send + Sync {
    async fn latest(&self, lines: usize) -> Result<LogSnapshot, PortError>;
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryItem {
    pub uuid: Option<String>,
    pub item_name: String,
    #[serde(default)]
    pub lore: Vec<String>,
    pub price: Option<f64>,
    pub tag: Option<String>,
    pub slot: Option<u8>,
    pub in_hotbar: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InventorySnapshot {
    pub account: AccountId,
    pub items: Vec<InventoryItem>,
}

#[async_trait]
pub trait InventoryProvider: Send + Sync {
    async fn snapshot(&self, account: &AccountId) -> Result<InventorySnapshot, PortError>;
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveAuction {
    pub auction_id: String,
    pub item_uuid: String,
    pub name: Option<String>,
}

#[async_trait]
pub trait ActiveAuctionProvider: Send + Sync {
    async fn active_auctions(&self, account: &AccountId) -> Result<Vec<ActiveAuction>, PortError>;
}

#[async_trait]
pub trait AccountSupervisor: Send + Sync {
    async fn start(&self, account: &AccountId) -> Result<(), PortError>;
    async fn stop(&self, account: Option<AccountId>) -> Result<(), PortError>;
}

/// Forces a booster-cookie buy/refresh for an account right now, regardless of
/// how much cookie time is left. The live runtime owns the multi-step bazaar
/// flow, so the port only records the request; the runtime loop drains it and
/// drives the existing auto-cookie machinery.
#[async_trait]
pub trait CookieForcer: Send + Sync {
    async fn force_cookie(&self, account: &AccountId) -> Result<(), PortError>;
}

#[derive(Debug, thiserror::Error)]
pub enum PortError {
    #[error("port unavailable: {0}")]
    Unavailable(String),
    #[error("port operation failed: {0}")]
    Failed(String),
}
