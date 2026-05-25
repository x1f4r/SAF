use async_trait::async_trait;
use saf_core::ports::{InventoryItem, PortError};
#[cfg(feature = "live-cofl")]
use std::collections::BTreeMap;
use std::sync::Arc;
#[cfg(feature = "live-cofl")]
use tokio::sync::Mutex as AsyncMutex;

#[async_trait]
pub(in crate::live_runtime) trait InventoryPriceLookup:
    Send + Sync
{
    async fn lookup(&self, item: &InventoryItem) -> Result<Option<f64>, PortError>;
}

pub(in crate::live_runtime) fn default_inventory_price_lookup()
-> Option<Arc<dyn InventoryPriceLookup>> {
    #[cfg(feature = "live-cofl")]
    {
        Some(Arc::new(CoflInventoryPriceLookup::default()))
    }
    #[cfg(not(feature = "live-cofl"))]
    {
        None
    }
}

#[cfg(feature = "live-cofl")]
struct CoflInventoryPriceLookup {
    client: saf_cofl::http_client::CoflHttpClient,
    cache: AsyncMutex<BTreeMap<String, Option<f64>>>,
}

#[cfg(feature = "live-cofl")]
impl Default for CoflInventoryPriceLookup {
    fn default() -> Self {
        Self {
            client: saf_cofl::http_client::CoflHttpClient::default(),
            cache: AsyncMutex::new(BTreeMap::new()),
        }
    }
}

#[cfg(feature = "live-cofl")]
#[async_trait]
impl InventoryPriceLookup for CoflInventoryPriceLookup {
    async fn lookup(&self, item: &InventoryItem) -> Result<Option<f64>, PortError> {
        let Some(tag) = item
            .tag
            .as_deref()
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
        else {
            return Ok(None);
        };
        {
            let cache = self.cache.lock().await;
            if let Some(price) = cache.get(tag) {
                return Ok(*price);
            }
        }
        let price = self.client.item_listing_price(tag).await?;
        self.cache.lock().await.insert(tag.to_string(), price);
        Ok(price)
    }
}
