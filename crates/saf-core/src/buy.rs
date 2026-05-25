use crate::config::SkipConfig;
use crate::flip::strip_item_name;
use crate::numbers::parse_number_input;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuySpeedProfile {
    pub name: &'static str,
    pub first_retry_ticks: u8,
    pub first_retry_ms: u64,
    pub retry_ticks: u8,
    pub retry_ms: u64,
    pub max_retries: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuyThresholds {
    pub fast_buy_price: f64,
    pub turbo_buy_price: f64,
}

impl Default for BuyThresholds {
    fn default() -> Self {
        Self {
            fast_buy_price: 15_000_000.0,
            turbo_buy_price: 30_000_000.0,
        }
    }
}

pub fn buy_speed_profile(price: f64, thresholds: BuyThresholds) -> BuySpeedProfile {
    if price.is_finite() && price >= thresholds.turbo_buy_price {
        return BuySpeedProfile {
            name: "turbo",
            first_retry_ticks: 1,
            first_retry_ms: 40,
            retry_ticks: 1,
            retry_ms: 55,
            max_retries: 3,
        };
    }
    if price.is_finite() && price >= thresholds.fast_buy_price {
        return BuySpeedProfile {
            name: "fast",
            first_retry_ticks: 1,
            first_retry_ms: 90,
            retry_ticks: 2,
            retry_ms: 120,
            max_retries: 5,
        };
    }
    BuySpeedProfile {
        name: "normal",
        first_retry_ticks: 3,
        first_retry_ms: 250,
        retry_ticks: 5,
        retry_ms: 350,
        max_retries: 8,
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SkipPolicy {
    pub always: bool,
    pub min_profit: f64,
    pub user_finder: bool,
    pub skins: bool,
    pub min_percent: f64,
    pub min_price: f64,
}

impl SkipPolicy {
    pub fn from_config(config: &SkipConfig) -> Self {
        Self {
            always: config.always,
            min_profit: parse_number_input(config.min_profit.clone().into())
                .unwrap_or(f64::INFINITY),
            user_finder: config.user_finder,
            skins: config.skins,
            min_percent: parse_number_input(config.profit_percentage.clone().into())
                .unwrap_or(f64::INFINITY),
            min_price: parse_number_input(config.min_price.clone().into()).unwrap_or(f64::INFINITY),
        }
    }

    pub fn decide(&self, context: &BuyDecision) -> SkipDecision {
        let mut reasons = Vec::new();
        if self.always {
            reasons.push(SkipReason::Always);
        }
        if self.user_finder && context.finder == "USER" {
            reasons.push(SkipReason::UserFinder);
        }
        if self.skins && is_skin(&context.item_name) {
            reasons.push(SkipReason::Skin);
        }
        if context.profit > self.min_profit {
            reasons.push(SkipReason::MinProfit);
        }
        if context.profit_percent > self.min_percent {
            reasons.push(SkipReason::MinPercent);
        }
        if context.price > self.min_price {
            reasons.push(SkipReason::MinPrice);
        }
        SkipDecision { reasons }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BuyDecision {
    pub item_name: String,
    pub finder: String,
    pub profit: f64,
    pub profit_percent: f64,
    pub price: f64,
}

impl BuyDecision {
    pub fn weird_item_name(&self) -> String {
        strip_item_name(&self.item_name)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkipDecision {
    pub reasons: Vec<SkipReason>,
}

impl SkipDecision {
    pub fn should_skip(&self) -> bool {
        !self.reasons.is_empty()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SkipReason {
    Always,
    UserFinder,
    Skin,
    MinProfit,
    MinPercent,
    MinPrice,
}

pub fn is_skin(item: &str) -> bool {
    item.contains('✦') || item.contains('✿') || item.to_ascii_lowercase().contains("skin")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SkipConfig;
    use serde_json::json;

    #[test]
    fn buy_speed_profile_gets_more_aggressive_for_value() {
        let thresholds = BuyThresholds::default();
        assert_eq!(buy_speed_profile(14_999_999.0, thresholds).name, "normal");
        assert_eq!(buy_speed_profile(15_000_000.0, thresholds).name, "fast");
        assert_eq!(buy_speed_profile(30_000_000.0, thresholds).name, "turbo");
        assert!(
            buy_speed_profile(30_000_000.0, thresholds).first_retry_ms
                < buy_speed_profile(15_000_000.0, thresholds).first_retry_ms
        );
    }

    #[test]
    fn skip_policy_tracks_existing_or_conditions() {
        let policy = SkipPolicy::from_config(&SkipConfig {
            always: false,
            min_profit: json!("25m"),
            profit_percentage: json!("500"),
            min_price: json!("500m"),
            user_finder: true,
            skins: true,
        });
        let decision = policy.decide(&BuyDecision {
            item_name: "Cool Skin".to_string(),
            finder: "USER".to_string(),
            profit: 1.0,
            profit_percent: 1.0,
            price: 1.0,
        });

        assert_eq!(
            decision.reasons,
            vec![SkipReason::UserFinder, SkipReason::Skin]
        );
    }
}
