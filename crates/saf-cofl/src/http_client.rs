use async_trait::async_trait;
use saf_core::inventory_listing_price;
use saf_core::ports::{AuctionMetadata, AuctionMetadataProvider, PortError};
use serde::Deserialize;

const DEFAULT_COFL_API_BASE: &str = "https://sky.coflnet.com/api";

#[derive(Clone)]
pub struct CoflHttpClient {
    client: reqwest::Client,
    base_url: String,
}

impl Default for CoflHttpClient {
    fn default() -> Self {
        Self::new(DEFAULT_COFL_API_BASE)
    }
}

impl CoflHttpClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    pub async fn item_listing_price(&self, tag: &str) -> Result<Option<f64>, PortError> {
        let tag = tag.trim();
        if tag.is_empty() {
            return Ok(None);
        }

        let price_url = format!("{}/item/price/{}", self.base_url, tag);
        let bin_url = format!("{}/item/price/{}/bin", self.base_url, tag);
        let price = self
            .client
            .get(price_url)
            .send()
            .await
            .map_err(|error| PortError::Failed(error.to_string()))?;
        if price.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let price = price
            .error_for_status()
            .map_err(|error| PortError::Failed(error.to_string()))?
            .json::<CoflItemPriceResponse>()
            .await
            .map_err(|error| PortError::Failed(error.to_string()))?;
        let bin = self
            .client
            .get(bin_url)
            .send()
            .await
            .map_err(|error| PortError::Failed(error.to_string()))?;
        if bin.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let bin = bin
            .error_for_status()
            .map_err(|error| PortError::Failed(error.to_string()))?
            .json::<CoflItemBinResponse>()
            .await
            .map_err(|error| PortError::Failed(error.to_string()))?;
        Ok(inventory_listing_price(
            price.median.unwrap_or_default(),
            price.volume.unwrap_or_default() / 5.0,
            bin.lowest.unwrap_or_default(),
        ))
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoflAuctionResponse {
    #[serde(default)]
    item_name: Option<String>,
    #[serde(default)]
    starting_bid: Option<f64>,
    #[serde(default)]
    tag: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoflItemPriceResponse {
    #[serde(default)]
    median: Option<f64>,
    #[serde(default)]
    volume: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoflItemBinResponse {
    #[serde(default)]
    lowest: Option<f64>,
}

#[async_trait]
impl AuctionMetadataProvider for CoflHttpClient {
    async fn lookup(&self, auction_id: &str) -> Result<Option<AuctionMetadata>, PortError> {
        let auction_id = auction_id.trim();
        if auction_id.is_empty() {
            return Ok(None);
        }

        let url = format!("{}/auction/{}", self.base_url, auction_id);
        let response = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|error| PortError::Failed(error.to_string()))?;
        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let auction = response
            .error_for_status()
            .map_err(|error| PortError::Failed(error.to_string()))?
            .json::<CoflAuctionResponse>()
            .await
            .map_err(|error| PortError::Failed(error.to_string()))?;

        Ok(Some(AuctionMetadata {
            auction_id: auction_id.to_string(),
            item_name: auction.item_name,
            starting_bid: auction.starting_bid,
            tag: auction.tag,
        }))
    }
}
