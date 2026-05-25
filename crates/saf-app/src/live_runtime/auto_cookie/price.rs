use async_trait::async_trait;
use saf_core::ports::PortError;
use serde_json::Value;

#[async_trait]
pub(in crate::live_runtime) trait CookiePriceProvider: Send + Sync {
    async fn booster_cookie_buy_price(&self) -> std::result::Result<Option<f64>, PortError>;
}

#[derive(Clone, Default)]
pub(in crate::live_runtime) struct HypixelCookiePriceProvider {
    client: reqwest::Client,
}

#[async_trait]
impl CookiePriceProvider for HypixelCookiePriceProvider {
    async fn booster_cookie_buy_price(&self) -> std::result::Result<Option<f64>, PortError> {
        let response = self
            .client
            .get("https://api.hypixel.net/v2/skyblock/bazaar")
            .send()
            .await
            .map_err(|error| PortError::Failed(error.to_string()))?
            .error_for_status()
            .map_err(|error| PortError::Failed(error.to_string()))?
            .json::<Value>()
            .await
            .map_err(|error| PortError::Failed(error.to_string()))?;
        Ok(response
            .pointer("/products/BOOSTER_COOKIE/quick_status/buyPrice")
            .and_then(Value::as_f64)
            .filter(|price| price.is_finite() && *price > 0.0)
            .map(f64::round))
    }
}
