use super::notifier::flip_buy_kind;
use super::now_ms;
use super::support::{
    normalized_purchase_item_name, number_value, purchase_lookup_key, sold_listing_lookup_key,
    string_value,
};
use crate::state_cli::FileQueueStore;
use anyhow::Result;
use async_trait::async_trait;
use saf_core::ports::{PortError, TrackedFlip, TrackedFlipProvider};
use saf_core::{AccountId, FlipEvent};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

type TrackedFlipKey = (AccountId, String);
type PurchaseFlipKey = (AccountId, String, u64);
type SoldListingKey = (AccountId, String, u64);

#[derive(Clone, Debug, Default)]
pub(super) struct LiveTrackedFlipProvider {
    pub(super) flips: Arc<Mutex<BTreeMap<TrackedFlipKey, TrackedFlip>>>,
    pub(super) purchase_flips: Arc<Mutex<BTreeMap<PurchaseFlipKey, Vec<TrackedFlip>>>>,
    saved: Option<FileQueueStore>,
}

impl LiveTrackedFlipProvider {
    pub(super) fn with_saved(saved: FileQueueStore) -> Self {
        Self {
            flips: Arc::new(Mutex::new(BTreeMap::new())),
            purchase_flips: Arc::new(Mutex::new(BTreeMap::new())),
            saved: Some(saved),
        }
    }

    #[cfg_attr(not(feature = "live-cofl"), allow(dead_code))]
    pub(super) fn record_flip(
        &self,
        account: &AccountId,
        flip: &FlipEvent,
    ) -> Result<(), PortError> {
        let Some(auction_id) = &flip.auction_id else {
            return Ok(());
        };
        if !flip.target.is_finite() || flip.target < 500.0 {
            return Ok(());
        }
        let tracked = TrackedFlip {
            auction_id: auction_id.to_string(),
            target_price: flip.target,
            weird_item_name: Some(flip.weird_item_name.clone()),
            tag: flip.tag.clone(),
            price_paid: flip.starting_bid.is_finite().then_some(flip.starting_bid),
            finder: Some(flip.finder.clone()),
            volume: flip.volume.is_finite().then_some(flip.volume),
            profit_percentage: flip
                .profit_percentage
                .is_finite()
                .then_some(flip.profit_percentage),
            buy_kind: Some(flip_buy_kind(flip).to_string()),
            seen_at_ms: Some(now_ms()),
        };
        self.flips
            .lock()
            .map_err(|_| PortError::Failed("tracked flip lock poisoned".to_string()))?
            .insert((account.clone(), auction_id.to_string()), tracked.clone());
        if let Some((item, price)) = purchase_lookup_key(&flip.item_name, flip.starting_bid) {
            self.purchase_flips
                .lock()
                .map_err(|_| PortError::Failed("tracked flip lock poisoned".to_string()))?
                .entry((account.clone(), item, price))
                .or_default()
                .push(tracked);
        }
        Ok(())
    }

    pub(super) fn take_purchase(
        &self,
        account: &AccountId,
        item_name: &str,
        price: u64,
    ) -> Result<Option<TrackedFlip>, PortError> {
        let item_name = normalized_purchase_item_name(item_name);
        if item_name.is_empty() || price == 0 {
            return Ok(None);
        }
        let key = (account.clone(), item_name, price);
        let mut flips = self
            .purchase_flips
            .lock()
            .map_err(|_| PortError::Failed("tracked flip lock poisoned".to_string()))?;
        let Some(mut candidates) = flips.remove(&key) else {
            return Ok(None);
        };
        let matched = candidates.first().cloned();
        if !candidates.is_empty() {
            candidates.remove(0);
        }
        if !candidates.is_empty() {
            flips.insert(key, candidates);
        }
        Ok(matched)
    }
}

#[async_trait]
impl TrackedFlipProvider for LiveTrackedFlipProvider {
    async fn lookup(
        &self,
        account: &AccountId,
        auction_id: &str,
    ) -> Result<Option<TrackedFlip>, PortError> {
        let live = self
            .flips
            .lock()
            .map_err(|_| PortError::Failed("tracked flip lock poisoned".to_string()))?
            .get(&(account.clone(), auction_id.to_string()))
            .cloned();
        if live.is_some() {
            return Ok(live);
        }
        if let Some(saved) = &self.saved {
            return saved.lookup(account, auction_id).await;
        }
        Ok(None)
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct LiveSoldTracker {
    listings: Arc<Mutex<BTreeMap<SoldListingKey, SoldListingMetadata>>>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct SoldListingMetadata {
    pub(super) auction_id: String,
    pub(super) expected_collected: u64,
    pub(super) profit: f64,
    pub(super) tag: Option<String>,
}

impl LiveSoldTracker {
    pub(super) fn record_listing(
        &self,
        account: &AccountId,
        action: &Value,
    ) -> Result<(), PortError> {
        let Some(list_price) = number_value(action, &["price"]) else {
            return Ok(());
        };
        let Some((item_name, collected_price)) = sold_listing_lookup_key(action, list_price) else {
            return Ok(());
        };
        let auction_id = string_value(action, &["auctionID", "auctionId", "auction_id"])
            .unwrap_or_else(|| item_name.clone());
        let metadata = SoldListingMetadata {
            auction_id,
            expected_collected: collected_price,
            profit: number_value(action, &["profit"]).unwrap_or(0.0),
            tag: string_value(action, &["tag"]),
        };
        self.listings
            .lock()
            .map_err(|_| PortError::Failed("sold tracker lock poisoned".to_string()))?
            .insert((account.clone(), item_name, collected_price), metadata);
        Ok(())
    }

    pub(super) fn take_claim(
        &self,
        account: &AccountId,
        item_name: &str,
        collected_price: u64,
    ) -> Result<Option<SoldListingMetadata>, PortError> {
        let item_name = normalized_purchase_item_name(item_name);
        if item_name.is_empty() || collected_price == 0 {
            return Ok(None);
        }
        let mut listings = self
            .listings
            .lock()
            .map_err(|_| PortError::Failed("sold tracker lock poisoned".to_string()))?;
        let exact_key = (account.clone(), item_name.clone(), collected_price);
        if let Some(metadata) = listings.remove(&exact_key) {
            return Ok(Some(metadata));
        }

        let closest_key = listings
            .keys()
            .filter(|(entry_account, entry_item, _)| {
                entry_account == account && entry_item == &item_name
            })
            .min_by_key(|(_, _, price)| price.abs_diff(collected_price))
            .cloned();
        Ok(closest_key.and_then(|key| listings.remove(&key)))
    }
}
