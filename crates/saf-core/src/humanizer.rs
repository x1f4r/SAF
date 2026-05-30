//! Central, configurable model for human-plausible action timing, request
//! throttling, reconnect backoff, and session pacing.
//!
//! All knobs live in [`HumanizerConfig`] with defaults wired to behave as if
//! the bot were a relaxed human: bursty short patterns are avoided, the only
//! place latency matters is the buy/snipe action (target ~100–200 ms with a
//! profit-adaptive bias), and reconnects/server switches are spaced with
//! jittered cooldowns so cadence itself cannot be fingerprinted.
//!
//! The module is fully deterministic given an explicit RNG seed (see tests),
//! so behaviour can be replayed and statistically validated.

use rand::distributions::{Distribution, Uniform};
use rand::rngs::StdRng;
use rand::{Rng, RngCore, SeedableRng, thread_rng};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Configuration of the humanizer subsystem. Defaults reflect the values
/// described in the anti-ban hardening spec and should rarely need overriding.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct HumanizerConfig {
    /// Master switch. When false the humanizer collapses to its legacy
    /// deterministic shortest delays — useful for offline tests and replay.
    pub enabled: bool,

    /// Critical buy/snipe reaction window.
    pub buy: BuyTimingConfig,

    /// Default jittered delay applied to non-critical actions (menu nav,
    /// clicks, list flows). Modelled as a truncated normal.
    pub default_action: NormalDelayConfig,

    /// Per-click jitter for streams of clicks that previously fired every few
    /// milliseconds (e.g. timed bed clicks). Keeps clicks above the human
    /// floor (~80 ms) without losing the burst entirely.
    pub click_stream: NormalDelayConfig,

    /// Outbound rate limiting (commands, chat, requests).
    pub throttle: ThrottleConfig,

    /// Reconnect backoff with decorrelated jitter (replaces purely exponential
    /// backoff that produces a predictable retry cadence).
    pub backoff: BackoffConfig,

    /// Limits on how often the runtime may switch SkyCofl websocket regions.
    pub server_switch: ServerSwitchConfig,

    /// Session-length jitter applied to scheduled rest/flip rotations so the
    /// "12r:12f" cadence does not sit at exact hour boundaries.
    pub session: SessionConfig,

    /// Idle "anti-AFK" look/jump behaviour. Drives the avatar to occasionally
    /// glance around (and rarely hop in place) while running and not busy with
    /// a market action, so a frozen camera never gives the bot away.
    pub idle: IdleBehaviorConfig,
}

impl Default for HumanizerConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            buy: BuyTimingConfig::default(),
            default_action: NormalDelayConfig::default_action_jitter(),
            click_stream: NormalDelayConfig::default_click_stream(),
            throttle: ThrottleConfig::default(),
            backoff: BackoffConfig::default(),
            server_switch: ServerSwitchConfig::default(),
            session: SessionConfig::default(),
            idle: IdleBehaviorConfig::default(),
        }
    }
}

/// Critical-path buy/snipe timing. The sampled latency is always inside
/// `[floor_ms, ceiling_ms]` (default 100..=200 ms) and biased toward the fast
/// end of that window proportional to the flip's expected profit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BuyTimingConfig {
    /// Hard floor in milliseconds. Anything faster (<10 ms is the classic
    /// "instant bot" tell) is disallowed regardless of profit.
    pub floor_ms: u64,
    /// Hard ceiling in milliseconds. ~200 ms keeps the bot competitive but
    /// still inside the human reaction-time distribution.
    pub ceiling_ms: u64,
    /// Profit in coins at or above which the sampler biases all the way toward
    /// `floor_ms`. Defaults to 30M coins (the existing "turbo" threshold).
    pub turbo_profit_coins: f64,
    /// Profit in coins below which a flip is treated as marginal and biased
    /// toward the slow end of the window. Defaults to 1M coins.
    pub marginal_profit_coins: f64,
    /// Standard deviation (ms) of the normal jitter applied around the
    /// profit-derived target. Keeps consecutive snipes from clustering on the
    /// same millisecond.
    pub jitter_std_ms: f64,
}

impl Default for BuyTimingConfig {
    fn default() -> Self {
        Self {
            floor_ms: 100,
            ceiling_ms: 200,
            turbo_profit_coins: 30_000_000.0,
            marginal_profit_coins: 1_000_000.0,
            jitter_std_ms: 12.0,
        }
    }
}

/// Truncated-normal delay configuration. Sampling draws from
/// `Normal(mean_ms, std_ms)` and clamps into `[min_ms, max_ms]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NormalDelayConfig {
    pub mean_ms: f64,
    pub std_ms: f64,
    pub min_ms: u64,
    pub max_ms: u64,
}

impl Default for NormalDelayConfig {
    fn default() -> Self {
        Self::default_action_jitter()
    }
}

impl NormalDelayConfig {
    /// Defaults for non-critical UI actions (menu navigation, lookups). Wide
    /// distribution centred on a relaxed human pace so the cadence varies
    /// every session.
    pub fn default_action_jitter() -> Self {
        Self {
            mean_ms: 280.0,
            std_ms: 110.0,
            min_ms: 120,
            max_ms: 900,
        }
    }

    /// Defaults for click streams (e.g. timed bed sniping). Floor is 80 ms,
    /// which is faster than reaction time but consistent with mouse spam by a
    /// focused player.
    pub fn default_click_stream() -> Self {
        Self {
            mean_ms: 130.0,
            std_ms: 35.0,
            min_ms: 80,
            max_ms: 260,
        }
    }
}

/// Token bucket configuration. Limits outbound bursts so the bot never
/// hammers a server faster than `steady_per_sec`, with a small allowed burst.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ThrottleConfig {
    /// Steady-state requests per second. Defaults to a comfortable 2/s which
    /// is well under any realistic limit.
    pub steady_per_sec: f64,
    /// Maximum burst size; bucket capacity in tokens.
    pub burst: u32,
}

impl Default for ThrottleConfig {
    fn default() -> Self {
        Self {
            steady_per_sec: 2.0,
            burst: 4,
        }
    }
}

/// Decorrelated-jitter backoff configuration for reconnect/retry loops.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BackoffConfig {
    pub base_ms: u64,
    pub cap_ms: u64,
    /// Multiplier applied to the previous delay when sampling the next one.
    /// `3.0` matches the classic AWS "decorrelated jitter" recipe.
    pub factor: f64,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            base_ms: 5_000,
            cap_ms: 120_000,
            factor: 3.0,
        }
    }
}

/// Restrictions on how aggressively the bot may bounce between SkyCofl
/// regions/sockets. Prevents detectable "server hopping" patterns.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ServerSwitchConfig {
    /// Minimum gap between two switches in milliseconds.
    pub min_cooldown_ms: u64,
    /// Mean jitter applied to the cooldown.
    pub mean_jitter_ms: f64,
    /// Std dev of the cooldown jitter.
    pub std_jitter_ms: f64,
    /// Maximum switches allowed in `window_ms`.
    pub max_in_window: u32,
    pub window_ms: u64,
}

impl Default for ServerSwitchConfig {
    fn default() -> Self {
        Self {
            min_cooldown_ms: 60_000,
            mean_jitter_ms: 45_000.0,
            std_jitter_ms: 20_000.0,
            max_in_window: 4,
            window_ms: 30 * 60_000,
        }
    }
}

/// Session pacing — adds jitter on top of any configured rotation schedule so
/// "12 hours flip, 12 hours rest" is not literally observed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SessionConfig {
    /// ± jitter ratio applied to scheduled flip/rest durations (e.g. 0.15
    /// expands a 12 h leg to a uniform sample in [10.2 h, 13.8 h]).
    pub leg_jitter_ratio: f64,
    /// Hard floor (ms) on any leg after jitter — guarantees a minimum
    /// flip/rest length regardless of misconfiguration.
    pub min_leg_ms: u64,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            leg_jitter_ratio: 0.15,
            min_leg_ms: 5 * 60 * 1_000,
        }
    }
}

/// Idle anti-AFK behaviour. Defaults are deliberately gentle: a glance every
/// ~25–75 s, rotation deltas of a handful of degrees, and a rare in-place jump.
/// Nothing here ever produces positional translation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct IdleBehaviorConfig {
    /// Master switch for the idle look/jump task. On by default.
    pub enabled: bool,
    /// Minimum gap between two idle nudges in milliseconds.
    pub min_interval_ms: u64,
    /// Maximum gap between two idle nudges in milliseconds.
    pub max_interval_ms: u64,
    /// Maximum absolute yaw change applied per nudge, in degrees.
    pub max_yaw_delta_deg: f32,
    /// Maximum absolute pitch change applied per nudge, in degrees.
    pub max_pitch_delta_deg: f32,
    /// Probability in `[0, 1]` that a given idle nudge is a jump instead of a
    /// look. Kept small so the avatar mostly just glances around.
    pub jump_probability: f64,
}

impl Default for IdleBehaviorConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_interval_ms: 25_000,
            max_interval_ms: 75_000,
            max_yaw_delta_deg: 18.0,
            max_pitch_delta_deg: 8.0,
            jump_probability: 0.08,
        }
    }
}

/// A single idle anti-AFK gesture sampled by [`Humanizer::sample_idle_action`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum IdleAction {
    /// Rotate the view by a small relative yaw/pitch delta (degrees).
    Look { yaw_delta: f32, pitch_delta: f32 },
    /// Hop in place.
    Jump,
}

// =============================================================================
// Runtime sampler
// =============================================================================

/// Stateful sampler. Owns the RNG and a token bucket; thread-safe via interior
/// mutability so it can be shared as `Arc<Humanizer>` across the async runtime.
pub struct Humanizer {
    config: HumanizerConfig,
    rng: Mutex<StdRng>,
    bucket: Mutex<TokenBucket>,
    server_switch: Mutex<SwitchTracker>,
}

impl std::fmt::Debug for Humanizer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Humanizer")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Humanizer {
    /// Construct a humanizer seeded from the OS RNG. Production code path.
    pub fn new(config: HumanizerConfig) -> Self {
        let seed = thread_rng().next_u64();
        Self::seeded(config, seed)
    }

    /// Construct a humanizer with an explicit RNG seed for tests/replays.
    pub fn seeded(config: HumanizerConfig, seed: u64) -> Self {
        let bucket = TokenBucket::new(&config.throttle);
        Self {
            config,
            rng: Mutex::new(StdRng::seed_from_u64(seed)),
            bucket: Mutex::new(bucket),
            server_switch: Mutex::new(SwitchTracker::default()),
        }
    }

    pub fn config(&self) -> &HumanizerConfig {
        &self.config
    }

    /// Sample a buy/snipe reaction latency in milliseconds. `profit_coins`
    /// must be the expected coin profit of the flip; non-finite or negative
    /// values are treated as the marginal case.
    pub fn buy_reaction_ms(&self, profit_coins: f64) -> u64 {
        let buy = &self.config.buy;
        let floor = buy.floor_ms.max(1) as f64;
        let ceiling = buy.ceiling_ms.max(buy.floor_ms + 1) as f64;

        if !self.config.enabled {
            return buy.floor_ms;
        }

        let bias = profit_bias(
            profit_coins,
            buy.marginal_profit_coins,
            buy.turbo_profit_coins,
        );
        // bias in [0,1]: 0 = marginal → slow end; 1 = turbo → fast end
        let target = ceiling - (ceiling - floor) * bias;
        let sample = self.sample_normal(target, buy.jitter_std_ms);
        clamp_ms(sample, buy.floor_ms, buy.ceiling_ms)
    }

    /// Sample a default jittered delay for non-critical actions.
    pub fn default_action_delay(&self) -> Duration {
        self.sample_delay(&self.config.default_action)
    }

    /// Sample an inter-click delay for click-stream sequences.
    pub fn click_stream_delay(&self) -> Duration {
        self.sample_delay(&self.config.click_stream)
    }

    /// Sample a normal-distributed delay using a custom config.
    pub fn sample_delay(&self, config: &NormalDelayConfig) -> Duration {
        if !self.config.enabled {
            return Duration::from_millis(config.min_ms);
        }
        let sampled = self.sample_normal(config.mean_ms, config.std_ms);
        Duration::from_millis(clamp_ms(sampled, config.min_ms, config.max_ms))
    }

    /// Returns the time to wait before the next throttled outbound action.
    /// Zero means "go now". Repeated calls drain the token bucket.
    pub fn acquire_throttle_token(&self) -> Duration {
        if !self.config.enabled {
            return Duration::ZERO;
        }
        let now = Instant::now();
        let mut bucket = match self.bucket.lock() {
            Ok(guard) => guard,
            Err(_) => return Duration::ZERO,
        };
        bucket.acquire(now)
    }

    /// Sample the next reconnect delay given the previous delay. Pass
    /// `Duration::ZERO` for the first failure.
    pub fn next_backoff(&self, previous: Duration) -> Duration {
        let backoff = &self.config.backoff;
        let base = Duration::from_millis(backoff.base_ms.max(1));
        let cap = Duration::from_millis(backoff.cap_ms.max(backoff.base_ms));
        if !self.config.enabled {
            return base;
        }
        let previous_ms = previous.as_millis().min(u128::from(u64::MAX)) as u64;
        let upper = (previous_ms as f64 * backoff.factor).max(base.as_millis() as f64);
        let upper = upper.min(cap.as_millis() as f64);
        let lower = base.as_millis() as f64;
        if upper <= lower {
            return Duration::from_millis(upper as u64);
        }
        let sample = self.sample_uniform(lower, upper);
        Duration::from_millis(sample.round().clamp(0.0, u64::MAX as f64) as u64)
    }

    /// Decide whether the runtime is allowed to switch SkyCofl servers right
    /// now. Returns `None` if a switch is allowed (and records the switch
    /// time), or `Some(duration)` to wait before the next attempt.
    pub fn try_register_server_switch(&self, now: Instant) -> Result<(), Duration> {
        let switch_cfg = &self.config.server_switch;
        let mut tracker = match self.server_switch.lock() {
            Ok(guard) => guard,
            Err(_) => return Ok(()),
        };

        if let Some(last) = tracker.last {
            let elapsed = now.saturating_duration_since(last);
            let cooldown_ms = switch_cfg.min_cooldown_ms as f64
                + self.sample_normal(switch_cfg.mean_jitter_ms, switch_cfg.std_jitter_ms);
            let cooldown_ms = cooldown_ms.max(switch_cfg.min_cooldown_ms as f64) as u64;
            let cooldown = Duration::from_millis(cooldown_ms);
            if elapsed < cooldown {
                return Err(cooldown - elapsed);
            }
        }

        let window = Duration::from_millis(switch_cfg.window_ms.max(1));
        tracker
            .recent
            .retain(|earlier| now.saturating_duration_since(*earlier) < window);
        if tracker.recent.len() as u32 >= switch_cfg.max_in_window {
            let oldest = tracker.recent.first().copied().unwrap_or(now);
            let wait = window.saturating_sub(now.saturating_duration_since(oldest));
            return Err(wait.max(Duration::from_millis(switch_cfg.min_cooldown_ms)));
        }

        tracker.last = Some(now);
        tracker.recent.push(now);
        Ok(())
    }

    /// Apply session-leg jitter. Given an intended leg duration (e.g. "rest
    /// for 12 h"), return a jittered, floored duration the runtime should
    /// actually wait. Idempotent and stateless aside from the RNG.
    pub fn jitter_session_leg(&self, planned: Duration) -> Duration {
        if !self.config.enabled {
            return planned;
        }
        let session = &self.config.session;
        let planned_ms = planned.as_millis().min(u128::from(u64::MAX)) as u64 as f64;
        if planned_ms <= 0.0 {
            return planned;
        }
        let jitter_span = planned_ms * session.leg_jitter_ratio.max(0.0);
        let lower = (planned_ms - jitter_span).max(session.min_leg_ms as f64);
        let upper = planned_ms + jitter_span;
        let sample = self.sample_uniform(lower, upper.max(lower));
        Duration::from_millis(sample.max(0.0) as u64)
    }

    /// Sample the delay to wait before the next idle anti-AFK gesture. Uniform
    /// in `[min_interval_ms, max_interval_ms]`. When idle behaviour or the
    /// whole humanizer is disabled, returns the configured maximum so the
    /// caller still ticks slowly rather than busy-looping.
    pub fn next_idle_delay(&self) -> Duration {
        let idle = &self.config.idle;
        let min = idle.min_interval_ms.max(1);
        let max = idle.max_interval_ms.max(min);
        if !self.config.enabled || !idle.enabled {
            return Duration::from_millis(max);
        }
        let sample = self.sample_uniform(min as f64, max as f64);
        Duration::from_millis(sample.round().clamp(min as f64, max as f64) as u64)
    }

    /// Sample one idle anti-AFK gesture: usually a small relative look, rarely
    /// an in-place jump. Look deltas are symmetric around zero so the camera
    /// wanders gently rather than always drifting one way. Returns `None` when
    /// idle behaviour is disabled.
    pub fn sample_idle_action(&self) -> Option<IdleAction> {
        let idle = &self.config.idle;
        if !self.config.enabled || !idle.enabled {
            return None;
        }
        let jump_roll = self.sample_uniform(0.0, 1.0);
        if jump_roll < idle.jump_probability.clamp(0.0, 1.0) {
            return Some(IdleAction::Jump);
        }
        let yaw_span = idle.max_yaw_delta_deg.abs().max(0.0) as f64;
        let pitch_span = idle.max_pitch_delta_deg.abs().max(0.0) as f64;
        let yaw_delta = self.sample_uniform(-yaw_span, yaw_span) as f32;
        let pitch_delta = self.sample_uniform(-pitch_span, pitch_span) as f32;
        Some(IdleAction::Look {
            yaw_delta,
            pitch_delta,
        })
    }

    // ---- internal helpers ----

    fn sample_normal(&self, mean: f64, std: f64) -> f64 {
        if std <= 0.0 {
            return mean;
        }
        // Box–Muller transform using two uniform samples on (0, 1].
        let mut rng = match self.rng.lock() {
            Ok(guard) => guard,
            Err(_) => return mean,
        };
        let dist = Uniform::new_inclusive(f64::EPSILON, 1.0);
        let u1: f64 = dist.sample(&mut *rng);
        let u2: f64 = dist.sample(&mut *rng);
        let z = (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos();
        mean + z * std
    }

    fn sample_uniform(&self, lower: f64, upper: f64) -> f64 {
        let mut rng = match self.rng.lock() {
            Ok(guard) => guard,
            Err(_) => return (lower + upper) * 0.5,
        };
        if upper <= lower {
            return lower;
        }
        rng.gen_range(lower..=upper)
    }
}

/// Derive a stable, distinct RNG seed for a named account from a base seed.
///
/// Each account gets its OWN humanizer (own token bucket + server-switch
/// tracker) so one account can never drain another's budget. Seeding by
/// `base_seed XOR hash(account)` keeps the per-account streams independent
/// while remaining deterministic for a given base seed (used by tests/replay).
pub fn account_humanizer_seed(base_seed: u64, account: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    account.hash(&mut hasher);
    base_seed ^ hasher.finish()
}

fn profit_bias(profit_coins: f64, marginal: f64, turbo: f64) -> f64 {
    if !profit_coins.is_finite() || profit_coins <= marginal {
        return 0.0;
    }
    if profit_coins >= turbo {
        return 1.0;
    }
    let span = (turbo - marginal).max(f64::EPSILON);
    ((profit_coins - marginal) / span).clamp(0.0, 1.0)
}

fn clamp_ms(value: f64, min_ms: u64, max_ms: u64) -> u64 {
    let lower = min_ms as f64;
    let upper = max_ms.max(min_ms) as f64;
    value.clamp(lower, upper).round() as u64
}

// =============================================================================
// Token bucket
// =============================================================================

#[derive(Clone, Debug)]
struct TokenBucket {
    tokens: f64,
    capacity: f64,
    refill_per_sec: f64,
    last_refill: Instant,
}

impl TokenBucket {
    fn new(config: &ThrottleConfig) -> Self {
        let capacity = config.burst.max(1) as f64;
        Self {
            tokens: capacity,
            capacity,
            refill_per_sec: config.steady_per_sec.max(0.01),
            last_refill: Instant::now(),
        }
    }

    fn refill(&mut self, now: Instant) {
        let elapsed = now
            .saturating_duration_since(self.last_refill)
            .as_secs_f64();
        if elapsed <= 0.0 {
            return;
        }
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        self.last_refill = now;
    }

    fn acquire(&mut self, now: Instant) -> Duration {
        self.refill(now);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            return Duration::ZERO;
        }
        let deficit = 1.0 - self.tokens;
        let wait_secs = deficit / self.refill_per_sec;
        self.tokens = 0.0;
        // pretend the token has been spent at the future moment; the caller
        // will sleep `wait_secs` before actually performing the action.
        self.last_refill = now + Duration::from_secs_f64(wait_secs);
        Duration::from_secs_f64(wait_secs)
    }
}

#[derive(Clone, Debug, Default)]
struct SwitchTracker {
    last: Option<Instant>,
    recent: Vec<Instant>,
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn sampler(seed: u64) -> Humanizer {
        Humanizer::seeded(HumanizerConfig::default(), seed)
    }

    #[test]
    fn buy_reaction_stays_in_window_for_all_profit_levels() {
        let h = sampler(42);
        for profit in [
            0.0,
            500_000.0,
            5_000_000.0,
            50_000_000.0,
            1_000_000_000.0,
            f64::NAN,
            f64::INFINITY,
        ] {
            for _ in 0..1_000 {
                let ms = h.buy_reaction_ms(profit);
                assert!(
                    (100..=200).contains(&ms),
                    "profit {profit} yielded {ms} ms outside [100,200]"
                );
            }
        }
    }

    #[test]
    fn buy_reaction_biases_fast_for_high_profit() {
        let h = sampler(7);
        let marginal: u64 = (0..10_000).map(|_| h.buy_reaction_ms(0.0)).sum();
        let turbo: u64 = (0..10_000).map(|_| h.buy_reaction_ms(50_000_000.0)).sum();
        let marginal_mean = marginal as f64 / 10_000.0;
        let turbo_mean = turbo as f64 / 10_000.0;
        assert!(
            turbo_mean + 25.0 < marginal_mean,
            "expected turbo flips noticeably faster than marginal flips, got turbo={turbo_mean} marginal={marginal_mean}"
        );
        assert!(turbo_mean >= 100.0);
        assert!(marginal_mean <= 200.0);
    }

    #[test]
    fn default_action_delay_is_jittered_and_clamped() {
        let h = sampler(13);
        let mut hits = BTreeMap::new();
        for _ in 0..2_000 {
            let ms = h.default_action_delay().as_millis() as u64;
            assert!((120..=900).contains(&ms));
            *hits.entry(ms / 50).or_insert(0_u32) += 1;
        }
        assert!(
            hits.len() >= 4,
            "expected at least 4 distinct 50-ms buckets for jitter, got {}",
            hits.len()
        );
    }

    #[test]
    fn click_stream_floor_is_humanlike() {
        let h = sampler(99);
        for _ in 0..1_000 {
            let ms = h.click_stream_delay().as_millis() as u64;
            assert!(ms >= 80, "click below human floor: {ms}");
            assert!(ms <= 260);
        }
    }

    #[test]
    fn token_bucket_throttles_bursts() {
        let h = sampler(1);
        let mut zero = 0;
        let mut delayed = 0;
        for _ in 0..20 {
            if h.acquire_throttle_token().is_zero() {
                zero += 1;
            } else {
                delayed += 1;
            }
        }
        assert!(zero <= 5, "burst window {} exceeded burst limit", zero);
        assert!(delayed >= 15, "throttle should have slowed most calls");
    }

    #[test]
    fn backoff_uses_decorrelated_jitter() {
        let h = sampler(2);
        let mut prev = Duration::ZERO;
        let mut max = Duration::ZERO;
        for _ in 0..50 {
            let next = h.next_backoff(prev);
            assert!(next.as_millis() >= 5_000);
            assert!(next.as_millis() <= 120_000);
            if next > max {
                max = next;
            }
            prev = next;
        }
        // Confirms growth happens but is capped.
        assert!(max.as_secs() >= 5);
    }

    #[test]
    fn backoff_is_never_zero() {
        let h = sampler(3);
        for _ in 0..200 {
            let next = h.next_backoff(Duration::ZERO);
            assert!(next.as_millis() >= 5_000);
        }
    }

    #[test]
    fn server_switch_respects_cooldown_and_window() {
        let cfg = HumanizerConfig {
            server_switch: ServerSwitchConfig {
                min_cooldown_ms: 1_000,
                mean_jitter_ms: 0.0,
                std_jitter_ms: 0.0,
                max_in_window: 2,
                window_ms: 60_000,
            },
            ..HumanizerConfig::default()
        };
        let h = Humanizer::seeded(cfg, 4);

        let t0 = Instant::now();
        assert!(h.try_register_server_switch(t0).is_ok());
        assert!(h.try_register_server_switch(t0).is_err());
        let t1 = t0 + Duration::from_millis(1_500);
        assert!(h.try_register_server_switch(t1).is_ok());
        let t2 = t1 + Duration::from_millis(1_500);
        // Two recorded switches; window cap of 2 should block the third.
        assert!(h.try_register_server_switch(t2).is_err());
    }

    #[test]
    fn session_jitter_respects_min_leg() {
        let cfg = HumanizerConfig {
            session: SessionConfig {
                min_leg_ms: 60_000,
                leg_jitter_ratio: 0.5,
            },
            ..HumanizerConfig::default()
        };
        let h = Humanizer::seeded(cfg, 5);
        let planned = Duration::from_millis(120_000);
        for _ in 0..500 {
            let actual = h.jitter_session_leg(planned);
            assert!(actual >= Duration::from_millis(60_000));
            assert!(actual <= Duration::from_millis(180_000));
        }
    }

    #[test]
    fn idle_actions_respect_configured_bounds() {
        let h = sampler(123);
        let cfg = &h.config().idle;
        for _ in 0..5_000 {
            match h.sample_idle_action() {
                Some(IdleAction::Look {
                    yaw_delta,
                    pitch_delta,
                }) => {
                    assert!(yaw_delta.abs() <= cfg.max_yaw_delta_deg + 1e-3);
                    assert!(pitch_delta.abs() <= cfg.max_pitch_delta_deg + 1e-3);
                }
                Some(IdleAction::Jump) => {}
                None => panic!("idle enabled by default should always yield an action"),
            }
            let delay = h.next_idle_delay().as_millis() as u64;
            assert!((cfg.min_interval_ms..=cfg.max_interval_ms).contains(&delay));
        }
    }

    #[test]
    fn idle_action_can_produce_both_looks_and_jumps() {
        let h = sampler(321);
        let mut looks = 0;
        let mut jumps = 0;
        for _ in 0..10_000 {
            match h.sample_idle_action() {
                Some(IdleAction::Look { .. }) => looks += 1,
                Some(IdleAction::Jump) => jumps += 1,
                None => unreachable!(),
            }
        }
        assert!(
            jumps > 0,
            "expected at least one jump with default probability"
        );
        assert!(looks > jumps, "looks should dominate jumps");
    }

    #[test]
    fn disabled_idle_yields_no_action() {
        let cfg = HumanizerConfig {
            idle: IdleBehaviorConfig {
                enabled: false,
                ..IdleBehaviorConfig::default()
            },
            ..HumanizerConfig::default()
        };
        let h = Humanizer::seeded(cfg, 9);
        assert_eq!(h.sample_idle_action(), None);
        // Still returns a bounded (max) delay so callers tick rather than spin.
        assert_eq!(
            h.next_idle_delay(),
            Duration::from_millis(IdleBehaviorConfig::default().max_interval_ms)
        );
    }

    #[test]
    fn per_account_seeds_are_distinct_and_deterministic() {
        let a = account_humanizer_seed(42, "Main");
        let b = account_humanizer_seed(42, "Alt");
        assert_ne!(a, b, "different accounts must get different seeds");
        assert_eq!(a, account_humanizer_seed(42, "Main"), "seed is stable");
    }

    #[test]
    fn disabled_humanizer_returns_deterministic_lower_bounds() {
        let cfg = HumanizerConfig {
            enabled: false,
            ..HumanizerConfig::default()
        };
        let h = Humanizer::seeded(cfg, 0);
        assert_eq!(h.buy_reaction_ms(0.0), 100);
        assert_eq!(h.default_action_delay(), Duration::from_millis(120));
        assert_eq!(h.click_stream_delay(), Duration::from_millis(80));
        assert!(h.acquire_throttle_token().is_zero());
        assert_eq!(h.next_backoff(Duration::ZERO), Duration::from_millis(5_000));
    }
}
