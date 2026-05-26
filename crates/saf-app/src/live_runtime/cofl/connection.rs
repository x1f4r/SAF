use saf_core::Humanizer;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub(in crate::live_runtime) struct LiveCoflClientState {
    pub(super) link: String,
    pub(super) default_link: String,
    pub(super) client: Option<saf_cofl::ws_client::CoflWebSocketClient>,
    pub(in crate::live_runtime) backoff: CoflReconnectBackoff,
    pub(super) connected_at: Option<Instant>,
    last_message_at: Option<Instant>,
    silent_watchdog: CoflSilentOpenWatchdog,
}

impl LiveCoflClientState {
    pub(super) fn new_with_humanizer(
        link: String,
        default_link: String,
        humanizer: Option<Arc<Humanizer>>,
    ) -> Self {
        let backoff = match humanizer {
            Some(humanizer) => CoflReconnectBackoff::with_humanizer(humanizer),
            None => CoflReconnectBackoff::new(Duration::from_secs(5), Duration::from_secs(60)),
        };
        Self {
            link,
            default_link,
            client: None,
            backoff,
            connected_at: None,
            last_message_at: None,
            silent_watchdog: CoflSilentOpenWatchdog::default(),
        }
    }

    pub(super) fn record_connected(
        &mut self,
        client: saf_cofl::ws_client::CoflWebSocketClient,
        now: Instant,
    ) {
        self.client = Some(client);
        self.connected_at = Some(now);
        self.last_message_at = None;
    }

    pub(super) fn record_message(&mut self) {
        self.last_message_at = Some(Instant::now());
        self.silent_watchdog.clear_count(&self.link);
    }

    pub(super) fn disconnect(&mut self) {
        self.client = None;
        self.connected_at = None;
        self.last_message_at = None;
        self.backoff.record_failure(Instant::now());
    }

    pub(super) fn has_received_message(&self) -> bool {
        self.last_message_at.is_some()
    }

    pub(super) fn defer_reconnect(&mut self, delay: Duration) {
        self.client = None;
        self.connected_at = None;
        self.last_message_at = None;
        self.backoff.record_deferred_attempt(Instant::now(), delay);
    }

    pub(super) fn is_region_backed_off(&self, link: &str, now: Instant) -> bool {
        self.silent_watchdog.is_region_backed_off(link, now)
    }

    pub(super) fn reconnect_silent_open(
        &mut self,
        now: Instant,
        timeout: Duration,
        region_backoff: Duration,
    ) -> Option<String> {
        if self.client.is_none()
            || self.last_message_at.is_some()
            || self
                .connected_at
                .is_none_or(|connected_at| now.duration_since(connected_at) < timeout)
        {
            return None;
        }
        let retry_link = self.silent_watchdog.retry_link_after_silent_open(
            &self.link,
            &self.default_link,
            now,
            region_backoff,
        );
        self.client = None;
        self.connected_at = None;
        self.last_message_at = None;
        self.link = retry_link.clone();
        self.backoff
            .record_deferred_attempt(now, Duration::from_secs(1));
        Some(retry_link)
    }
}

#[derive(Clone, Debug, Default)]
pub(in crate::live_runtime) struct CoflSilentOpenWatchdog {
    silent_open_counts: BTreeMap<String, u32>,
    region_backoff_until: BTreeMap<String, Instant>,
}

impl CoflSilentOpenWatchdog {
    pub(in crate::live_runtime) fn retry_link_after_silent_open(
        &mut self,
        link: &str,
        default_link: &str,
        now: Instant,
        region_backoff: Duration,
    ) -> String {
        let count = self.silent_open_counts.entry(link.to_string()).or_default();
        *count += 1;
        if *count >= 2 {
            self.backoff_region(link, now + region_backoff);
            default_link.to_string()
        } else {
            link.to_string()
        }
    }

    pub(in crate::live_runtime) fn clear_count(&mut self, link: &str) {
        self.silent_open_counts.remove(link);
    }

    pub(in crate::live_runtime) fn is_region_backed_off(&self, link: &str, now: Instant) -> bool {
        cofl_socket_host(link)
            .and_then(|host| self.region_backoff_until.get(&host).copied())
            .is_some_and(|until| until > now)
    }

    fn backoff_region(&mut self, link: &str, until: Instant) {
        let Some(host) = cofl_socket_host(link) else {
            return;
        };
        if host == "sky.coflnet.com" {
            return;
        }
        self.region_backoff_until.insert(host, until);
    }
}

pub(in crate::live_runtime) fn cofl_socket_host(link: &str) -> Option<String> {
    let url = url::Url::parse(link).ok()?;
    let mut host = url.host_str()?.to_string();
    if host.contains(':') && !host.starts_with('[') {
        host = format!("[{host}]");
    }
    if let Some(port) = url.port() {
        host.push(':');
        host.push_str(&port.to_string());
    }
    Some(host)
}

#[derive(Clone)]
pub(in crate::live_runtime) struct CoflReconnectBackoff {
    min_delay: Duration,
    max_delay: Duration,
    next_delay: Duration,
    pub(in crate::live_runtime) next_attempt: Option<Instant>,
    humanizer: Option<Arc<Humanizer>>,
}

impl std::fmt::Debug for CoflReconnectBackoff {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CoflReconnectBackoff")
            .field("min_delay", &self.min_delay)
            .field("max_delay", &self.max_delay)
            .field("next_delay", &self.next_delay)
            .field("next_attempt", &self.next_attempt)
            .field("humanizer", &self.humanizer.is_some())
            .finish()
    }
}

impl CoflReconnectBackoff {
    pub(in crate::live_runtime) fn new(min_delay: Duration, max_delay: Duration) -> Self {
        Self {
            min_delay,
            max_delay,
            next_delay: min_delay,
            next_attempt: None,
            humanizer: None,
        }
    }

    pub(in crate::live_runtime) fn with_humanizer(humanizer: Arc<Humanizer>) -> Self {
        let backoff = &humanizer.config().backoff;
        let min_delay = Duration::from_millis(backoff.base_ms.max(1));
        let max_delay = Duration::from_millis(backoff.cap_ms.max(backoff.base_ms));
        Self {
            min_delay,
            max_delay,
            next_delay: min_delay,
            next_attempt: None,
            humanizer: Some(humanizer),
        }
    }

    pub(in crate::live_runtime) fn should_attempt(&self, now: Instant) -> bool {
        self.next_attempt.is_none_or(|attempt| now >= attempt)
    }

    pub(in crate::live_runtime) fn record_success(&mut self) {
        self.next_delay = self.min_delay;
        self.next_attempt = None;
    }

    pub(in crate::live_runtime) fn record_failure(&mut self, now: Instant) {
        let scheduled = self.next_delay;
        self.next_attempt = Some(now + scheduled);
        let promoted = match &self.humanizer {
            Some(humanizer) => humanizer.next_backoff(scheduled),
            None => scheduled.saturating_mul(2).min(self.max_delay),
        };
        self.next_delay = promoted.min(self.max_delay);
    }

    pub(super) fn record_deferred_attempt(&mut self, now: Instant, delay: Duration) {
        self.next_delay = self.min_delay;
        self.next_attempt = Some(now + delay);
    }
}
