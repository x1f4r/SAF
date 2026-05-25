use super::*;
use saf_core::{Enchantment, ItemContext};

#[tokio::test]
async fn file_blacklist_store_updates_config_and_live_policy() {
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("config.json5");
    tokio::fs::write(
        &config_path,
        r#"{
          // Keep operator notes intact
          igns: ["Main"],
          doNotBuy: { tags: [], names: [], enchantments: [], itemEnchantments: [] },
          doNotRelist: { tags: ["GIANTS_SWORD"], names: [], enchantments: [], itemEnchantments: [] }
        }"#,
    )
    .await
    .unwrap();
    let config = SafConfig::from_path(&config_path).unwrap();
    let handle = BlacklistPolicyHandle::new(BlacklistPolicy::from_config(
        &config.do_not_buy,
        &config.do_not_relist,
    ));
    let store = FileBlacklistStore::new(&config_path, handle.clone());
    let account = AccountId::new("Main").unwrap();

    let result = store
        .apply(
            &account,
            BlacklistRequest::Update(BlacklistUpdate {
                action: BlacklistAction::Add,
                scope: BlacklistScope::Buy,
                field: BlacklistField::Tag,
                value: "speed_relic".to_string(),
                duration: None,
                until: None,
            }),
        )
        .await
        .unwrap();

    let raw = tokio::fs::read_to_string(&config_path).await.unwrap();
    assert!(result.changed);
    assert!(raw.contains("// Keep operator notes intact"));
    assert!(raw.contains("SPEED_RELIC"));
    assert_eq!(
        handle.current().buy_block_reason(&ItemContext {
            tag: Some("SPEED_RELIC".to_string()),
            ..Default::default()
        }),
        Some(saf_core::BlockReason::Tag("SPEED_RELIC".to_string()))
    );
}

#[tokio::test]
async fn file_blacklist_store_update_preserves_implicit_item_enchant_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("config.json5");
    tokio::fs::write(
        &config_path,
        r#"{
          igns: ["Main"],
          doNotBuy: {},
          doNotRelist: {},
        }"#,
    )
    .await
    .unwrap();
    let config = SafConfig::from_path(&config_path).unwrap();
    let handle = BlacklistPolicyHandle::new(BlacklistPolicy::from_config(
        &config.do_not_buy,
        &config.do_not_relist,
    ));
    let store = FileBlacklistStore::new(&config_path, handle.clone());
    let account = AccountId::new("Main").unwrap();

    store
        .apply(
            &account,
            BlacklistRequest::Update(BlacklistUpdate {
                action: BlacklistAction::Add,
                scope: BlacklistScope::Buy,
                field: BlacklistField::ItemEnchant,
                value: "SPEED_RELIC THE_ONE:5".to_string(),
                duration: None,
                until: None,
            }),
        )
        .await
        .unwrap();

    let raw = tokio::fs::read_to_string(&config_path).await.unwrap();
    assert!(raw.contains("LAVA_SHELL_NECKLACE"));
    assert!(raw.contains("SPEED_RELIC"));
    assert_eq!(
        handle.current().buy_block_reason(&ItemContext {
            tag: Some("LAVA_SHELL_NECKLACE".to_string()),
            enchantments: vec![Enchantment::new("THE_ONE", Some(5))],
            ..Default::default()
        }),
        Some(saf_core::BlockReason::ItemEnchantment(
            "LAVA_SHELL_NECKLACE:THE_ONE:5".to_string()
        ))
    );
    assert_eq!(
        handle.current().buy_block_reason(&ItemContext {
            tag: Some("SPEED_RELIC".to_string()),
            enchantments: vec![Enchantment::new("THE_ONE", Some(5))],
            ..Default::default()
        }),
        Some(saf_core::BlockReason::ItemEnchantment(
            "SPEED_RELIC:THE_ONE:5".to_string()
        ))
    );
}

#[tokio::test]
async fn file_blacklist_store_lists_effective_config_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let config_path = temp.path().join("config.json5");
    tokio::fs::write(
        &config_path,
        r#"{
          igns: ["Main"],
          doNotBuy: { tags: ["SPEED_RELIC"] },
          doNotRelist: {},
        }"#,
    )
    .await
    .unwrap();
    let config = SafConfig::from_path(&config_path).unwrap();
    let handle = BlacklistPolicyHandle::new(BlacklistPolicy::from_config(
        &config.do_not_buy,
        &config.do_not_relist,
    ));
    let store = FileBlacklistStore::new(&config_path, handle);
    let account = AccountId::new("Main").unwrap();

    let result = store.apply(&account, BlacklistRequest::List).await.unwrap();
    let snapshot: Value = serde_json::from_str(&result.summary).unwrap();

    assert_eq!(snapshot["doNotBuy"]["tags"][0], "SPEED_RELIC");
    assert_eq!(
        snapshot["doNotBuy"]["itemEnchantments"][0]["tag"],
        "LAVA_SHELL_NECKLACE"
    );
    assert_eq!(
        snapshot["doNotRelist"]["itemEnchantments"][0]["enchantment"],
        "THE_ONE"
    );
    assert_eq!(snapshot["doNotRelist"]["tags"][0], "GIANTS_SWORD");
}

#[test]
fn patch_config_array_preserves_surrounding_json5() {
    let raw = r#"{
      doNotBuy: {
        // Tags stay documented
        tags: ["OLD"],
        names: []
      }
    }"#;

    let patched = patch_config_array(raw, "doNotBuy", "tags", &[json!("NEW")]).unwrap();

    assert!(patched.contains("// Tags stay documented"));
    assert!(patched.contains("\"NEW\""));
    assert!(!patched.contains("\"OLD\""));
}
