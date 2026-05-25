use super::{
    BlockReason, Enchantment, ItemContext, enchant_matches, entry_expires_at, item_names,
    item_tags, normalize_enchant_id, normalize_name, normalize_tag, parse_enchant_spec,
};
use crate::config::{BlockListConfig, ItemEnchantmentConfig, RelistBlockConfig};
use crate::numbers::parse_number_input;
use crate::time::unix_millis;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BlacklistPolicy {
    pub buy: BlockSection,
    pub relist: RelistSection,
}

impl BlacklistPolicy {
    pub fn from_config(buy: &BlockListConfig, relist: &RelistBlockConfig) -> Self {
        Self {
            buy: BlockSection::from_config(buy),
            relist: RelistSection::from_config(relist),
        }
    }

    pub fn buy_block_reason(&self, item: &ItemContext) -> Option<BlockReason> {
        self.buy.block_reason(item)
    }

    pub fn relist_block_reason(&self, item: &ItemContext) -> Option<BlockReason> {
        self.relist.block_reason(item)
    }
}

#[derive(Clone, Debug)]
pub struct BlacklistPolicyHandle {
    inner: Arc<Mutex<BlacklistPolicy>>,
}

impl BlacklistPolicyHandle {
    pub fn new(policy: BlacklistPolicy) -> Self {
        Self {
            inner: Arc::new(Mutex::new(policy)),
        }
    }

    pub fn current(&self) -> BlacklistPolicy {
        self.inner
            .lock()
            .map(|policy| policy.clone())
            .unwrap_or_default()
    }

    pub fn replace(&self, policy: BlacklistPolicy) {
        if let Ok(mut current) = self.inner.lock() {
            *current = policy;
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BlockSection {
    pub tags: Vec<TimedValue>,
    pub names: Vec<TimedValue>,
    pub enchantments: Vec<TimedValue>,
    pub item_enchantments: Vec<ItemEnchantmentRule>,
}

impl BlockSection {
    pub fn from_config(config: &BlockListConfig) -> Self {
        Self {
            tags: values(&config.tags, normalize_tag),
            names: values(&config.names, normalize_name),
            enchantments: values(&config.enchantments, |value| {
                parse_enchant_spec(value)
                    .map(|enchant| enchant.key())
                    .unwrap_or_else(|| normalize_enchant_id(value))
            }),
            item_enchantments: config
                .item_enchantments
                .iter()
                .filter_map(ItemEnchantmentRule::from_config)
                .collect(),
        }
    }

    pub fn block_reason(&self, item: &ItemContext) -> Option<BlockReason> {
        let now = unix_millis();
        let tags = item_tags(item);
        if let Some(rule) = self
            .tags
            .iter()
            .find(|rule| rule.active_at(now) && tags.iter().any(|tag| tag == &rule.value))
        {
            return Some(BlockReason::Tag(rule.value.clone()));
        }

        let names = item_names(item);
        if let Some(rule) = self
            .names
            .iter()
            .find(|rule| rule.active_at(now) && names.iter().any(|name| name == &rule.value))
        {
            return Some(BlockReason::Name(rule.value.clone()));
        }

        if let Some(rule) = blocked_enchantment(&self.enchantments, &item.enchantments, now) {
            return Some(BlockReason::Enchantment(rule));
        }

        if let Some(rule) = self
            .item_enchantments
            .iter()
            .find(|rule| rule.active_at(now) && rule.matches(item))
        {
            return Some(BlockReason::ItemEnchantment(rule.label()));
        }

        None
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct RelistSection {
    pub base: BlockSection,
    pub profit_over: Option<f64>,
    pub skinned: bool,
    pub finders: Vec<TimedValue>,
}

impl RelistSection {
    pub fn from_config(config: &RelistBlockConfig) -> Self {
        let block_config = BlockListConfig {
            tags: config.tags.clone(),
            names: config.names.clone(),
            enchantments: config.enchantments.clone(),
            item_enchantments: config.item_enchantments.clone(),
        };

        Self {
            base: BlockSection::from_config(&block_config),
            profit_over: parse_number_input(config.profit_over.clone().into()),
            skinned: config.skinned,
            finders: values(&config.finders, |value| value.trim().to_ascii_uppercase()),
        }
    }

    pub fn block_reason(&self, item: &ItemContext) -> Option<BlockReason> {
        if let Some(reason) = self.base.block_reason(item) {
            return Some(reason);
        }

        if self.skinned && item.skinned {
            return Some(BlockReason::Skin);
        }

        if !item.is_purchased_inventory_relist {
            if let (Some(limit), Some(profit)) = (self.profit_over, item.profit)
                && profit > limit
            {
                return Some(BlockReason::Profit(limit.to_string()));
            }

            if let Some(finder) = &item.finder {
                let finder = finder.trim().to_ascii_uppercase();
                let now = unix_millis();
                if let Some(rule) = self
                    .finders
                    .iter()
                    .find(|rule| rule.active_at(now) && rule.value == finder)
                {
                    return Some(BlockReason::Finder(rule.value.clone()));
                }
            }
        }

        None
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TimedValue {
    pub value: String,
    pub expires_at: Option<u64>,
}

impl TimedValue {
    pub fn active_at(&self, now: u64) -> bool {
        self.expires_at.is_none_or(|expires_at| expires_at > now)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ItemEnchantmentRule {
    pub tag: Option<String>,
    pub name: Option<String>,
    pub enchantment: Enchantment,
    pub expires_at: Option<u64>,
}

impl ItemEnchantmentRule {
    fn from_config(config: &ItemEnchantmentConfig) -> Option<Self> {
        Some(Self {
            tag: config.tag.as_deref().map(normalize_tag),
            name: config.name.as_deref().map(normalize_name),
            enchantment: Enchantment::new(&config.enchantment, config.level),
            expires_at: config.expires_at,
        })
    }

    fn active_at(&self, now: u64) -> bool {
        self.expires_at.is_none_or(|expires_at| expires_at > now)
    }

    fn matches(&self, item: &ItemContext) -> bool {
        let tag_matches = self
            .tag
            .as_ref()
            .is_none_or(|rule| item_tags(item).iter().any(|tag| tag == rule));
        let name_matches = self
            .name
            .as_ref()
            .is_none_or(|rule| item_names(item).iter().any(|name| name == rule));
        tag_matches
            && name_matches
            && item
                .enchantments
                .iter()
                .any(|actual| enchant_matches(actual, &self.enchantment))
    }

    fn label(&self) -> String {
        let item = self
            .tag
            .as_deref()
            .or(self.name.as_deref())
            .unwrap_or("item");
        format!("{item}:{}", self.enchantment.key())
    }
}

fn values(values: &[Value], normalize: impl Fn(&str) -> String) -> Vec<TimedValue> {
    values
        .iter()
        .filter_map(|value| match value {
            Value::String(value) => Some(TimedValue {
                value: normalize(value),
                expires_at: None,
            }),
            Value::Object(object) => {
                let raw = object.get("value")?.as_str()?;
                Some(TimedValue {
                    value: normalize(raw),
                    expires_at: entry_expires_at(value),
                })
            }
            _ => None,
        })
        .collect()
}

fn blocked_enchantment(rules: &[TimedValue], actual: &[Enchantment], now: u64) -> Option<String> {
    rules
        .iter()
        .filter(|rule| rule.active_at(now))
        .filter_map(|rule| parse_enchant_spec(&rule.value))
        .find(|rule| actual.iter().any(|actual| enchant_matches(actual, rule)))
        .map(|rule| rule.key())
}
