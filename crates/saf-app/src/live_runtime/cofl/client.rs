use super::super::{COFL_REGION_BACKOFF, COFL_SILENT_OPEN_TIMEOUT, should_upload_scoreboard};
use super::connection::LiveCoflClientState;
use anyhow::{Context, Result};
use async_trait::async_trait;
use regex::Regex;
use saf_cofl::{CoflSettingsMutation, CoflSettingsSummary};
use saf_core::numbers::{add_commas_to_number, parse_number_input};
use saf_core::ports::{CoflClient, PortError};
use saf_core::protocol_text::clean_scoreboard_lines;
use saf_core::{AccountId, FlipEvent, Humanizer};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex as AsyncMutex;

const INITIAL_SCOREBOARD_UPLOAD_DELAY: Duration = Duration::from_millis(5_500);
const STARTUP_ACCOUNT_INFO_RETRY_INTERVAL: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug)]
pub(in crate::live_runtime) struct StartupAccountInfoRequest {
    connected_at: Instant,
    requested_at: Instant,
}

#[derive(Clone, Copy, Debug)]
pub(in crate::live_runtime) struct CoflFlipSafety {
    pub(in crate::live_runtime) min_profit: f64,
    pub(in crate::live_runtime) min_profit_percent: f64,
}

impl CoflFlipSafety {
    pub(in crate::live_runtime) fn new(min_profit: f64, min_profit_percent: f64) -> Option<Self> {
        (min_profit.is_finite()
            && min_profit > 0.0
            && min_profit_percent.is_finite()
            && min_profit_percent > 0.0)
            .then_some(Self {
                min_profit,
                min_profit_percent,
            })
    }
}

#[derive(Clone, Debug, Default)]
struct LiveCoflSettingsState {
    min_profit: Option<f64>,
    min_profit_raw: Option<String>,
    min_profit_percent: Option<f64>,
    min_profit_percent_raw: Option<String>,
    using: Option<String>,
}

impl LiveCoflSettingsState {
    fn replace_summary(&mut self, summary: &CoflSettingsSummary) {
        self.min_profit = parse_optional_setting(summary.min_profit.as_deref());
        self.min_profit_raw = summary.min_profit.clone();
        self.min_profit_percent = parse_optional_setting(summary.min_profit_percent.as_deref());
        self.min_profit_percent_raw = summary.min_profit_percent.clone();
        if summary.using.is_some() {
            self.using = summary.using.clone();
        }
    }

    fn apply_mutation(&mut self, mutation: &CoflSettingsMutation) {
        if let Some(value) = mutation.min_profit.as_deref() {
            self.min_profit = parse_optional_setting(Some(value));
            self.min_profit_raw = Some(value.to_string());
        }
        if let Some(value) = mutation.min_profit_percent.as_deref() {
            self.min_profit_percent = parse_optional_setting(Some(value));
            self.min_profit_percent_raw = Some(value.to_string());
        }
    }

    fn is_complete_for_safety(&self) -> bool {
        self.min_profit.is_some()
    }
}

fn parse_optional_setting(value: Option<&str>) -> Option<f64> {
    value.and_then(|value| parse_number_input(value.to_string().into()))
}

fn format_floor(value: f64) -> String {
    add_commas_to_number(value)
}

#[derive(Debug, Default)]
struct CoflPrivacyFilter {
    pattern: Option<String>,
    regex: Option<Regex>,
}

impl CoflPrivacyFilter {
    fn update(&mut self, pattern: &str) -> Result<()> {
        let regex = Regex::new(pattern).with_context(|| "invalid Cofl privacy chat regex")?;
        self.pattern = Some(pattern.to_string());
        self.regex = Some(regex);
        Ok(())
    }

    fn matches(&self, text: &str) -> bool {
        self.regex
            .as_ref()
            .is_some_and(|regex| regex.is_match(text))
    }
}

pub(in crate::live_runtime) struct LiveCoflClient {
    account: AccountId,
    session_id: String,
    pub(in crate::live_runtime) state: AsyncMutex<LiveCoflClientState>,
    privacy_filter: Mutex<CoflPrivacyFilter>,
    initial_scoreboard_upload_at: Mutex<Option<Instant>>,
    startup_account_info_requested_at: Mutex<Option<StartupAccountInfoRequest>>,
    startup_account_info_accepted: AtomicBool,
    settings_loaded: AtomicBool,
    pending_inventory_upload: AtomicBool,
    settings_state: Mutex<LiveCoflSettingsState>,
    flip_safety: Option<CoflFlipSafety>,
    fallback_settings_summary_requested: AtomicBool,
    humanizer: Option<Arc<Humanizer>>,
    last_server_switch: Mutex<Option<Instant>>,
}

impl LiveCoflClient {
    #[cfg(test)]
    pub(in crate::live_runtime) fn new(
        account: AccountId,
        link: String,
        session_id: String,
        default_link: String,
    ) -> Self {
        Self::new_with_extras(account, link, session_id, default_link, None, None)
    }

    #[cfg(test)]
    pub(in crate::live_runtime) fn new_with_safety(
        account: AccountId,
        link: String,
        session_id: String,
        default_link: String,
        flip_safety: Option<CoflFlipSafety>,
    ) -> Self {
        Self::new_with_extras(account, link, session_id, default_link, flip_safety, None)
    }

    pub(in crate::live_runtime) fn new_with_extras(
        account: AccountId,
        link: String,
        session_id: String,
        default_link: String,
        flip_safety: Option<CoflFlipSafety>,
        humanizer: Option<Arc<Humanizer>>,
    ) -> Self {
        Self {
            account,
            session_id,
            state: AsyncMutex::new(LiveCoflClientState::new_with_humanizer(
                link,
                default_link,
                humanizer.clone(),
            )),
            privacy_filter: Mutex::new(CoflPrivacyFilter::default()),
            initial_scoreboard_upload_at: Mutex::new(None),
            startup_account_info_requested_at: Mutex::new(None),
            startup_account_info_accepted: AtomicBool::new(false),
            settings_loaded: AtomicBool::new(false),
            pending_inventory_upload: AtomicBool::new(false),
            settings_state: Mutex::new(LiveCoflSettingsState::default()),
            flip_safety,
            fallback_settings_summary_requested: AtomicBool::new(false),
            humanizer,
            last_server_switch: Mutex::new(None),
        }
    }

    pub(super) fn session_id(&self) -> &str {
        &self.session_id
    }

    pub(in crate::live_runtime) fn mark_settings_loaded(&self) {
        self.settings_loaded.store(true, Ordering::Relaxed);
    }

    pub(in crate::live_runtime) fn mark_settings_unloaded(&self) {
        self.settings_loaded.store(false, Ordering::Relaxed);
        self.fallback_settings_summary_requested
            .store(false, Ordering::Relaxed);
    }

    pub(in crate::live_runtime) fn defer_inventory_upload(&self) {
        self.pending_inventory_upload.store(true, Ordering::Relaxed);
    }

    pub(in crate::live_runtime) fn take_deferred_inventory_upload(&self) -> bool {
        self.pending_inventory_upload.swap(false, Ordering::Relaxed)
    }

    #[cfg(test)]
    pub(in crate::live_runtime) fn deferred_inventory_upload_pending(&self) -> bool {
        self.pending_inventory_upload.load(Ordering::Relaxed)
    }

    pub(in crate::live_runtime) fn settings_loaded(&self) -> bool {
        self.settings_loaded.load(Ordering::Relaxed)
    }

    pub(in crate::live_runtime) fn replace_settings_summary(&self, summary: &CoflSettingsSummary) {
        match self.settings_state.lock() {
            Ok(mut settings) => settings.replace_summary(summary),
            Err(_) => tracing::warn!(
                account = %self.account,
                "Cofl settings lock poisoned; cannot update loaded settings"
            ),
        }
    }

    pub(in crate::live_runtime) fn apply_settings_mutation(&self, mutation: &CoflSettingsMutation) {
        match self.settings_state.lock() {
            Ok(mut settings) => settings.apply_mutation(mutation),
            Err(_) => tracing::warn!(
                account = %self.account,
                "Cofl settings lock poisoned; cannot update setting mutation"
            ),
        }
    }

    pub(in crate::live_runtime) fn should_request_text_settings_summary(&self) -> bool {
        if self.flip_safety.is_none() {
            return false;
        }
        let incomplete = match self.settings_state.lock() {
            Ok(settings) => !settings.is_complete_for_safety(),
            Err(_) => true,
        };
        incomplete
            && !self
                .fallback_settings_summary_requested
                .swap(true, Ordering::Relaxed)
    }

    pub(in crate::live_runtime) fn flip_safety_violation(
        &self,
        flip: &FlipEvent,
    ) -> Option<String> {
        let safety = self.flip_safety?;
        if !flip.is_valid() {
            return None;
        }
        let settings = match self.settings_state.lock() {
            Ok(settings) => settings.clone(),
            Err(_) => {
                return Some("Cofl settings safety state is unavailable".to_string());
            }
        };
        if settings
            .min_profit
            .is_some_and(|min_profit| min_profit + f64::EPSILON < safety.min_profit)
        {
            return Some(format!(
                "loaded Cofl MinProfit `{}` is below required `{}`",
                settings.min_profit_raw.as_deref().unwrap_or("unknown"),
                format_floor(safety.min_profit)
            ));
        }
        if settings
            .min_profit_percent
            .is_some_and(|min_percent| min_percent + f64::EPSILON < safety.min_profit_percent)
        {
            return Some(format!(
                "loaded Cofl MinProfitPercent `{}` is below required `{}`",
                settings
                    .min_profit_percent_raw
                    .as_deref()
                    .unwrap_or("unknown"),
                safety.min_profit_percent
            ));
        }
        if flip.profit + f64::EPSILON < safety.min_profit {
            return Some(format!(
                "flip profit `{}` is below required `{}`",
                format_floor(flip.profit),
                format_floor(safety.min_profit)
            ));
        }
        if flip.profit_percentage + f64::EPSILON < safety.min_profit_percent {
            return Some(format!(
                "flip profit percent `{:.2}` is below required `{}`",
                flip.profit_percentage, safety.min_profit_percent
            ));
        }
        None
    }

    pub(in crate::live_runtime) fn mark_startup_account_info_accepted(&self) {
        self.startup_account_info_accepted
            .store(true, Ordering::Relaxed);
    }

    pub(in crate::live_runtime) fn reset_startup_account_info_state(&self) {
        self.startup_account_info_accepted
            .store(false, Ordering::Relaxed);
        self.fallback_settings_summary_requested
            .store(false, Ordering::Relaxed);
        match self.startup_account_info_requested_at.lock() {
            Ok(mut requested_at) => *requested_at = None,
            Err(_) => tracing::warn!(
                account = %self.account,
                "startup Cofl account-info lock poisoned while resetting request state"
            ),
        }
    }

    #[cfg(test)]
    pub(in crate::live_runtime) async fn current_link(&self) -> String {
        self.state.lock().await.link.clone()
    }

    pub(in crate::live_runtime) async fn switch_link(&self, link: String) {
        let mut state = self.state.lock().await;
        let now = Instant::now();
        if state.is_region_backed_off(&link, now) {
            tracing::debug!(
                account = %self.account,
                link = %saf_cofl::redact_cofl_socket_link(&link),
                "ignoring Cofl socket switch while region is backed off"
            );
            return;
        }
        if state.link != link {
            if let Some(humanizer) = self.humanizer.as_ref() {
                if let Err(wait) = humanizer.try_register_server_switch(now) {
                    tracing::debug!(
                        account = %self.account,
                        link = %saf_cofl::redact_cofl_socket_link(&link),
                        wait_ms = wait.as_millis() as u64,
                        "deferring Cofl socket switch while humanizer cooldown is active"
                    );
                    if let Ok(mut last) = self.last_server_switch.lock() {
                        *last = Some(now);
                    }
                    return;
                }
                if let Ok(mut last) = self.last_server_switch.lock() {
                    *last = Some(now);
                }
            }
            state.link = link;
        }
        state.defer_reconnect(Duration::from_secs(5));
    }

    pub(in crate::live_runtime) async fn send_raw_if_connected(
        &self,
        message: &str,
    ) -> Result<bool, PortError> {
        let mut state = self.state.lock().await;
        let Some(client) = state.client.as_ref() else {
            return Ok(false);
        };
        match client.send_raw(message).await {
            Ok(()) => Ok(true),
            Err(error) => {
                state.disconnect();
                Err(PortError::Failed(error.to_string()))
            }
        }
    }

    async fn connected_at(&self) -> Option<Instant> {
        self.state.lock().await.connected_at
    }

    pub(in crate::live_runtime) fn initial_scoreboard_upload_message(
        &self,
        connected_at: Instant,
        lines: &[String],
    ) -> Result<Option<String>> {
        self.initial_scoreboard_upload_message_at(connected_at, Instant::now(), lines)
    }

    pub(in crate::live_runtime) fn initial_scoreboard_upload_message_at(
        &self,
        connected_at: Instant,
        now: Instant,
        lines: &[String],
    ) -> Result<Option<String>> {
        let lines = clean_scoreboard_lines(lines);
        if !should_upload_scoreboard(&lines) {
            return Ok(None);
        }
        if now.saturating_duration_since(connected_at) < INITIAL_SCOREBOARD_UPLOAD_DELAY {
            return Ok(None);
        }
        if self
            .initial_scoreboard_upload_at
            .lock()
            .map_err(|_| anyhow::anyhow!("initial scoreboard upload lock poisoned"))?
            .is_some_and(|uploaded_at| uploaded_at == connected_at)
        {
            return Ok(None);
        }
        Ok(Some(saf_cofl::encode_scoreboard_upload(&lines)?))
    }

    pub(in crate::live_runtime) fn mark_initial_scoreboard_uploaded(&self, connected_at: Instant) {
        match self.initial_scoreboard_upload_at.lock() {
            Ok(mut uploaded_at) => *uploaded_at = Some(connected_at),
            Err(_) => tracing::warn!(
                account = %self.account,
                "initial scoreboard upload lock poisoned; upload may repeat after reconnect"
            ),
        }
    }

    pub(super) async fn upload_initial_scoreboard_if_needed(&self, lines: &[String]) {
        let Some(connected_at) = self.connected_at().await else {
            return;
        };
        let message = match self.initial_scoreboard_upload_message(connected_at, lines) {
            Ok(Some(message)) => message,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(
                    account = %self.account,
                    error = %error,
                    "failed to encode initial Cofl scoreboard upload"
                );
                return;
            }
        };

        match self.send_raw_if_connected(&message).await {
            Ok(true) => {
                self.mark_initial_scoreboard_uploaded(connected_at);
                tracing::info!(
                    account = %self.account,
                    "uploaded initial scoreboard to Cofl websocket"
                );
            }
            Ok(false) => {}
            Err(error) => tracing::warn!(
                account = %self.account,
                error = %error,
                "failed to upload cached scoreboard to Cofl websocket"
            ),
        }
    }

    async fn has_received_message(&self) -> bool {
        self.state.lock().await.has_received_message()
    }

    pub(super) async fn request_startup_account_info_if_needed(&self) {
        if self.startup_account_info_accepted.load(Ordering::Relaxed) {
            return;
        }
        let Some(connected_at) = self.connected_at().await else {
            return;
        };
        if !self.has_received_message().await {
            return;
        }
        let now = Instant::now();
        if !self.startup_account_info_request_due_at(connected_at, now) {
            return;
        }

        if let Err(error) = self.send_command(&self.account, "/cofl get json").await {
            tracing::warn!(
                account = %self.account,
                error = %error,
                "failed to request startup Cofl account info"
            );
            return;
        }
        tracing::info!(
            account = %self.account,
            "requested startup Cofl account info"
        );

        self.mark_startup_account_info_requested_at(connected_at, now);
    }

    pub(in crate::live_runtime) fn startup_account_info_request_due_at(
        &self,
        connected_at: Instant,
        now: Instant,
    ) -> bool {
        if self.startup_account_info_accepted.load(Ordering::Relaxed) {
            return false;
        }
        match self.startup_account_info_requested_at.lock() {
            Ok(requested_at) => requested_at.is_none_or(|request| {
                request.connected_at != connected_at
                    || now.saturating_duration_since(request.requested_at)
                        >= STARTUP_ACCOUNT_INFO_RETRY_INTERVAL
            }),
            Err(_) => {
                tracing::warn!(
                    account = %self.account,
                    "startup Cofl account-info lock poisoned; account info request may repeat"
                );
                true
            }
        }
    }

    pub(in crate::live_runtime) fn mark_startup_account_info_requested_at(
        &self,
        connected_at: Instant,
        requested_at: Instant,
    ) {
        match self.startup_account_info_requested_at.lock() {
            Ok(mut latest_request) => {
                *latest_request = Some(StartupAccountInfoRequest {
                    connected_at,
                    requested_at,
                });
            }
            Err(_) => tracing::warn!(
                account = %self.account,
                "startup Cofl account-info lock poisoned; account info request may repeat"
            ),
        }
    }

    pub(super) fn set_privacy_chat_regex(&self, pattern: &str) {
        match self.privacy_filter.lock() {
            Ok(mut filter) => {
                if let Err(error) = filter.update(pattern) {
                    tracing::warn!(
                        account = %self.account,
                        error = %error,
                        "ignoring invalid Cofl privacy chat regex"
                    );
                }
            }
            Err(_) => tracing::warn!(
                account = %self.account,
                "Cofl privacy filter lock poisoned; ignoring privacy settings"
            ),
        }
    }

    pub(in crate::live_runtime) fn chat_batch_upload_message_if_requested(
        &self,
        text: &str,
    ) -> Result<Option<String>> {
        let should_upload = self
            .privacy_filter
            .lock()
            .map_err(|_| anyhow::anyhow!("Cofl privacy filter lock poisoned"))?
            .matches(text);
        if !should_upload {
            return Ok(None);
        }
        Ok(Some(saf_cofl::encode_chat_batch(&[text.to_string()])?))
    }

    pub(in crate::live_runtime) async fn upload_chat_batch_if_requested(&self, text: &str) {
        let message = match self.chat_batch_upload_message_if_requested(text) {
            Ok(Some(message)) => message,
            Ok(None) => return,
            Err(error) => {
                tracing::warn!(
                    account = %self.account,
                    error = %error,
                    "failed to encode Cofl chat batch"
                );
                return;
            }
        };

        match self.send_raw_if_connected(&message).await {
            Ok(true) => {
                tracing::debug!(account = %self.account, "uploaded chat batch to Cofl websocket");
            }
            Ok(false) => {}
            Err(error) => tracing::warn!(
                account = %self.account,
                error = %error,
                "failed to upload chat batch to Cofl websocket"
            ),
        }
    }

    pub(in crate::live_runtime) async fn is_connected(&self) -> bool {
        self.state.lock().await.client.is_some()
    }

    pub(in crate::live_runtime) async fn disconnect_for_stop(&self) {
        self.state.lock().await.disconnect();
    }

    pub(in crate::live_runtime) async fn resume_after_stop(&self) {
        let mut state = self.state.lock().await;
        if state.client.is_none() {
            state.backoff.record_success();
        }
    }

    pub(super) async fn reconnect_silent_open(&self) -> Option<String> {
        self.state.lock().await.reconnect_silent_open(
            Instant::now(),
            COFL_SILENT_OPEN_TIMEOUT,
            COFL_REGION_BACKOFF,
        )
    }

    pub(super) async fn next_envelope(&self) -> Result<Option<saf_cofl::CoflEnvelope>> {
        let mut state = self.state.lock().await;
        let Some(client) = state.client.as_ref() else {
            return Ok(None);
        };

        match client.next_envelope().await {
            Ok(Some(envelope)) => {
                state.record_message();
                state.backoff.record_success();
                Ok(Some(envelope))
            }
            Ok(None) => {
                state.disconnect();
                Ok(None)
            }
            Err(error) => {
                state.disconnect();
                Err(error.into())
            }
        }
    }

    pub(in crate::live_runtime) async fn ensure_connected(&self, force: bool) -> Result<bool> {
        {
            let state = self.state.lock().await;
            if state.client.is_some() {
                return Ok(true);
            }
            let now = Instant::now();
            if !force && !state.backoff.should_attempt(now) {
                return Ok(false);
            }
        }

        let link = {
            let state = self.state.lock().await;
            state.link.clone()
        };
        let now = Instant::now();
        let connect = tokio::time::timeout(
            Duration::from_secs(5),
            saf_cofl::ws_client::CoflWebSocketClient::connect(self.account.clone(), link.as_str()),
        )
        .await;
        let mut state = self.state.lock().await;
        if state.client.is_some() {
            return Ok(true);
        }
        if state.link != link {
            return Ok(false);
        }
        match connect {
            Ok(Ok(client)) => {
                state.record_connected(client, now);
                self.mark_settings_unloaded();
                self.reset_startup_account_info_state();
                tracing::info!(
                    account = %self.account,
                    link = %saf_cofl::redact_cofl_socket_link(&link),
                    "connected Cofl websocket"
                );
                Ok(true)
            }
            Ok(Err(error)) => {
                state.backoff.record_failure(now);
                Err(error.into())
            }
            Err(_) => {
                state.backoff.record_failure(now);
                Err(anyhow::anyhow!("cofl websocket connect timed out"))
            }
        }
    }
}

#[async_trait]
impl CoflClient for LiveCoflClient {
    async fn send_command(&self, account: &AccountId, command: &str) -> Result<(), PortError> {
        if account != &self.account {
            return Err(PortError::Unavailable(format!(
                "cofl websocket is configured for {}, not {account}",
                self.account
            )));
        }

        if let Some(humanizer) = self.humanizer.as_ref() {
            let wait = humanizer.acquire_throttle_token();
            if !wait.is_zero() {
                tokio::time::sleep(wait).await;
            }
        }

        self.ensure_connected(true)
            .await
            .map_err(|error| PortError::Failed(error.to_string()))?;
        let mut state = self.state.lock().await;
        let Some(client) = state.client.as_ref() else {
            return Err(PortError::Unavailable(format!(
                "cofl websocket is not connected for {account}"
            )));
        };
        match client.send_command(account, command).await {
            Ok(()) => Ok(()),
            Err(error) => {
                state.disconnect();
                Err(error)
            }
        }
    }
}
