mod plan;
mod price;
mod runtime;

pub(super) const COOKIE_PURCHASE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
pub(super) const COOKIE_ACTIVATION_DELAY: std::time::Duration =
    std::time::Duration::from_millis(250);
pub(super) const COOKIE_DURATION: std::time::Duration =
    std::time::Duration::from_secs(4 * 24 * 60 * 60);
pub(super) const MAX_COOKIE_PRICE: f64 = 20_000_000.0;

#[cfg(test)]
pub(super) use plan::AutoCookiePhase;
pub(super) use plan::PendingAutoCookie;
pub(super) use price::{CookiePriceProvider, HypixelCookiePriceProvider};
