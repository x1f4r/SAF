//! Embedded loopback dashboard API (REST + WebSocket).
//!
//! This module is compiled only with the `api` feature. It is spawned as a
//! tokio task from inside the live runtime (`run-live`) so it shares the same
//! `Arc<RuntimeSession>` and in-memory stat providers the Discord gateway uses,
//! and drives commands through the identical `process_local_command` path.
//!
//! The server binds `127.0.0.1` only. It is never exposed on a public
//! interface; the macOS dashboard reaches it through an SSH tunnel. A bearer
//! token is still required on every request as defense-in-depth on the shared
//! VPS.

use super::stats::{ClaimStatsUpdate, LiveStatsProvider, PurchaseStatsUpdate, SoldStatsUpdate};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path as AxumPath, Query, State};
use axum::http::{StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use saf_core::ports::{
    AccountConnectionProvider, AccountStats, AccountStatsProvider, Notification, Notifier,
    PortError,
};
use saf_core::{AccountId, LocalCommand, RuntimeSession, SafConfig};
use saf_discord::{
    CommandInvocation, CommandOptionValue, DiscordCommandPlan, DiscordControllerAction,
    command_definitions, plan_button, plan_invocation,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;

const DEFAULT_PORT: u16 = 8787;
const EVENT_CHANNEL_CAPACITY: usize = 1024;
const LEDGER_FILE: &str = "SavedData/dashboard-ledger.jsonl";
const LEDGER_READ_LIMIT: usize = 20_000;

// ---------------------------------------------------------------------------
// Live event stream + persistent ledger
// ---------------------------------------------------------------------------

/// One structured buy/sell event recorded the moment SAF parses the in-game
/// chat line. Pushed live over WebSocket and appended to the on-disk ledger so
/// profit history survives restarts and VPS moves.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::live_runtime) struct FlipRecord {
    pub ts: u64,
    pub account: String,
    pub item: String,
    pub weird_item_name: String,
    pub tag: Option<String>,
    pub price: u64,
    pub target_price: f64,
    pub profit: f64,
    pub finder: String,
    pub volume: Option<f64>,
    pub profit_percentage: Option<f64>,
    pub buy_kind: String,
    pub buy_speed_ms: Option<u64>,
    pub auction_id: String,
}

impl FlipRecord {
    pub(in crate::live_runtime) fn from_update(
        account: &AccountId,
        p: &PurchaseStatsUpdate,
        ts: u64,
    ) -> Self {
        Self {
            ts,
            account: account.to_string(),
            item: p.item_name.clone(),
            weird_item_name: p.weird_item_name.clone(),
            tag: p.tag.clone(),
            price: p.price,
            target_price: p.target_price,
            profit: p.profit,
            finder: p.finder.clone(),
            volume: p.volume,
            profit_percentage: p.profit_percentage,
            buy_kind: p.buy_kind.clone(),
            buy_speed_ms: p.buy_speed_ms,
            auction_id: p.auction_id.clone(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::live_runtime) struct SaleRecord {
    pub ts: u64,
    pub account: String,
    pub item: String,
    pub buyer: String,
    pub price: u64,
}

impl SaleRecord {
    pub(in crate::live_runtime) fn from_update(
        account: &AccountId,
        s: &SoldStatsUpdate,
        ts: u64,
    ) -> Self {
        Self {
            ts,
            account: account.to_string(),
            item: s.item_name.clone(),
            buyer: s.buyer.clone(),
            price: s.price,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::live_runtime) struct ClaimRecord {
    pub ts: u64,
    pub account: String,
    pub item: String,
    pub buyer: String,
    pub coins: u64,
}

impl ClaimRecord {
    pub(in crate::live_runtime) fn from_update(
        account: &AccountId,
        c: &ClaimStatsUpdate,
        ts: u64,
    ) -> Self {
        Self {
            ts,
            account: account.to_string(),
            item: c.item_name.clone(),
            buyer: c.buyer.clone(),
            coins: c.coins,
        }
    }
}

/// A message broadcast to every connected WebSocket client.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub(in crate::live_runtime) enum LiveEvent {
    Purchase(FlipRecord),
    Sold(SaleRecord),
    Claim(ClaimRecord),
    Notification {
        ts: u64,
        notification: Value,
    },
    State {
        ts: u64,
        halted: bool,
        paused: bool,
        running: Vec<String>,
    },
}

/// The persisted form, one JSON object per ledger line, tagged by `kind`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum LedgerRecord {
    Purchase(FlipRecord),
    Sold(SaleRecord),
    Claim(ClaimRecord),
}

/// Sink trait the stats provider holds so it can emit without depending on the
/// concrete hub type.
pub(in crate::live_runtime) trait DashboardEventSink:
    Send + Sync + std::fmt::Debug
{
    fn emit(&self, event: LiveEvent);
}

#[derive(Debug)]
struct LedgerWriter {
    path: PathBuf,
    lock: std::sync::Mutex<()>,
}

impl LedgerWriter {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            lock: std::sync::Mutex::new(()),
        }
    }

    fn append(&self, record: &LedgerRecord) {
        let Ok(line) = serde_json::to_string(record) else {
            return;
        };
        let _guard = self
            .lock
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
        {
            let _ = writeln!(file, "{line}");
        }
    }

    fn read(&self, limit: usize) -> Vec<LedgerRecord> {
        let Ok(raw) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        let lines: Vec<&str> = raw.lines().filter(|l| !l.trim().is_empty()).collect();
        let start = lines.len().saturating_sub(limit);
        lines[start..]
            .iter()
            .filter_map(|line| serde_json::from_str::<LedgerRecord>(line).ok())
            .collect()
    }
}

/// Shared event hub: fans buys/sells into the live WebSocket channel and the
/// persistent ledger. Constructed only when the API is enabled at runtime.
#[derive(Debug)]
pub(in crate::live_runtime) struct DashboardHub {
    events_tx: broadcast::Sender<LiveEvent>,
    ledger: LedgerWriter,
}

impl DashboardHub {
    /// Build a hub when `SAF_API_ENABLED` is truthy, otherwise `None` so the
    /// runtime stays exactly as it was before this feature.
    pub(in crate::live_runtime) fn maybe(state_base_dir: &std::path::Path) -> Option<Arc<Self>> {
        if !env_truthy("SAF_API_ENABLED") {
            return None;
        }
        let (events_tx, _) = broadcast::channel(EVENT_CHANNEL_CAPACITY);
        Some(Arc::new(Self {
            events_tx,
            ledger: LedgerWriter::new(state_base_dir.join(LEDGER_FILE)),
        }))
    }

    pub(in crate::live_runtime) fn events_tx(&self) -> broadcast::Sender<LiveEvent> {
        self.events_tx.clone()
    }
}

impl DashboardEventSink for DashboardHub {
    fn emit(&self, event: LiveEvent) {
        match &event {
            LiveEvent::Purchase(r) => self.ledger.append(&LedgerRecord::Purchase(r.clone())),
            LiveEvent::Sold(r) => self.ledger.append(&LedgerRecord::Sold(r.clone())),
            LiveEvent::Claim(r) => self.ledger.append(&LedgerRecord::Claim(r.clone())),
            LiveEvent::Notification { .. } | LiveEvent::State { .. } => {}
        }
        // Lagging/absent subscribers are fine; drop on a full channel.
        let _ = self.events_tx.send(event);
    }
}

/// Notifier that mirrors every operator notification (flip found, errors,
/// startup) into the live event stream.
#[derive(Clone)]
pub(in crate::live_runtime) struct BroadcastNotifier {
    events_tx: broadcast::Sender<LiveEvent>,
}

impl BroadcastNotifier {
    pub(in crate::live_runtime) fn new(events_tx: broadcast::Sender<LiveEvent>) -> Self {
        Self { events_tx }
    }
}

#[async_trait::async_trait]
impl Notifier for BroadcastNotifier {
    async fn notify(&self, notification: Notification) -> Result<(), PortError> {
        let event = LiveEvent::Notification {
            ts: now_ms(),
            notification: serde_json::to_value(&notification).unwrap_or(Value::Null),
        };
        let _ = self.events_tx.send(event);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// HTTP/WS server
// ---------------------------------------------------------------------------

/// Everything the spawned server needs, assembled in `LiveRuntime::start`.
pub(in crate::live_runtime) struct ApiContext {
    pub session: Arc<RuntimeSession>,
    pub stats: Arc<LiveStatsProvider>,
    pub hub: Arc<DashboardHub>,
    pub halted: Arc<AtomicBool>,
    pub accounts: Vec<AccountId>,
    pub config: SafConfig,
    pub state_base_dir: PathBuf,
    pub command_inbox: PathBuf,
    pub pause_file: PathBuf,
    pub log_path: PathBuf,
    pub started_at_ms: u64,
    pub version: String,
    pub market_mode: String,
}

#[derive(Clone)]
struct ApiState {
    session: Arc<RuntimeSession>,
    stats: Arc<LiveStatsProvider>,
    events_tx: broadcast::Sender<LiveEvent>,
    ledger: Arc<LedgerWriter>,
    halted: Arc<AtomicBool>,
    accounts: Arc<Vec<AccountId>>,
    token: Arc<str>,
    state_base_dir: Arc<PathBuf>,
    command_inbox: Arc<PathBuf>,
    pause_file: Arc<PathBuf>,
    log_path: Arc<PathBuf>,
    started_at_ms: u64,
    version: Arc<str>,
    market_mode: Arc<str>,
    branding_name: Arc<str>,
}

/// Spawn the server when the API is enabled and a token is configured. Returns
/// the task handle so the runtime can abort it on shutdown.
pub(in crate::live_runtime) fn maybe_spawn_server(
    ctx: ApiContext,
) -> Option<tokio::task::JoinHandle<()>> {
    let token = std::env::var("SAF_API_TOKEN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let Some(token) = token else {
        tracing::warn!(
            "SAF_API_ENABLED is set but SAF_API_TOKEN is empty; dashboard API not started"
        );
        return None;
    };
    let port = std::env::var("SAF_API_PORT")
        .ok()
        .and_then(|value| value.trim().parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);

    let state = ApiState {
        session: ctx.session,
        stats: ctx.stats,
        events_tx: ctx.hub.events_tx(),
        ledger: Arc::new(LedgerWriter::new(ctx.state_base_dir.join(LEDGER_FILE))),
        halted: ctx.halted,
        accounts: Arc::new(ctx.accounts),
        token: Arc::from(token.as_str()),
        state_base_dir: Arc::new(ctx.state_base_dir),
        command_inbox: Arc::new(ctx.command_inbox),
        pause_file: Arc::new(ctx.pause_file),
        log_path: Arc::new(ctx.log_path),
        started_at_ms: ctx.started_at_ms,
        version: Arc::from(ctx.version.as_str()),
        market_mode: Arc::from(ctx.market_mode.as_str()),
        branding_name: Arc::from(ctx.config.branding.name.as_str()),
    };

    Some(tokio::spawn(async move {
        if let Err(error) = serve(state, port).await {
            tracing::error!(error = %error, "dashboard API server stopped");
        }
    }))
}

async fn serve(state: ApiState, port: u16) -> anyhow::Result<()> {
    let protected = Router::new()
        .route("/v1/status", get(status))
        .route("/v1/accounts", get(accounts))
        .route("/v1/accounts/{ign}/queue", get(account_queue))
        .route("/v1/profit", get(profit_global))
        .route("/v1/profit/{ign}", get(profit_account))
        .route("/v1/profit/{ign}/series", get(profit_series))
        .route("/v1/flips", get(flips))
        .route("/v1/logs", get(logs))
        .route("/v1/alerts", get(alerts))
        .route("/v1/commands", get(commands_catalog))
        .route("/v1/command", post(execute_command))
        .route("/v1/control/{action}", post(control))
        .route("/v1/events", get(events_ws))
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));

    let app = Router::new()
        .route("/healthz", get(healthz))
        .merge(protected)
        .with_state(state);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "dashboard API listening (loopback only)");
    axum::serve(listener, app.into_make_service()).await?;
    Ok(())
}

fn json_error(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    (
        status,
        Json(json!({ "error": code, "message": message.into() })),
    )
        .into_response()
}

async fn require_auth(
    State(state): State<ApiState>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let header_token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .map(str::trim);
    let query_token = request.uri().query().and_then(|q| {
        url::form_urlencoded::parse(q.as_bytes())
            .find(|(key, _)| key == "token")
            .map(|(_, value)| value.into_owned())
    });
    let presented = header_token
        .map(str::to_string)
        .or(query_token)
        .unwrap_or_default();
    if constant_time_eq(presented.as_bytes(), state.token.as_bytes()) {
        next.run(request).await
    } else {
        json_error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid bearer token",
        )
    }
}

async fn healthz(State(state): State<ApiState>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "service": "saf-dashboard-api",
        "name": state.branding_name.as_ref(),
        "version": state.version.as_ref(),
        "uptimeMs": now_ms().saturating_sub(state.started_at_ms),
    }))
}

async fn status(State(state): State<ApiState>) -> Json<Value> {
    let selector = state.session.selector();
    let running = state.session.running_accounts();
    Json(json!({
        "ok": true,
        "version": state.version.as_ref(),
        "name": state.branding_name.as_ref(),
        "startedAtMs": state.started_at_ms,
        "uptimeMs": now_ms().saturating_sub(state.started_at_ms),
        "marketMode": state.market_mode.as_ref(),
        "halted": state.halted.load(Ordering::SeqCst),
        "paused": state.pause_file.exists(),
        "configured": selector.configured,
        "running": running,
        "defaultIgn": selector.default_ign,
        "commandInbox": state.command_inbox.display().to_string(),
        "stateBaseDir": state.state_base_dir.display().to_string(),
    }))
}

async fn account_summary(state: &ApiState, ign: &AccountId, running: &[String]) -> Value {
    let stats = AccountStatsProvider::stats(state.stats.as_ref(), ign)
        .await
        .ok();
    let connection_id = AccountConnectionProvider::connection_id(state.stats.as_ref(), ign)
        .await
        .ok()
        .flatten();
    let queue_size = crate::state_cli::snapshot(state.state_base_dir.as_ref(), ign.as_str())
        .map(|snapshot| snapshot.queue.len())
        .unwrap_or(0);
    let is_running = running
        .iter()
        .any(|name| name.eq_ignore_ascii_case(ign.as_str()));

    // Honest, derived per-account status so the dashboard reflects reality
    // (connected? has a booster cookie? enough coins?) rather than showing every
    // configured account as if it were flipping.
    let cofl_connected = connection_id.is_some();
    let now_secs = now_ms() / 1000;
    let has_cookie = stats
        .as_ref()
        .and_then(|s| s.cookie_expires_at)
        .is_some_and(|expires| expires > now_secs);
    let purse = stats.as_ref().and_then(|s| s.purse);
    let min_coins = std::env::var("SAF_MIN_FLIP_COINS")
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .unwrap_or(1_000_000.0);

    let (status, ready, reason): (&str, bool, Option<&str>) = if !is_running {
        ("offline", false, Some("Stopped"))
    } else if !cofl_connected {
        ("connecting", false, Some("Connecting to SkyCofl"))
    } else if !has_cookie {
        ("online", false, Some("No booster cookie"))
    } else if purse.is_some_and(|p| p < min_coins) {
        ("online", false, Some("Low coins"))
    } else {
        ("online", true, None)
    };

    json!({
        "ign": ign.as_str(),
        "running": is_running,
        "queueSize": queue_size,
        "connectionId": connection_id,
        "coflConnected": cofl_connected,
        "hasCookie": has_cookie,
        "status": status,
        "ready": ready,
        "reason": reason,
        "stats": stats,
        "headUrl": crate::player_head::account_head_thumbnail_url(ign.as_str()),
    })
}

async fn accounts(State(state): State<ApiState>) -> Json<Value> {
    let selector = state.session.selector();
    let running = state.session.running_accounts();
    let mut list = Vec::with_capacity(state.accounts.len());
    for ign in state.accounts.iter() {
        list.push(account_summary(&state, ign, &running).await);
    }
    let ready_count = list
        .iter()
        .filter(|a| a["ready"].as_bool() == Some(true))
        .count();
    let connected_count = list
        .iter()
        .filter(|a| a["status"].as_str() == Some("online"))
        .count();
    Json(json!({
        "configured": selector.configured,
        "running": running,
        "defaultIgn": selector.default_ign,
        "readyCount": ready_count,
        "connectedCount": connected_count,
        "accounts": list,
    }))
}

async fn account_queue(State(state): State<ApiState>, AxumPath(ign): AxumPath<String>) -> Response {
    match crate::state_cli::snapshot(state.state_base_dir.as_ref(), ign.trim()) {
        Ok(snapshot) => Json(json!({
            "ign": ign,
            "queue": snapshot.queue,
            "bidData": snapshot.bid_data,
        }))
        .into_response(),
        Err(error) => json_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "queue_read_failed",
            error.to_string(),
        ),
    }
}

fn profit_summary_from(stats: &AccountStats) -> Value {
    json!({
        "totalProfit": stats.total_profit,
        "bought": stats.bought,
        "sold": stats.sold,
        "userFinderFlips": stats.user_finder_flips,
        "profitPerHour": stats.profit_per_hour,
        "purse": stats.purse,
    })
}

async fn profit_global(State(state): State<ApiState>) -> Json<Value> {
    let mut total_profit = 0.0;
    let mut bought = 0usize;
    let mut sold = 0usize;
    let mut purse = 0.0;
    let mut per_account = Vec::new();
    for ign in state.accounts.iter() {
        if let Ok(stats) = AccountStatsProvider::stats(state.stats.as_ref(), ign).await {
            total_profit += stats.total_profit.max(0.0);
            bought += stats.bought;
            sold += stats.sold;
            purse += stats.purse.unwrap_or(0.0);
            per_account
                .push(json!({ "ign": ign.as_str(), "summary": profit_summary_from(&stats) }));
        }
    }

    // Lifetime aggregates from the persistent ledger. These survive process
    // restarts and VPS moves, so the dashboard can show history even right after
    // a restart when the in-memory counters are empty.
    let mut ledger_profit = 0.0;
    let mut ledger_bought = 0usize;
    let mut ledger_sold = 0usize;
    for record in state.ledger.read(LEDGER_READ_LIMIT) {
        match record {
            LedgerRecord::Purchase(flip) => {
                ledger_bought += 1;
                ledger_profit += flip.profit;
            }
            LedgerRecord::Sold(_) => ledger_sold += 1,
            LedgerRecord::Claim(_) => {}
        }
    }

    Json(json!({
        "totalProfit": total_profit,
        "bought": bought,
        "sold": sold,
        "purse": purse,
        "accounts": per_account,
        "lifetime": {
            "totalProfit": ledger_profit,
            "bought": ledger_bought,
            "sold": ledger_sold,
        },
    }))
}

async fn profit_account(
    State(state): State<ApiState>,
    AxumPath(ign): AxumPath<String>,
) -> Response {
    let Some(account) = AccountId::new(ign.trim()) else {
        return json_error(
            StatusCode::BAD_REQUEST,
            "bad_account",
            "invalid account name",
        );
    };
    match AccountStatsProvider::stats(state.stats.as_ref(), &account).await {
        Ok(stats) => Json(json!({
            "ign": account.as_str(),
            "summary": profit_summary_from(&stats),
            "stats": stats,
        }))
        .into_response(),
        Err(error) => json_error(
            StatusCode::NOT_FOUND,
            "account_not_found",
            error.to_string(),
        ),
    }
}

#[derive(Deserialize)]
struct SeriesQuery {
    since: Option<u64>,
    bucket: Option<u64>,
    account: Option<String>,
}

async fn profit_series(
    State(state): State<ApiState>,
    AxumPath(ign): AxumPath<String>,
    Query(query): Query<SeriesQuery>,
) -> Json<Value> {
    let bucket_ms = query.bucket.unwrap_or(3600).max(1) * 1000;
    let since = query.since.unwrap_or(0);
    let account_filter = query
        .account
        .filter(|_| ign == "all")
        .or_else(|| (ign != "all").then(|| ign.clone()));
    let mut buckets: BTreeMap<u64, (f64, u64)> = BTreeMap::new();
    for record in state.ledger.read(LEDGER_READ_LIMIT) {
        let LedgerRecord::Purchase(flip) = record else {
            continue;
        };
        if flip.ts < since {
            continue;
        }
        if let Some(filter) = &account_filter
            && !flip.account.eq_ignore_ascii_case(filter)
        {
            continue;
        }
        let key = flip.ts / bucket_ms * bucket_ms;
        let entry = buckets.entry(key).or_insert((0.0, 0));
        entry.0 += flip.profit;
        entry.1 += 1;
    }
    let mut cumulative = 0.0;
    let points: Vec<Value> = buckets
        .into_iter()
        .map(|(ts, (profit, count))| {
            cumulative += profit;
            json!({ "ts": ts, "profit": profit, "cumulative": cumulative, "count": count })
        })
        .collect();
    Json(json!({ "bucketMs": bucket_ms, "points": points }))
}

#[derive(Deserialize)]
struct FlipsQuery {
    kind: Option<String>,
    account: Option<String>,
    limit: Option<usize>,
}

async fn flips(State(state): State<ApiState>, Query(query): Query<FlipsQuery>) -> Json<Value> {
    let limit = query.limit.unwrap_or(100).min(2000);
    let kind = query.kind.unwrap_or_else(|| "bought".to_string());
    let account = query.account;
    let records = state.ledger.read(LEDGER_READ_LIMIT);
    let mut out: Vec<Value> = Vec::new();
    for record in records.into_iter().rev() {
        let matches_account = |acc: &str| {
            account
                .as_ref()
                .map(|a| acc.eq_ignore_ascii_case(a))
                .unwrap_or(true)
        };
        let value = match (&record, kind.as_str()) {
            (LedgerRecord::Purchase(f), "bought") if matches_account(&f.account) => {
                serde_json::to_value(f).ok()
            }
            (LedgerRecord::Sold(s), "sold") if matches_account(&s.account) => {
                serde_json::to_value(s).ok()
            }
            (LedgerRecord::Claim(c), "claimed") if matches_account(&c.account) => {
                serde_json::to_value(c).ok()
            }
            _ => None,
        };
        if let Some(value) = value {
            out.push(value);
            if out.len() >= limit {
                break;
            }
        }
    }
    Json(json!({ "kind": kind, "flips": out }))
}

#[derive(Deserialize)]
struct LogsQuery {
    lines: Option<usize>,
}

async fn logs(State(state): State<ApiState>, Query(query): Query<LogsQuery>) -> Json<Value> {
    let want = query.lines.unwrap_or(200).min(2000);
    let content = read_runtime_log(&state).await;
    let all: Vec<String> = content.lines().map(strip_ansi).collect();
    let start = all.len().saturating_sub(want);
    Json(json!({ "lines": all[start..].to_vec() }))
}

#[derive(Deserialize)]
struct AlertsQuery {
    lines: Option<usize>,
}

/// Recent WARN/ERROR log lines, parsed for the Diagnostics/Alerts feed.
async fn alerts(State(state): State<ApiState>, Query(query): Query<AlertsQuery>) -> Json<Value> {
    let want = query.lines.unwrap_or(120).min(500);
    let content = read_runtime_log(&state).await;
    let mut out: Vec<Value> = Vec::new();
    for raw in content.lines().rev() {
        let line = strip_ansi(raw);
        let mut tokens = line.split_whitespace();
        let ts = tokens.next().unwrap_or("").to_string();
        let level = match tokens.next() {
            Some("ERROR") => "error",
            Some("WARN") => "warn",
            _ => continue,
        };
        out.push(json!({ "ts": ts, "level": level, "message": line }));
        if out.len() >= want {
            break;
        }
    }
    out.reverse();
    Json(json!({ "alerts": out }))
}

/// Read the runtime log, preferring the configured `latest.log` but falling back
/// to the tmux console log the supervised runtime actually writes.
async fn read_runtime_log(state: &ApiState) -> String {
    let primary = tokio::fs::read_to_string(state.log_path.as_ref())
        .await
        .unwrap_or_default();
    if !primary.trim().is_empty() {
        return primary;
    }
    if let Some(dir) = state.log_path.parent() {
        for name in ["tmux-console.log", "latest.log"] {
            if let Ok(text) = tokio::fs::read_to_string(dir.join(name)).await {
                if !text.trim().is_empty() {
                    return text;
                }
            }
        }
    }
    primary
}

/// Strip ANSI color escapes so parsed/rendered log lines are clean.
fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for n in chars.by_ref() {
                if n == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

async fn commands_catalog() -> Json<Value> {
    Json(json!({ "commands": command_definitions() }))
}

#[derive(Deserialize)]
struct CommandRequest {
    command: Option<String>,
    #[serde(default)]
    options: BTreeMap<String, Value>,
    button: Option<String>,
    line: Option<String>,
    transfer: Option<TransferRequest>,
}

#[derive(Deserialize)]
struct TransferRequest {
    from: String,
    to: String,
    #[serde(default = "default_amount")]
    amount: String,
    #[serde(default)]
    stop_source: Option<bool>,
}

fn default_amount() -> String {
    "all".to_string()
}

async fn execute_command(
    State(state): State<ApiState>,
    Json(req): Json<CommandRequest>,
) -> Response {
    // Raw terminal line passthrough.
    if let Some(line) = req
        .line
        .as_ref()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
    {
        return run_local_command(
            &state,
            LocalCommand::Terminal {
                line: line.to_string(),
                created_at: Some(now_ms()),
            },
        )
        .await;
    }
    // Coin transfer.
    if let Some(transfer) = req.transfer {
        return run_local_command(
            &state,
            LocalCommand::Transfer {
                from: transfer.from,
                to: transfer.to,
                amount: transfer.amount,
                stop_source: transfer.stop_source.unwrap_or(true),
                created_at: Some(now_ms()),
            },
        )
        .await;
    }
    // Dashboard button id (used for confirm actions and quick buttons).
    if let Some(button) = req
        .button
        .as_ref()
        .map(|b| b.trim())
        .filter(|b| !b.is_empty())
    {
        return match plan_button(button) {
            Ok(Some(plan)) => run_plan(&state, plan).await,
            Ok(None) => Json(json!({ "ok": true, "canceled": true })).into_response(),
            Err(error) => json_error(StatusCode::BAD_REQUEST, "bad_button", error.to_string()),
        };
    }
    // Slash command form.
    if let Some(name) = req
        .command
        .as_ref()
        .map(|c| c.trim())
        .filter(|c| !c.is_empty())
    {
        let mut invocation = CommandInvocation::new(name.to_string());
        for (key, value) in req.options {
            if let Some(option) = option_value(value) {
                invocation.options.insert(key, option);
            }
        }
        return match plan_invocation(&invocation) {
            Ok(plan) => run_plan(&state, plan).await,
            Err(error) => json_error(StatusCode::BAD_REQUEST, "bad_command", error.to_string()),
        };
    }
    json_error(
        StatusCode::BAD_REQUEST,
        "empty_command",
        "provide command, button, line, or transfer",
    )
}

fn option_value(value: Value) -> Option<CommandOptionValue> {
    match value {
        Value::String(text) => Some(CommandOptionValue::String(text)),
        Value::Bool(flag) => Some(CommandOptionValue::Boolean(flag)),
        Value::Number(number) => number
            .as_i64()
            .map(CommandOptionValue::Integer)
            .or_else(|| Some(CommandOptionValue::String(number.to_string()))),
        _ => None,
    }
}

async fn run_plan(state: &ApiState, plan: DiscordCommandPlan) -> Response {
    match plan {
        DiscordCommandPlan::LocalCommand { command } => run_local_command(state, command).await,
        DiscordCommandPlan::ControllerAction { action } => {
            controller_action_response(state, action)
        }
    }
}

fn controller_action_response(state: &ApiState, action: DiscordControllerAction) -> Response {
    let resolve = |username: &Option<String>| {
        state
            .session
            .selector()
            .resolve_running(username.as_deref())
            .or_else(|| username.clone())
            .unwrap_or_default()
    };
    let body = match action {
        DiscordControllerAction::Dashboard => json!({ "ok": true, "action": "dashboard" }),
        DiscordControllerAction::Status => json!({ "ok": true, "action": "status" }),
        DiscordControllerAction::Help => {
            json!({ "ok": true, "action": "help", "commands": command_definitions() })
        }
        DiscordControllerAction::AccountPanel { username } => {
            json!({ "ok": true, "action": "accountPanel", "account": resolve(&Some(username)) })
        }
        DiscordControllerAction::DiscordId => {
            json!({ "ok": true, "action": "discordId", "message": "Not applicable to the desktop app." })
        }
        DiscordControllerAction::RefreshHeads { username } => {
            json!({ "ok": true, "action": "refreshHeads", "account": resolve(&username) })
        }
        DiscordControllerAction::SellInventory {
            username,
            include_hotbar,
        } => {
            let account = resolve(&username);
            json!({
                "ok": true,
                "requiresConfirmation": true,
                "confirm": {
                    "title": "Queue inventory listings",
                    "message": format!("Queue listings for {account}. Hotbar items {}.", if include_hotbar { "included" } else { "excluded" }),
                    "button": format!("saf:confirmSellInventory:{account}:{}", if include_hotbar { 1 } else { 0 }),
                },
            })
        }
        DiscordControllerAction::DelistEverything { username } => {
            let account = resolve(&username);
            json!({
                "ok": true,
                "requiresConfirmation": true,
                "confirm": {
                    "title": "Delist everything",
                    "message": format!("Scan active auctions for {account} and queue every one for delisting."),
                    "button": format!("saf:confirmDelistAll:{account}"),
                },
            })
        }
        DiscordControllerAction::StatusForConfirmation { username, action } => {
            let account = resolve(&Some(username));
            json!({
                "ok": true,
                "requiresConfirmation": true,
                "confirm": {
                    "title": "Confirm action",
                    "message": format!("Confirm `{action}` for {account}."),
                    "button": format!("saf:confirm{}:{account}", capitalize(&action)),
                },
            })
        }
    };
    Json(body).into_response()
}

fn capitalize(value: &str) -> String {
    let cleaned: String = value
        .split(['_', '-'])
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect::<String>(),
                None => String::new(),
            }
        })
        .collect();
    cleaned
}

async fn run_local_command(state: &ApiState, command: LocalCommand) -> Response {
    match state.session.process_local_command(command).await {
        Ok(outcome) => Json(json!({ "ok": true, "outcome": outcome })).into_response(),
        Err(error) => json_error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "command_failed",
            error.to_string(),
        ),
    }
}

async fn control(State(state): State<ApiState>, AxumPath(action): AxumPath<String>) -> Response {
    let line = match action.as_str() {
        "start" => "start",
        "stop" => "stop",
        other => {
            return json_error(
                StatusCode::BAD_REQUEST,
                "bad_action",
                format!("unknown control action `{other}` (use start or stop)"),
            );
        }
    };
    run_local_command(
        &state,
        LocalCommand::Terminal {
            line: line.to_string(),
            created_at: Some(now_ms()),
        },
    )
    .await
}

async fn events_ws(State(state): State<ApiState>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| events_socket(socket, state))
}

async fn events_socket(mut socket: WebSocket, state: ApiState) {
    // Initial snapshot so a freshly connected client knows current state.
    let initial = LiveEvent::State {
        ts: now_ms(),
        halted: state.halted.load(Ordering::SeqCst),
        paused: state.pause_file.exists(),
        running: state.session.running_accounts(),
    };
    if let Ok(text) = serde_json::to_string(&initial)
        && socket.send(Message::Text(text.into())).await.is_err()
    {
        return;
    }

    let mut rx = state.events_tx.subscribe();
    loop {
        tokio::select! {
            received = rx.recv() => {
                match received {
                    Ok(event) => {
                        if let Ok(text) = serde_json::to_string(&event)
                            && socket.send(Message::Text(text.into())).await.is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Ping(payload))) => {
                        if socket.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn now_ms() -> u64 {
    super::now_ms()
}

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|value| {
            let value = value.trim();
            value == "1" || value.eq_ignore_ascii_case("true")
        })
        .unwrap_or(false)
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() || a.is_empty() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}
