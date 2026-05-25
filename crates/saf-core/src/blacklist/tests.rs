use super::*;
use crate::config::{BlockListConfig, ItemEnchantmentConfig, RelistBlockConfig, SafConfig};
use crate::parse_lore_enchantments;
use serde_json::json;

#[test]
fn blocks_buy_by_item_enchantment_combo() {
    let buy = BlockListConfig {
        item_enchantments: vec![ItemEnchantmentConfig {
            tag: Some("LAVA_SHELL_NECKLACE".to_string()),
            name: None,
            enchantment: "THE_ONE".to_string(),
            level: Some(5),
            expires_at: None,
        }],
        ..Default::default()
    };
    let policy = BlacklistPolicy::from_config(&buy, &RelistBlockConfig::default());
    let reason = policy.buy_block_reason(&ItemContext {
        tag: Some("lava_shell_necklace".to_string()),
        enchantments: vec![Enchantment::new("the one", Some(5))],
        ..Default::default()
    });

    assert_eq!(
        reason,
        Some(BlockReason::ItemEnchantment(
            "LAVA_SHELL_NECKLACE:THE_ONE:5".to_string()
        ))
    );
}

#[test]
fn temporary_rules_expire() {
    let buy = BlockListConfig {
        tags: vec![json!({ "value": "GIANTS_SWORD", "expiresAt": 1 })],
        ..Default::default()
    };
    let policy = BlacklistPolicy::from_config(&buy, &RelistBlockConfig::default());

    assert_eq!(
        policy.buy_block_reason(&ItemContext {
            tag: Some("GIANTS_SWORD".to_string()),
            ..Default::default()
        }),
        None
    );
}

#[test]
fn node_style_iso_expiries_are_honored() {
    let config = SafConfig::from_json5_str(
        r#"{
          doNotBuy: {
            tags: [
              { value: "OLD_TAG", expiresAt: "2000-01-01T00:00:00.000Z" },
              { value: "FUTURE_TAG", expiresAt: "2030-01-01T00:00:00.000Z" }
            ],
            itemEnchantments: [
              { tag: "LAVA_SHELL_NECKLACE", enchantment: "THE_ONE", level: 5, expiresAt: "2030-01-01T00:00:00.000Z" }
            ]
          }
        }"#,
    )
    .unwrap();
    let policy = BlacklistPolicy::from_config(&config.do_not_buy, &RelistBlockConfig::default());

    assert_eq!(
        policy.buy_block_reason(&ItemContext {
            tag: Some("OLD_TAG".to_string()),
            ..Default::default()
        }),
        None
    );
    assert_eq!(
        policy.buy_block_reason(&ItemContext {
            tag: Some("FUTURE_TAG".to_string()),
            ..Default::default()
        }),
        Some(BlockReason::Tag("FUTURE_TAG".to_string()))
    );
    assert_eq!(
        policy.buy_block_reason(&ItemContext {
            tag: Some("LAVA_SHELL_NECKLACE".to_string()),
            enchantments: vec![Enchantment::new("THE_ONE", Some(5))],
            ..Default::default()
        }),
        Some(BlockReason::ItemEnchantment(
            "LAVA_SHELL_NECKLACE:THE_ONE:5".to_string()
        ))
    );
}

#[test]
fn purchased_inventory_relist_ignores_finder_profit_blocks() {
    let relist = RelistBlockConfig {
        finders: vec![json!("USER")],
        profit_over: json!("1"),
        ..Default::default()
    };
    let policy = BlacklistPolicy::from_config(&BlockListConfig::default(), &relist);

    assert_eq!(
        policy.relist_block_reason(&ItemContext {
            finder: Some("USER".to_string()),
            profit: Some(50.0),
            is_purchased_inventory_relist: true,
            ..Default::default()
        }),
        None
    );
}

#[test]
fn parses_enchantments_from_formatted_inventory_lore() {
    let enchantments = parse_lore_enchantments(&[
        "§dThe One V".to_string(),
        "§9Growth V, §9Protection 5".to_string(),
    ]);

    assert!(enchantments.contains(&Enchantment::new("THE_ONE", Some(5))));
    assert!(enchantments.contains(&Enchantment::new("GROWTH", Some(5))));
    assert!(enchantments.contains(&Enchantment::new("PROTECTION", Some(5))));
}

#[test]
fn parses_blacklist_list_and_update_commands() {
    assert_eq!(parse_blacklist_request("").unwrap(), BlacklistRequest::List);
    assert_eq!(
        parse_blacklist_request("add buy item-enchant LAVA_SHELL_NECKLACE THE_ONE:5 --for 7d")
            .unwrap(),
        BlacklistRequest::Update(BlacklistUpdate {
            action: BlacklistAction::Add,
            scope: BlacklistScope::Buy,
            field: BlacklistField::ItemEnchant,
            value: "LAVA_SHELL_NECKLACE THE_ONE:5".to_string(),
            duration: Some("7d".to_string()),
            until: None,
        })
    );
    assert_eq!(
        parse_blacklist_request("remove relist tag GIANTS_SWORD --until 2030-01-01").unwrap(),
        BlacklistRequest::Update(BlacklistUpdate {
            action: BlacklistAction::Remove,
            scope: BlacklistScope::Relist,
            field: BlacklistField::Tag,
            value: "GIANTS_SWORD".to_string(),
            duration: None,
            until: Some("2030-01-01".to_string()),
        })
    );
}

#[test]
fn blacklist_parser_reports_missing_value() {
    assert_eq!(
        parse_blacklist_request("add buy tag").unwrap_err(),
        BlacklistCommandError::Missing("value")
    );
}
