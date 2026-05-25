use crate::auction::{
    ListingDurationRule, PriceRule, calc_duration_visual, fallback_from_config_tail,
    rounded_listing_price,
};
use crate::config::SafConfig;
use crate::ids::{AuctionId, ItemUuid};
use crate::numbers::parse_compact_number;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelistPlan {
    pub auction_id: AuctionId,
    pub list_price: u64,
    pub listing_hours: f64,
    pub duration_visual: String,
    pub profit: f64,
    pub item_name: String,
    pub tag: Option<String>,
    pub item_uuid: Option<ItemUuid>,
}

impl RelistPlan {
    pub fn from_purchase(config: &SafConfig, purchase: RelistPurchase) -> Option<Self> {
        let listing_hours = listing_hours_for_price(config, purchase.target_price);
        let list_price = if purchase.override_price {
            purchase.target_price.round().max(1.0) as u64
        } else {
            let percent = price_cut_for_price(config, purchase.target_price);
            rounded_listing_price(purchase.target_price, percent, config.round_to)?
        };

        Some(Self {
            auction_id: purchase.auction_id,
            list_price,
            listing_hours,
            duration_visual: calc_duration_visual(listing_hours),
            profit: if purchase.profit.is_finite() {
                purchase.profit
            } else {
                0.0
            },
            item_name: purchase.item_name,
            tag: purchase.tag,
            item_uuid: purchase.item_uuid,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RelistPurchase {
    pub auction_id: AuctionId,
    pub target_price: f64,
    pub profit: f64,
    pub item_name: String,
    pub tag: Option<String>,
    pub item_uuid: Option<ItemUuid>,
    pub override_price: bool,
}

pub fn price_cut_for_price(config: &SafConfig, price: f64) -> f64 {
    let fallback = fallback_from_config_tail(&config.percent_of_target, 100.0);
    let rules = PriceRule::from_flat_config(&config.percent_of_target);
    PriceRule::percent_for_price(&rules, price, fallback)
}

pub fn listing_hours_for_price(config: &SafConfig, price: f64) -> f64 {
    let fallback = fallback_from_config_tail(&config.list_hours, 48.0);
    let rules = ListingDurationRule::from_flat_config(&config.list_hours);
    ListingDurationRule::hours_for_price(&rules, price, fallback)
}

pub fn round_listing_number(number: f64, precision: u32) -> Option<f64> {
    if !number.is_finite() {
        return None;
    }
    if precision == 0 {
        return Some(number);
    }
    let rounding_factor = 10_u64.saturating_pow(precision.saturating_sub(1)) as f64;
    if rounding_factor <= 1.0 {
        return Some(number);
    }
    let rounded = (number / rounding_factor).round() * rounding_factor;
    Some(if rounded == 0.0 { number } else { rounded })
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ExpiredRelistMode {
    Nbt,
    Percent(f64),
    Unknown(String),
}

impl ExpiredRelistMode {
    pub fn parse(value: &str) -> Self {
        if value == "1" {
            return Self::Nbt;
        }

        if let Some(percent) = value
            .strip_prefix("2:")
            .and_then(|value| value.parse::<f64>().ok())
            .map(|value| value / 100.0)
            .filter(|value| value.is_finite() && *value > 0.0)
        {
            return Self::Percent(percent);
        }

        Self::Unknown(value.to_string())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExpiredRelistPricing {
    pub mode: ExpiredRelistMode,
    pub round_to: u32,
    pub percent_rules: Vec<serde_json::Value>,
}

impl ExpiredRelistPricing {
    pub fn from_config(config: &SafConfig) -> Self {
        Self {
            mode: ExpiredRelistMode::parse(&config.do_not_relist.relist_mode),
            round_to: config.round_to,
            percent_rules: config.percent_of_target.clone(),
        }
    }

    pub fn resolve(&self, old_price: f64, nbt_price: Result<f64, String>) -> Result<f64, String> {
        match self.mode {
            ExpiredRelistMode::Nbt => nbt_price
                .ok()
                .filter(|price| price.is_finite() && *price >= 500.0)
                .or_else(|| self.fallback_from_old_price(old_price))
                .ok_or_else(|| "no valid NBT or old-price fallback".to_string()),
            ExpiredRelistMode::Percent(percent) => Ok(old_price * percent),
            ExpiredRelistMode::Unknown(_) => nbt_price
                .ok()
                .filter(|price| price.is_finite() && *price >= 500.0)
                .or_else(|| self.fallback_from_old_price(old_price))
                .ok_or_else(|| "no valid NBT or old-price fallback".to_string()),
        }
    }

    fn fallback_from_old_price(&self, old_price: f64) -> Option<f64> {
        if !old_price.is_finite() || old_price < 500.0 {
            return None;
        }
        let fallback = fallback_from_config_tail(&self.percent_rules, 100.0);
        let rules = PriceRule::from_flat_config(&self.percent_rules);
        let percent = PriceRule::percent_for_price(&rules, old_price, fallback);
        round_listing_number(old_price * percent / 100.0, self.round_to)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExpiredRelistQueueAction {
    pub price: u64,
    pub auction_id: String,
    pub username: String,
    pub inventory_uuid: String,
    pub item_name: String,
    pub tag: Option<String>,
    pub price_paid: u64,
    pub old_price: u64,
}

impl ExpiredRelistQueueAction {
    pub fn new(
        username: impl Into<String>,
        item_uuid: &ItemUuid,
        item_name: impl Into<String>,
        tag: Option<String>,
        price: f64,
        old_price: f64,
    ) -> Option<Self> {
        if !price.is_finite() || price < 500.0 || !old_price.is_finite() {
            return None;
        }
        Some(Self {
            price: price.round() as u64,
            auction_id: item_uuid.to_string(),
            username: username.into(),
            inventory_uuid: item_uuid.to_string(),
            item_name: item_name.into(),
            tag,
            price_paid: 0,
            old_price: old_price.max(0.0).round() as u64,
        })
    }
}

pub fn parse_old_price_from_lore_line(line: &str) -> Option<f64> {
    let cleaned = line
        .chars()
        .filter(|ch| {
            ch.is_ascii_digit()
                || matches!(
                    ch,
                    '.' | ',' | '_' | 'k' | 'K' | 'm' | 'M' | 'b' | 'B' | 't' | 'T'
                )
        })
        .collect::<String>();
    parse_compact_number(&cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SafConfig;

    #[test]
    fn expired_pricing_uses_nbt_or_percent_cut() {
        let mut config = SafConfig::default();
        config.do_not_relist.relist_mode = "1".to_string();
        let pricing = ExpiredRelistPricing::from_config(&config);
        assert_eq!(
            pricing.resolve(36_400_000.0, Ok(12_345_678.0)).unwrap(),
            12_345_678.0
        );

        config.do_not_relist.relist_mode = "2:97".to_string();
        let pricing = ExpiredRelistPricing::from_config(&config);
        assert_eq!(
            pricing.resolve(36_400_000.0, Ok(12_345_678.0)).unwrap(),
            35_308_000.0
        );
    }

    #[test]
    fn expired_pricing_falls_back_to_rounded_old_price() {
        let config = SafConfig::default();
        let pricing = ExpiredRelistPricing::from_config(&config);

        assert_eq!(
            pricing
                .resolve(11_500_000.0, Err("no price row".to_string()))
                .unwrap(),
            11_200_000.0
        );
    }

    #[test]
    fn relist_plan_matches_configured_price_and_duration_rules() {
        let config = SafConfig::default();
        let plan = RelistPlan::from_purchase(
            &config,
            RelistPurchase {
                auction_id: AuctionId::new("auction-1").unwrap(),
                target_price: 2_000_000.0,
                profit: 500_000.0,
                item_name: "Test Item".to_string(),
                tag: Some("TEST_ITEM".to_string()),
                item_uuid: Some(ItemUuid::new("item-uuid").unwrap()),
                override_price: false,
            },
        )
        .unwrap();

        assert_eq!(plan.list_price, 1_900_000);
        assert_eq!(plan.duration_visual, "2 Day");
    }

    #[test]
    fn expired_queue_action_uses_unique_uuid_as_auction_and_inventory_key() {
        let action = ExpiredRelistQueueAction::new(
            "Tester",
            &ItemUuid::new("expired-item-uuid").unwrap(),
            "Menacing Mythos Cloak",
            Some("MYTHOS_CLOAK".to_string()),
            12_345_678.4,
            24_300_000.0,
        )
        .unwrap();

        assert_eq!(action.price, 12_345_678);
        assert_eq!(action.auction_id, "expired-item-uuid");
        assert_eq!(action.inventory_uuid, "expired-item-uuid");
    }
}
