use crate::numbers::parse_number_input;
use crate::time::parse_timestamp_millis;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SafConfig {
    pub igns: Vec<String>,
    pub default_ign: String,
    pub start_default_only: bool,
    pub discord_id: String,
    pub allowed_i_ds: Vec<String>,
    pub discord_bot: DiscordBotConfig,
    pub branding: BrandingConfig,
    pub external_backend: ExternalBackendConfig,
    #[serde(deserialize_with = "deserialize_webhooks")]
    pub webhook: Vec<String>,
    pub webhook_format: String,
    #[serde(deserialize_with = "deserialize_webhooks")]
    pub send_all_flips: Vec<String>,
    pub visit_friend: String,
    pub use_cookie: bool,
    pub auto_cookie: String,
    pub angry_coop_prevention: bool,
    pub relist: bool,
    pub ping_on_update: bool,
    pub delay: u64,
    pub buying_ready_delay: u64,
    pub waittime: u64,
    pub percent_of_target: Vec<Value>,
    pub list_hours: Vec<Value>,
    pub click_delay: u64,
    pub bed_spam: bool,
    pub block_useless_messages: bool,
    pub round_to: u32,
    pub skip: SkipConfig,
    pub do_not_buy: BlockListConfig,
    pub do_not_relist: RelistBlockConfig,
    pub auto_rotate: BTreeMap<String, String>,
    pub session: String,
}

impl Default for SafConfig {
    fn default() -> Self {
        Self {
            igns: vec![String::new()],
            default_ign: String::new(),
            start_default_only: false,
            discord_id: String::new(),
            allowed_i_ds: Vec::new(),
            discord_bot: DiscordBotConfig::default(),
            branding: BrandingConfig::default(),
            external_backend: ExternalBackendConfig::default(),
            webhook: Vec::new(),
            webhook_format:
                "You bought [``{0}``](https://sky.coflnet.com/auction/{7}) for ``{2}`` (``{1}`` profit) in ``{4}ms``"
                    .to_string(),
            send_all_flips: Vec::new(),
            visit_friend: String::new(),
            use_cookie: true,
            auto_cookie: "1h".to_string(),
            angry_coop_prevention: false,
            relist: true,
            ping_on_update: false,
            delay: 250,
            buying_ready_delay: 75,
            waittime: 15,
            percent_of_target: vec![Value::from("0"), Value::from("10b"), Value::from(97)],
            list_hours: vec![Value::from("0"), Value::from("10b"), Value::from(48)],
            click_delay: 125,
            bed_spam: false,
            block_useless_messages: true,
            round_to: 6,
            skip: SkipConfig::default(),
            do_not_buy: BlockListConfig::default(),
            do_not_relist: RelistBlockConfig::default(),
            auto_rotate: default_auto_rotate(),
            session: String::new(),
        }
    }
}

impl SafConfig {
    pub fn from_json5_str(raw: &str) -> Result<Self, ConfigError> {
        let mut parsed: Self = json5::from_str(raw)?;
        parsed.normalize_node_defaults();
        Ok(parsed)
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, ConfigError> {
        let raw = fs::read_to_string(path)?;
        Self::from_json5_str(&raw)
    }

    pub fn configured_igns(&self) -> Vec<String> {
        crate::account::configured_igns(&self.igns)
    }

    pub fn default_account(&self) -> Option<String> {
        let configured = self.configured_igns();
        if configured.is_empty() {
            return None;
        }

        if !self.default_ign.trim().is_empty() {
            let wanted = self.default_ign.trim();
            if let Some(found) = configured
                .iter()
                .find(|ign| ign.eq_ignore_ascii_case(wanted))
                .cloned()
            {
                return Some(found);
            }
        }

        configured.first().cloned()
    }

    pub fn startup_igns(&self) -> Vec<String> {
        if self.start_default_only {
            self.default_account().into_iter().collect()
        } else {
            self.configured_igns()
        }
    }

    pub fn buying_ready_delay_ms(&self) -> u64 {
        self.buying_ready_delay.min(100)
    }

    fn normalize_node_defaults(&mut self) {
        for (key, value) in default_auto_rotate() {
            self.auto_rotate.entry(key).or_insert(value);
        }

        if self.branding.player_head_url_template
            == "https://crafthead.net/cube/{uuid}?size=180&v={version}"
        {
            self.branding.player_head_url_template =
                BrandingConfig::default().player_head_url_template;
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DiscordBotConfig {
    pub enabled: bool,
    pub token: String,
    pub client_id: String,
    pub guild_id: String,
    pub ephemeral: bool,
    pub allowed_i_ds: Vec<String>,
}

impl Default for DiscordBotConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: String::new(),
            client_id: String::new(),
            guild_id: String::new(),
            ephemeral: true,
            allowed_i_ds: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BrandingConfig {
    pub name: String,
    pub icon_url: String,
    pub player_head_version: String,
    pub player_head_url_template: String,
}

impl Default for BrandingConfig {
    fn default() -> Self {
        Self {
            name: "SAF".to_string(),
            icon_url: String::new(),
            player_head_version: String::new(),
            player_head_url_template: "https://crafthead.net/cube/{texture}?size=180&v={version}"
                .to_string(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExternalBackendConfig {
    pub enabled: bool,
    pub url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SkipConfig {
    pub always: bool,
    pub min_profit: Value,
    pub profit_percentage: Value,
    pub min_price: Value,
    pub user_finder: bool,
    pub skins: bool,
}

impl Default for SkipConfig {
    fn default() -> Self {
        Self {
            always: false,
            min_profit: Value::from("25m"),
            profit_percentage: Value::from("500"),
            min_price: Value::from("500m"),
            user_finder: true,
            skins: true,
        }
    }
}

impl SkipConfig {
    pub fn min_profit_value(&self) -> Option<f64> {
        parse_number_input(self.min_profit.clone().into())
    }

    pub fn min_price_value(&self) -> Option<f64> {
        parse_number_input(self.min_price.clone().into())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BlockListConfig {
    pub tags: Vec<Value>,
    pub names: Vec<Value>,
    pub enchantments: Vec<Value>,
    pub item_enchantments: Vec<ItemEnchantmentConfig>,
}

impl Default for BlockListConfig {
    fn default() -> Self {
        Self {
            tags: Vec::new(),
            names: Vec::new(),
            enchantments: Vec::new(),
            item_enchantments: vec![default_lava_shell_the_one_block()],
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RelistBlockConfig {
    pub profit_over: Value,
    pub skinned: bool,
    pub tags: Vec<Value>,
    pub names: Vec<Value>,
    pub enchantments: Vec<Value>,
    pub item_enchantments: Vec<ItemEnchantmentConfig>,
    pub finders: Vec<Value>,
    pub stacks: bool,
    pub ping_on_failed_listing: bool,
    pub drill_with_parts: bool,
    pub expired_auctions: bool,
    pub relist_mode: String,
}

impl Default for RelistBlockConfig {
    fn default() -> Self {
        Self {
            profit_over: Value::from("1000t"),
            skinned: false,
            tags: vec![Value::from("GIANTS_SWORD")],
            names: Vec::new(),
            enchantments: Vec::new(),
            item_enchantments: vec![default_lava_shell_the_one_block()],
            finders: Vec::new(),
            stacks: true,
            ping_on_failed_listing: false,
            drill_with_parts: false,
            expired_auctions: true,
            relist_mode: "1".to_string(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ItemEnchantmentConfig {
    pub tag: Option<String>,
    pub name: Option<String>,
    pub enchantment: String,
    pub level: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_expiry_millis")]
    pub expires_at: Option<u64>,
}

fn default_lava_shell_the_one_block() -> ItemEnchantmentConfig {
    ItemEnchantmentConfig {
        tag: Some("LAVA_SHELL_NECKLACE".to_string()),
        name: None,
        enchantment: "THE_ONE".to_string(),
        level: Some(5),
        expires_at: None,
    }
}

fn default_auto_rotate() -> BTreeMap<String, String> {
    BTreeMap::from([("ign".to_string(), "12r:12f".to_string())])
}

fn deserialize_webhooks<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::String(value) if value.trim().is_empty() => Vec::new(),
        Value::String(value) => vec![value],
        Value::Array(values) => values
            .into_iter()
            .filter_map(|value| value.as_str().map(str::trim).map(ToOwned::to_owned))
            .filter(|value| !value.is_empty())
            .collect(),
        _ => Vec::new(),
    })
}

fn deserialize_expiry_millis<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| match value {
        Value::Number(number) => number.as_u64(),
        Value::String(value) => parse_timestamp_millis(&value),
        _ => None,
    }))
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("failed to read config: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to parse config json5: {0}")]
    Json5(#[from] json5::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_existing_json5_shape() {
        let config = SafConfig::from_json5_str(
            r#"{
                igns: ["Main", "Alt"],
                defaultIgn: "Alt",
                startDefaultOnly: true,
                webhook: "https://example.test/webhook",
                webhookFormat: "Bought {0} for {2}",
                sendAllFlips: "",
                doNotRelist: { tags: ["GIANTS_SWORD"] }
            }"#,
        )
        .unwrap();

        assert_eq!(config.startup_igns(), vec!["Alt"]);
        assert_eq!(config.webhook, vec!["https://example.test/webhook"]);
        assert_eq!(config.webhook_format, "Bought {0} for {2}");
        assert!(config.send_all_flips.is_empty());
        assert!(config.discord_bot.ephemeral);
    }

    #[test]
    fn discord_bot_defaults_to_ephemeral_unless_explicitly_disabled() {
        let defaulted = SafConfig::from_json5_str("{ discordBot: {} }").unwrap();
        assert!(defaulted.discord_bot.ephemeral);

        let public = SafConfig::from_json5_str("{ discordBot: { ephemeral: false } }").unwrap();
        assert!(!public.discord_bot.ephemeral);
    }

    #[test]
    fn buy_and_relist_item_enchantment_defaults_match_node_config() {
        let config = SafConfig::from_json5_str("{}").unwrap();

        assert_eq!(config.do_not_buy.item_enchantments.len(), 1);
        assert_eq!(
            config.do_not_buy.item_enchantments[0].tag.as_deref(),
            Some("LAVA_SHELL_NECKLACE")
        );
        assert_eq!(
            config.do_not_buy.item_enchantments[0].enchantment,
            "THE_ONE"
        );
        assert_eq!(config.do_not_buy.item_enchantments[0].level, Some(5));
        assert_eq!(
            config.do_not_relist.item_enchantments,
            config.do_not_buy.item_enchantments
        );

        let explicit = SafConfig::from_json5_str("{ doNotBuy: {}, doNotRelist: {} }").unwrap();
        assert_eq!(
            explicit.do_not_buy.item_enchantments,
            config.do_not_buy.item_enchantments
        );
        assert_eq!(
            explicit.do_not_relist.item_enchantments,
            config.do_not_relist.item_enchantments
        );
    }

    #[test]
    fn configured_igns_are_trimmed_and_deduped() {
        let config = SafConfig {
            igns: vec![
                " Main ".to_string(),
                "main".to_string(),
                "Alt".to_string(),
                " ".to_string(),
            ],
            default_ign: "alt".to_string(),
            ..Default::default()
        };

        assert_eq!(config.configured_igns(), vec!["Main", "Alt"]);
        assert_eq!(config.default_account().as_deref(), Some("Alt"));
        assert_eq!(config.startup_igns(), vec!["Main", "Alt"]);
    }

    #[test]
    fn old_player_head_renderer_is_normalized() {
        let config = SafConfig::from_json5_str(
            r#"{
                branding: {
                    playerHeadUrlTemplate: "https://crafthead.net/cube/{uuid}?size=180&v={version}"
                }
            }"#,
        )
        .unwrap();

        assert_eq!(
            config.branding.player_head_url_template,
            BrandingConfig::default().player_head_url_template
        );
    }

    #[test]
    fn auto_rotate_defaults_match_node_config() {
        let defaulted = SafConfig::from_json5_str("{}").unwrap();
        assert_eq!(
            defaulted.auto_rotate.get("ign").map(String::as_str),
            Some("12r:12f")
        );

        let merged = SafConfig::from_json5_str("{ autoRotate: { Alt: \"6r:6f\" } }").unwrap();
        assert_eq!(
            merged.auto_rotate.get("ign").map(String::as_str),
            Some("12r:12f")
        );
        assert_eq!(
            merged.auto_rotate.get("Alt").map(String::as_str),
            Some("6r:6f")
        );
    }
}
