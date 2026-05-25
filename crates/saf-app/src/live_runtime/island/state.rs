use super::super::STARTUP_PROFILE_SCAN_TIMEOUT;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub(in crate::live_runtime) struct LiveIslandState {
    pub(in crate::live_runtime) use_cookie: bool,
    pub(in crate::live_runtime) visit_friend: Option<String>,
    pub(in crate::live_runtime) visit_pending: bool,
    pub(in crate::live_runtime) last_scoreboard_lines: Vec<String>,
    pub(in crate::live_runtime) require_ready_for_market: bool,
    pub(in crate::live_runtime) base_message: String,
    pub(in crate::live_runtime) locraw_delay: Duration,
    pub(in crate::live_runtime) bad_mod_backoff: Duration,
    pub(in crate::live_runtime) locraw_due: Option<Instant>,
    pub(in crate::live_runtime) awaiting_locraw: bool,
    pub(in crate::live_runtime) locraw_requested_at: Option<Instant>,
    pub(in crate::live_runtime) locraw_timeout_count: u8,
    pub(in crate::live_runtime) ready: bool,
    pub(in crate::live_runtime) startup_reconcile_queued: bool,
    pub(in crate::live_runtime) startup_profile_scan_due: Option<Instant>,
    pub(in crate::live_runtime) startup_profile_scan_requested: bool,
    pub(in crate::live_runtime) startup_profile_scan_started_at: Option<Instant>,
    pub(in crate::live_runtime) startup_profile_scan_complete: bool,
    pub(in crate::live_runtime) startup_cookie_scan_due: Option<Instant>,
    pub(in crate::live_runtime) startup_cookie_scan_requested: bool,
    pub(in crate::live_runtime) startup_cookie_scan_started_at: Option<Instant>,
    pub(in crate::live_runtime) startup_cookie_scan_complete: bool,
    pub(in crate::live_runtime) startup_ready_notified: bool,
    pub(in crate::live_runtime) bad_mod_backoff_until: Option<Instant>,
    pub(in crate::live_runtime) movement_backoff_until: Option<Instant>,
}

impl LiveIslandState {
    pub(in crate::live_runtime) fn new(
        use_cookie: bool,
        visit_friend: String,
        require_ready_for_market: bool,
        locraw_delay: Duration,
        bad_mod_backoff: Duration,
    ) -> Self {
        let visit_friend = use_cookie
            .then(|| visit_friend.trim().to_string())
            .filter(|value| !value.is_empty());
        Self {
            use_cookie,
            visit_friend,
            visit_pending: false,
            last_scoreboard_lines: Vec::new(),
            require_ready_for_market,
            base_message: if use_cookie { "Private Island" } else { "Hub" }.to_string(),
            locraw_delay,
            bad_mod_backoff,
            locraw_due: None,
            awaiting_locraw: false,
            locraw_requested_at: None,
            locraw_timeout_count: 0,
            ready: !require_ready_for_market,
            startup_reconcile_queued: false,
            startup_profile_scan_due: None,
            startup_profile_scan_requested: false,
            startup_profile_scan_started_at: None,
            startup_profile_scan_complete: false,
            startup_cookie_scan_due: None,
            startup_cookie_scan_requested: false,
            startup_cookie_scan_started_at: None,
            startup_cookie_scan_complete: false,
            startup_ready_notified: false,
            bad_mod_backoff_until: None,
            movement_backoff_until: None,
        }
    }

    pub(in crate::live_runtime) fn schedule_locraw(&mut self, now: Instant) {
        if self.bad_modification_backoff_active(now)
            || self.movement_backoff_active(now)
            || self.awaiting_locraw
        {
            return;
        }
        self.locraw_due = Some(now + self.locraw_delay);
    }

    pub(in crate::live_runtime) fn take_due_locraw(&mut self, now: Instant) -> bool {
        if self.bad_modification_backoff_active(now) || self.movement_backoff_active(now) {
            return false;
        }
        if self.locraw_due.is_some_and(|due| due <= now) {
            self.locraw_due = None;
            self.awaiting_locraw = true;
            self.locraw_requested_at = Some(now);
            return true;
        }
        false
    }

    pub(in crate::live_runtime) fn finish_locraw(&mut self) {
        self.awaiting_locraw = false;
        self.locraw_requested_at = None;
        self.locraw_timeout_count = 0;
        self.locraw_due = None;
    }

    pub(in crate::live_runtime) fn locraw_response_timed_out(
        &self,
        now: Instant,
        timeout: Duration,
    ) -> bool {
        self.awaiting_locraw
            && self
                .locraw_requested_at
                .is_some_and(|requested| now.duration_since(requested) >= timeout)
    }

    pub(in crate::live_runtime) fn retry_locraw_after_timeout(&mut self, now: Instant) -> u8 {
        self.awaiting_locraw = false;
        self.locraw_requested_at = None;
        self.locraw_due = Some(now);
        self.locraw_timeout_count = self.locraw_timeout_count.saturating_add(1);
        self.locraw_timeout_count
    }

    pub(in crate::live_runtime) fn mark_moving(&mut self) {
        self.ready = false;
        self.awaiting_locraw = false;
        self.locraw_requested_at = None;
        self.locraw_timeout_count = 0;
        self.startup_reconcile_queued = false;
        self.startup_ready_notified = false;
        self.movement_backoff_until = None;
        self.reset_startup_profile_scan();
    }

    pub(in crate::live_runtime) fn mark_ready(&mut self) -> bool {
        let became_ready = !self.ready;
        self.ready = true;
        self.visit_pending = false;
        self.awaiting_locraw = false;
        self.locraw_requested_at = None;
        self.locraw_timeout_count = 0;
        self.locraw_due = None;
        self.movement_backoff_until = None;
        became_ready
    }

    pub(in crate::live_runtime) fn mark_disconnected(&mut self) {
        self.ready = !self.require_ready_for_market;
        self.visit_pending = false;
        self.awaiting_locraw = false;
        self.locraw_requested_at = None;
        self.locraw_timeout_count = 0;
        self.locraw_due = None;
        self.startup_reconcile_queued = false;
        self.startup_ready_notified = false;
        self.movement_backoff_until = None;
        self.reset_startup_profile_scan();
    }

    pub(in crate::live_runtime) fn take_startup_reconcile_request(&mut self) -> bool {
        if self.startup_reconcile_queued {
            return false;
        }
        self.startup_reconcile_queued = true;
        true
    }

    pub(in crate::live_runtime) fn schedule_startup_profile_scan(&mut self, now: Instant) {
        if !self.use_cookie
            || self.startup_profile_scan_complete
            || self.startup_profile_scan_requested
            || self.startup_profile_scan_due.is_some()
        {
            return;
        }
        self.startup_profile_scan_due = Some(now);
    }

    pub(in crate::live_runtime) fn take_due_startup_profile_scan(&mut self, now: Instant) -> bool {
        if self.startup_profile_scan_due.is_some_and(|due| due <= now) {
            self.startup_profile_scan_due = None;
            self.startup_profile_scan_requested = true;
            self.startup_profile_scan_started_at = Some(now);
            return true;
        }
        false
    }

    pub(in crate::live_runtime) fn finish_startup_profile_scan(&mut self) -> bool {
        if !self.startup_profile_scan_requested {
            return false;
        }
        self.startup_profile_scan_requested = false;
        self.startup_profile_scan_started_at = None;
        self.startup_profile_scan_complete = true;
        true
    }

    pub(in crate::live_runtime) fn reset_startup_profile_scan(&mut self) {
        self.startup_profile_scan_due = None;
        self.startup_profile_scan_requested = false;
        self.startup_profile_scan_started_at = None;
        self.startup_profile_scan_complete = false;
        self.reset_startup_cookie_scan();
    }

    pub(in crate::live_runtime) fn expire_startup_profile_scan(&mut self, now: Instant) -> bool {
        if !self.startup_profile_scan_requested
            || self
                .startup_profile_scan_started_at
                .is_none_or(|started| now.duration_since(started) < STARTUP_PROFILE_SCAN_TIMEOUT)
        {
            return false;
        }
        self.startup_profile_scan_requested = false;
        self.startup_profile_scan_started_at = None;
        self.startup_profile_scan_complete = true;
        true
    }

    pub(in crate::live_runtime) fn take_startup_ready_notification_request(&mut self) -> bool {
        if self.startup_ready_notified
            || !self.ready
            || self.startup_profile_scan_due.is_some()
            || self.startup_profile_scan_requested
            || self.startup_cookie_scan_due.is_some()
            || self.startup_cookie_scan_requested
        {
            return false;
        }
        self.startup_ready_notified = true;
        true
    }

    pub(in crate::live_runtime) fn mark_bad_modification(&mut self, now: Instant) {
        self.bad_mod_backoff_until = Some(now + self.bad_mod_backoff);
        self.mark_disconnected();
    }

    pub(in crate::live_runtime) fn bad_modification_backoff_active(&self, now: Instant) -> bool {
        self.bad_mod_backoff_until.is_some_and(|until| until > now)
    }

    pub(in crate::live_runtime) fn mark_movement_backoff(
        &mut self,
        now: Instant,
        duration: Duration,
    ) {
        self.movement_backoff_until = Some(now + duration);
        self.locraw_due = None;
        self.awaiting_locraw = false;
        self.locraw_requested_at = None;
    }

    pub(in crate::live_runtime) fn movement_backoff_active(&self, now: Instant) -> bool {
        self.movement_backoff_until.is_some_and(|until| until > now)
    }

    pub(in crate::live_runtime) fn should_visit_friend(&self) -> bool {
        saf_core::island::should_visit_friend(
            &self.last_scoreboard_lines,
            self.use_cookie,
            self.visit_friend.as_deref(),
        ) && !self.visit_pending
    }

    pub(in crate::live_runtime) fn record_scoreboard(&mut self, lines: &[String]) -> bool {
        self.last_scoreboard_lines = lines.to_vec();
        saf_core::island::scoreboard_matches_target_island(
            lines,
            self.use_cookie,
            self.visit_friend.as_deref(),
        )
    }

    pub(in crate::live_runtime) fn mark_visit_pending(&mut self) -> Option<String> {
        let friend = self.visit_friend.as_deref()?.trim();
        if friend.is_empty() {
            return None;
        }
        self.ready = false;
        self.visit_pending = true;
        Some(format!("/visit {friend}"))
    }

    pub(in crate::live_runtime) fn mark_visit_confirmed(&mut self, now: Instant) {
        self.visit_pending = false;
        self.mark_moving();
        self.schedule_locraw(now);
    }

    pub(in crate::live_runtime) fn disable_visit_friend(&mut self) {
        self.visit_friend = None;
        self.visit_pending = false;
    }

    pub(in crate::live_runtime) fn schedule_startup_cookie_scan(&mut self, now: Instant) {
        if !self.use_cookie
            || self.startup_cookie_scan_complete
            || self.startup_cookie_scan_requested
            || self.startup_cookie_scan_due.is_some()
        {
            return;
        }
        self.startup_cookie_scan_due = Some(now);
    }

    pub(in crate::live_runtime) fn take_due_startup_cookie_scan(&mut self, now: Instant) -> bool {
        if self.startup_cookie_scan_due.is_some_and(|due| due <= now) {
            self.startup_cookie_scan_due = None;
            self.startup_cookie_scan_requested = true;
            self.startup_cookie_scan_started_at = Some(now);
            return true;
        }
        false
    }

    pub(in crate::live_runtime) fn finish_startup_cookie_scan(&mut self) -> bool {
        if !self.startup_cookie_scan_requested {
            return false;
        }
        self.startup_cookie_scan_requested = false;
        self.startup_cookie_scan_started_at = None;
        self.startup_cookie_scan_complete = true;
        true
    }

    pub(in crate::live_runtime) fn reset_startup_cookie_scan(&mut self) {
        self.startup_cookie_scan_due = None;
        self.startup_cookie_scan_requested = false;
        self.startup_cookie_scan_started_at = None;
        self.startup_cookie_scan_complete = false;
    }

    pub(in crate::live_runtime) fn expire_startup_cookie_scan(&mut self, now: Instant) -> bool {
        if !self.startup_cookie_scan_requested
            || self
                .startup_cookie_scan_started_at
                .is_none_or(|started| now.duration_since(started) < STARTUP_PROFILE_SCAN_TIMEOUT)
        {
            return false;
        }
        self.startup_cookie_scan_requested = false;
        self.startup_cookie_scan_started_at = None;
        self.startup_cookie_scan_complete = true;
        true
    }

    pub(in crate::live_runtime) fn allows_market_work(&self) -> bool {
        (!self.require_ready_for_market || self.ready)
            && self.startup_profile_scan_due.is_none()
            && !self.startup_profile_scan_requested
            && self.startup_cookie_scan_due.is_none()
            && !self.startup_cookie_scan_requested
    }
}
