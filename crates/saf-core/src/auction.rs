use crate::numbers::{normal_number, parse_number_input};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PriceRule {
    pub lower: f64,
    pub upper: f64,
    pub percent: f64,
}

impl PriceRule {
    pub fn from_flat_config(values: &[Value]) -> Vec<Self> {
        values
            .chunks(3)
            .filter_map(|chunk| {
                let lower = parse_number_input(chunk.first()?.clone().into())?;
                let upper = parse_number_input(chunk.get(1)?.clone().into())?;
                let percent = parse_number_input(chunk.get(2)?.clone().into())?;
                Some(Self {
                    lower,
                    upper,
                    percent,
                })
            })
            .collect()
    }

    pub fn percent_for_price(rules: &[Self], price: f64, fallback: f64) -> f64 {
        rules
            .iter()
            .find(|rule| price >= rule.lower && price < rule.upper)
            .map(|rule| rule.percent)
            .unwrap_or(fallback)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ListingDurationRule {
    pub lower: f64,
    pub upper: f64,
    pub hours: f64,
}

impl ListingDurationRule {
    pub fn from_flat_config(values: &[Value]) -> Vec<Self> {
        values
            .chunks(3)
            .filter_map(|chunk| {
                let lower = parse_number_input(chunk.first()?.clone().into())?;
                let upper = parse_number_input(chunk.get(1)?.clone().into())?;
                let hours = parse_number_input(chunk.get(2)?.clone().into())?;
                Some(Self {
                    lower,
                    upper,
                    hours,
                })
            })
            .collect()
    }

    pub fn hours_for_price(rules: &[Self], price: f64, fallback: f64) -> f64 {
        rules
            .iter()
            .find(|rule| price >= rule.lower && price < rule.upper)
            .map(|rule| rule.hours)
            .unwrap_or(fallback)
    }
}

pub fn rounded_listing_price(target: f64, percent: f64, round_to: u32) -> Option<u64> {
    if !target.is_finite() || !percent.is_finite() || target <= 0.0 {
        return None;
    }

    let raw = target * percent / 100.0;
    let precision = 10_u64.saturating_pow(round_to.saturating_sub(1));
    if precision <= 1 {
        return Some(raw.round().max(1.0) as u64);
    }

    Some(((raw / precision as f64).round() as u64).max(1) * precision)
}

pub fn inventory_listing_price(median: f64, volume: f64, lbin: f64) -> Option<f64> {
    if !median.is_finite() || median <= 0.0 {
        return (lbin.is_finite() && lbin > 1.0).then_some(lbin - 1.0);
    }
    if !lbin.is_finite() || lbin <= 1.0 {
        return Some(median);
    }
    let volume = if volume.is_finite() { volume } else { 0.0 };
    if median < lbin {
        if volume <= 3.0 {
            Some(median)
        } else {
            let difference = lbin - median;
            let volume_factor = 2.0 - (volume / (volume - 1.5));
            Some(median + difference * volume_factor)
        }
    } else if lbin * 2.0 < median {
        Some(median - 1.0)
    } else {
        Some(lbin - 1.0)
    }
}

pub fn calc_duration_visual(hours: f64) -> String {
    if hours > 336.0 {
        "14 Days".to_string()
    } else if hours > 24.0 {
        format!("{} Day", (hours / 24.0).floor() as u64)
    } else {
        format!("{} Hour", hours.floor() as u64)
    }
}

pub fn fallback_from_config_tail(values: &[Value], default: f64) -> f64 {
    values
        .last()
        .map(|value| normal_number(value.clone()))
        .filter(|value| value.is_finite())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn reads_flat_price_rules() {
        let config = vec![
            json!("0"),
            json!("10m"),
            json!(97),
            json!("10m"),
            json!("10b"),
            json!(95),
        ];
        let rules = PriceRule::from_flat_config(&config);

        assert_eq!(
            PriceRule::percent_for_price(&rules, 12_000_000.0, 90.0),
            95.0
        );
        assert_eq!(
            rounded_listing_price(12_345_678.0, 95.0, 6),
            Some(11_700_000)
        );
    }

    #[test]
    fn inventory_listing_price_matches_node_heuristic() {
        assert_eq!(inventory_listing_price(0.0, 0.0, 2_000.0), Some(1_999.0));
        assert_eq!(
            inventory_listing_price(1_000_000.0, 2.0, 1_500_000.0),
            Some(1_000_000.0)
        );
        assert_eq!(
            inventory_listing_price(1_000_000.0, 6.0, 1_600_000.0),
            Some(1_400_000.0)
        );
        assert_eq!(
            inventory_listing_price(3_000_000.0, 0.0, 1_000_000.0),
            Some(2_999_999.0)
        );
    }

    #[test]
    fn duration_visual_matches_existing_labels() {
        assert_eq!(calc_duration_visual(48.0), "2 Day");
        assert_eq!(calc_duration_visual(400.0), "14 Days");
    }
}
