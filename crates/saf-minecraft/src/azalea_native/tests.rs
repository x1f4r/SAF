use super::*;
use azalea_inventory::components;
use saf_core::ids::AuctionId;
use simdnbt::owned::{BaseNbt, Nbt, NbtCompound};

#[test]
fn azalea_config_keeps_auth_and_target_server_explicit() {
    let account = AccountId::new("Main").unwrap();
    let config =
        AzaleaClientConfig::microsoft(account.clone(), "mc.hypixel.net", "main@example.com");

    assert_eq!(config.account, account);
    assert_eq!(config.server, "mc.hypixel.net");
    assert_eq!(
        config.auth,
        AzaleaAuthMode::Microsoft {
            cache_key: "main@example.com".to_string()
        }
    );
}

#[test]
fn azalea_action_mapping_matches_existing_terminal_flow() {
    assert_eq!(
        AzaleaMinecraftClient::action_chat_command(&MinecraftAction::OpenAuction(
            AuctionId::new("auction-1").unwrap()
        )),
        Some("/viewauction auction-1".to_string())
    );
    assert_eq!(
        AzaleaMinecraftClient::action_chat_command(&MinecraftAction::Chat("/is".to_string())),
        Some("/is".to_string())
    );
    assert_eq!(
        AzaleaMinecraftClient::action_chat_command(&MinecraftAction::ClickSlot(31)),
        None
    );
    assert_eq!(
        AzaleaMinecraftClient::action_chat_command(&MinecraftAction::TypeText(
            "1000000".to_string()
        )),
        None
    );
    assert_eq!(
        AzaleaMinecraftClient::action_chat_command(&MinecraftAction::ActivateHeldItem),
        None
    );
}

#[test]
fn azalea_inventory_pending_error_is_transient() {
    assert!(azalea_inventory_component_is_pending(
        "Player 22v0 is missing a required component: 'azalea_entity::inventory::Inventory'"
    ));
    assert!(!azalea_inventory_component_is_pending(
        "server closed the inventory window"
    ));

    let account = AccountId::new("Main").unwrap();
    let error = azalea_inventory_get_error(
        &account,
        "Player 22v0 is missing a required component: 'azalea_entity::inventory::Inventory'"
            .to_string(),
    );
    assert!(
        matches!(error, PortError::Unavailable(message) if message.contains("inventory is not ready for Main"))
    );

    let error = azalea_inventory_get_error(&account, "server closed the inventory window".into());
    assert!(
        matches!(error, PortError::Failed(message) if message == "server closed the inventory window")
    );
}

#[test]
fn minecraft_item_names_match_existing_slot_resolver_shape() {
    assert_eq!(minecraft_item_name("GoldNugget"), "gold_nugget");
    assert_eq!(minecraft_item_name("PoisonousPotato"), "poisonous_potato");
}

#[test]
fn azalea_item_metadata_reads_components_and_extra_attributes() {
    let item = azalea_inventory::ItemStack::new(
        azalea_crate::registry::builtin::ItemKind::DiamondSword,
        1,
    )
    .with_component(components::CustomName {
        name: "Aspect of the Dragons".into(),
    })
    .with_component(components::Lore {
        lines: vec!["Damage: +225".into(), "Seller: Main".into()],
    })
    .with_component(components::CustomData {
        nbt: Nbt::Some(BaseNbt::new(
            "",
            NbtCompound::from([(
                "ExtraAttributes",
                NbtCompound::from([
                    ("id", "ASPECT_OF_THE_DRAGON".into()),
                    ("uuid", "item-uuid".into()),
                ])
                .into(),
            )]),
        )),
    });

    let metadata = azalea_item_metadata(&item);

    assert_eq!(metadata.item_name, "diamond_sword");
    assert_eq!(metadata.display_name, "Aspect of the Dragons");
    assert_eq!(metadata.lore, vec!["Damage: +225", "Seller: Main"]);
    assert_eq!(metadata.item_uuid.as_deref(), Some("item-uuid"));
    assert_eq!(
        metadata.skyblock_tag.as_deref(),
        Some("ASPECT_OF_THE_DRAGON")
    );
}

#[test]
fn azalea_item_metadata_derives_rune_item_id() {
    let item =
        azalea_inventory::ItemStack::new(azalea_crate::registry::builtin::ItemKind::Paper, 1)
            .with_component(components::CustomData {
                nbt: Nbt::Some(BaseNbt::new(
                    "",
                    NbtCompound::from([(
                        "ExtraAttributes",
                        NbtCompound::from([
                            ("id", "RUNE".into()),
                            ("runes", NbtCompound::from([("COUTURE", 1.into())]).into()),
                        ])
                        .into(),
                    )]),
                )),
            });

    let metadata = azalea_item_metadata(&item);

    assert_eq!(metadata.item_uuid, None);
    assert_eq!(metadata.skyblock_tag.as_deref(), Some("COUTURE_RUNE"));
}

#[test]
fn azalea_item_metadata_keeps_tag_separate_when_uuid_is_absent() {
    let item = azalea_inventory::ItemStack::new(
        azalea_crate::registry::builtin::ItemKind::LeatherLeggings,
        1,
    )
    .with_component(components::CustomName {
        name: "Ancient Necron's Leggings".into(),
    })
    .with_component(components::CustomData {
        nbt: Nbt::Some(BaseNbt::new(
            "",
            NbtCompound::from([(
                "ExtraAttributes",
                NbtCompound::from([("id", "POWER_WITHER_LEGGINGS".into())]).into(),
            )]),
        )),
    });

    let metadata = azalea_item_metadata(&item);

    assert_eq!(metadata.item_uuid, None);
    assert_eq!(
        metadata.skyblock_tag.as_deref(),
        Some("POWER_WITHER_LEGGINGS")
    );
}

#[test]
fn inventory_snapshot_filters_open_gui_slots_to_player_inventory() {
    let mut slots = vec![azalea_inventory::ItemStack::Empty; 90];
    slots[13] =
        azalea_inventory::ItemStack::new(azalea_crate::registry::builtin::ItemKind::StoneButton, 1)
            .with_component(components::CustomName {
                name: "Click an item in your inventory!".into(),
            });
    slots[54] =
        azalea_inventory::ItemStack::new(azalea_crate::registry::builtin::ItemKind::Diamond, 1)
            .with_component(components::CustomName {
                name: "Inventory Item".into(),
            });
    slots[89] =
        azalea_inventory::ItemStack::new(azalea_crate::registry::builtin::ItemKind::NetherStar, 1)
            .with_component(components::CustomName {
                name: "SkyBlock Menu (Click)".into(),
            });

    let items = inventory_items_from_slots(slots);

    assert_eq!(
        items
            .iter()
            .map(|item| (item.item_name.as_str(), item.slot, item.in_hotbar))
            .collect::<Vec<_>>(),
        vec![
            ("Inventory Item", Some(9), false),
            ("SkyBlock Menu (Click)", Some(44), true)
        ]
    );
}

#[test]
fn azalea_set_score_packet_maps_scoreboard_lines() {
    let event = map_azalea_event(azalea_crate::Event::Packet(std::sync::Arc::new(
        azalea_crate::protocol::packets::game::ClientboundGamePacket::SetScore(
            azalea_crate::protocol::packets::game::ClientboundSetScore {
                owner: "hidden-owner".to_string(),
                objective_name: "sidebar".to_string(),
                score: 1,
                display: Some("§6Purse: §e123,456".into()),
                number_format: None,
            },
        ),
    )))
    .unwrap();

    assert_eq!(
        event,
        MinecraftEvent::Scoreboard {
            lines: vec!["§6Purse: §e123,456".to_string()]
        }
    );

    let event = map_azalea_event(azalea_crate::Event::Packet(std::sync::Arc::new(
        azalea_crate::protocol::packets::game::ClientboundGamePacket::SetScore(
            azalea_crate::protocol::packets::game::ClientboundSetScore {
                owner: "hidden-owner".to_string(),
                objective_name: "sidebar".to_string(),
                score: 1,
                display: None,
                number_format: None,
            },
        ),
    )))
    .unwrap();

    assert_eq!(
        event,
        MinecraftEvent::Scoreboard {
            lines: vec!["hidden-owner".to_string()]
        }
    );
}

#[test]
fn azalea_team_packets_preserve_sidebar_owner_on_changes() {
    use azalea_crate::protocol::packets::game::c_set_player_team::Method;

    let mut tracker = AzaleaWindowTracker::default();
    let event = tracker
        .observe_packet(
            &azalea_crate::protocol::packets::game::ClientboundGamePacket::SetPlayerTeam(
                azalea_crate::protocol::packets::game::ClientboundSetPlayerTeam {
                    name: "purse".to_string(),
                    method: Method::Add((
                        team_parameters("§6Purse: ", "§e123,456"),
                        vec!["§1".to_string()],
                    )),
                },
            ),
        )
        .unwrap();

    assert_eq!(
        event,
        MinecraftEvent::Scoreboard {
            lines: vec!["§6Purse: §1§e123,456".to_string()]
        }
    );

    tracker.observe_packet(
        &azalea_crate::protocol::packets::game::ClientboundGamePacket::SetScore(
            azalea_crate::protocol::packets::game::ClientboundSetScore {
                owner: "§1".to_string(),
                objective_name: "sidebar".to_string(),
                score: 12,
                display: None,
                number_format: None,
            },
        ),
    );
    let event = tracker
        .observe_packet(
            &azalea_crate::protocol::packets::game::ClientboundGamePacket::SetPlayerTeam(
                azalea_crate::protocol::packets::game::ClientboundSetPlayerTeam {
                    name: "purse".to_string(),
                    method: Method::Change(team_parameters("§6Purse: ", "§e456,789")),
                },
            ),
        )
        .unwrap();

    assert_eq!(
        event,
        MinecraftEvent::Scoreboard {
            lines: vec!["§6Purse: §1§e456,789".to_string()]
        }
    );
}

#[test]
fn azalea_team_packets_track_join_leave_and_score_order() {
    use azalea_crate::protocol::packets::game::c_set_player_team::Method;

    let mut tracker = AzaleaWindowTracker::default();
    tracker.observe_packet(
        &azalea_crate::protocol::packets::game::ClientboundGamePacket::SetPlayerTeam(
            azalea_crate::protocol::packets::game::ClientboundSetPlayerTeam {
                name: "purse".to_string(),
                method: Method::Add((
                    team_parameters("§6Purse: ", "§e1,000"),
                    vec!["§1".to_string()],
                )),
            },
        ),
    );
    tracker.observe_packet(
        &azalea_crate::protocol::packets::game::ClientboundGamePacket::SetPlayerTeam(
            azalea_crate::protocol::packets::game::ClientboundSetPlayerTeam {
                name: "location".to_string(),
                method: Method::Join(vec!["§2".to_string(), "§2".to_string()]),
            },
        ),
    );
    tracker.observe_packet(
        &azalea_crate::protocol::packets::game::ClientboundGamePacket::SetPlayerTeam(
            azalea_crate::protocol::packets::game::ClientboundSetPlayerTeam {
                name: "location".to_string(),
                method: Method::Change(team_parameters("§a", "Your Island")),
            },
        ),
    );
    tracker.observe_packet(
        &azalea_crate::protocol::packets::game::ClientboundGamePacket::SetScore(
            azalea_crate::protocol::packets::game::ClientboundSetScore {
                owner: "§1".to_string(),
                objective_name: "sidebar".to_string(),
                score: 4,
                display: None,
                number_format: None,
            },
        ),
    );
    let event = tracker
        .observe_packet(
            &azalea_crate::protocol::packets::game::ClientboundGamePacket::SetScore(
                azalea_crate::protocol::packets::game::ClientboundSetScore {
                    owner: "§2".to_string(),
                    objective_name: "sidebar".to_string(),
                    score: 5,
                    display: None,
                    number_format: None,
                },
            ),
        )
        .unwrap();

    assert_eq!(
        event,
        MinecraftEvent::Scoreboard {
            lines: vec![
                "§a§2Your Island".to_string(),
                "§6Purse: §1§e1,000".to_string()
            ]
        }
    );

    let event = tracker
        .observe_packet(
            &azalea_crate::protocol::packets::game::ClientboundGamePacket::SetPlayerTeam(
                azalea_crate::protocol::packets::game::ClientboundSetPlayerTeam {
                    name: "location".to_string(),
                    method: Method::Leave(vec!["§2".to_string()]),
                },
            ),
        )
        .unwrap();

    assert_eq!(
        event,
        MinecraftEvent::Scoreboard {
            lines: vec!["§2".to_string(), "§6Purse: §1§e1,000".to_string()]
        }
    );
}

#[test]
fn azalea_disconnect_reason_maps_kicks_separately() {
    for reason in [
        "Kicked whilst connecting to mini123: You are sending commands too fast",
        "You logged in from another location",
        "Flying is not enabled on this server",
        "We have detected badly behaving modifications being used on your account.",
    ] {
        assert_eq!(
            map_disconnect_reason(Some(reason.to_string())),
            MinecraftEvent::Kicked {
                reason: reason.to_string()
            }
        );
    }
    assert_eq!(
        map_disconnect_reason(Some("Connection closed by remote host".to_string())),
        MinecraftEvent::Disconnected {
            reason: "Connection closed by remote host".to_string()
        }
    );
    assert_eq!(
        map_disconnect_reason(None),
        MinecraftEvent::Disconnected {
            reason: "disconnected".to_string()
        }
    );
}

#[test]
fn azalea_window_tracker_emits_snapshot_after_content_packet() {
    let mut tracker = AzaleaWindowTracker::default();
    let item =
        azalea_inventory::ItemStack::new(azalea_crate::registry::builtin::ItemKind::GoldNugget, 1)
            .with_component(components::CustomName {
                name: "Buy Item Right Now".into(),
            })
            .with_component(components::Lore {
                lines: vec!["Price: 1,000 coins".into()],
            });

    assert!(
        tracker
            .observe_packet(
                &azalea_crate::protocol::packets::game::ClientboundGamePacket::OpenScreen(
                    azalea_crate::protocol::packets::game::ClientboundOpenScreen {
                        container_id: 7,
                        menu_type: azalea_crate::registry::builtin::MenuKind::Generic9x6,
                        title: "BIN Auction View".into(),
                    },
                ),
            )
            .is_none()
    );
    let event = tracker
        .observe_packet(
            &azalea_crate::protocol::packets::game::ClientboundGamePacket::ContainerSetContent(
                azalea_crate::protocol::packets::game::ClientboundContainerSetContent {
                    container_id: 7,
                    state_id: 1,
                    items: vec![azalea_inventory::ItemStack::Empty, item],
                    carried_item: azalea_inventory::ItemStack::Empty,
                },
            ),
        )
        .unwrap();

    assert_eq!(
        event,
        MinecraftEvent::WindowOpen(WindowSnapshot {
            title: "BIN Auction View".to_string(),
            slots: vec![WindowSlot {
                slot: 1,
                name: "gold_nugget".to_string(),
                display_name: "Buy Item Right Now".to_string(),
                lore: vec!["Price: 1,000 coins".to_string()],
                item_uuid: None,
            }],
        })
    );
}

#[test]
fn azalea_window_tracker_updates_slots_and_clears_on_close() {
    let mut tracker = AzaleaWindowTracker::default();
    let item =
        azalea_inventory::ItemStack::new(azalea_crate::registry::builtin::ItemKind::GoldIngot, 1)
            .with_component(components::CustomName {
                name: "Create BIN Auction".into(),
            });

    tracker.observe_packet(
        &azalea_crate::protocol::packets::game::ClientboundGamePacket::OpenScreen(
            azalea_crate::protocol::packets::game::ClientboundOpenScreen {
                container_id: 3,
                menu_type: azalea_crate::registry::builtin::MenuKind::Generic9x6,
                title: "Auction House".into(),
            },
        ),
    );
    let event = tracker
        .observe_packet(
            &azalea_crate::protocol::packets::game::ClientboundGamePacket::ContainerSetSlot(
                azalea_crate::protocol::packets::game::ClientboundContainerSetSlot {
                    container_id: 3,
                    state_id: 2,
                    slot: 31,
                    item_stack: item.clone(),
                },
            ),
        )
        .unwrap();
    assert!(matches!(
        event,
        MinecraftEvent::WindowOpen(WindowSnapshot { title, slots })
            if title == "Auction House"
                && slots.len() == 1
                && slots[0].slot == 31
                && slots[0].display_name == "Create BIN Auction"
    ));

    assert_eq!(
        tracker.observe_packet(
            &azalea_crate::protocol::packets::game::ClientboundGamePacket::ContainerClose(
                azalea_crate::protocol::packets::game::ClientboundContainerClose {
                    container_id: 4,
                },
            ),
        ),
        None
    );
    let event = tracker
        .observe_packet(
            &azalea_crate::protocol::packets::game::ClientboundGamePacket::ContainerSetSlot(
                azalea_crate::protocol::packets::game::ClientboundContainerSetSlot {
                    container_id: 3,
                    state_id: 3,
                    slot: 32,
                    item_stack: item.clone(),
                },
            ),
        )
        .unwrap();
    assert!(matches!(
        event,
        MinecraftEvent::WindowOpen(WindowSnapshot { title, slots })
            if title == "Auction House"
                && slots.len() == 2
                && slots[1].slot == 32
                && slots[1].display_name == "Create BIN Auction"
    ));

    assert_eq!(
        tracker.observe_packet(
            &azalea_crate::protocol::packets::game::ClientboundGamePacket::ContainerClose(
                azalea_crate::protocol::packets::game::ClientboundContainerClose {
                    container_id: 3,
                },
            ),
        ),
        Some(MinecraftEvent::WindowClosed)
    );
    assert!(
        tracker
            .observe_packet(
                &azalea_crate::protocol::packets::game::ClientboundGamePacket::ContainerSetSlot(
                    azalea_crate::protocol::packets::game::ClientboundContainerSetSlot {
                        container_id: 3,
                        state_id: 4,
                        slot: 32,
                        item_stack: item,
                    },
                ),
            )
            .is_none()
    );
}

#[test]
fn azalea_window_tracker_emits_current_snapshot_fallback_changes() {
    let mut tracker = AzaleaWindowTracker::default();
    let snapshot = WindowSnapshot {
        title: "Auction House".to_string(),
        slots: vec![WindowSlot {
            slot: 15,
            name: "gold_ingot".to_string(),
            display_name: "Manage Auctions".to_string(),
            lore: Vec::new(),
            item_uuid: None,
        }],
    };

    assert_eq!(
        tracker.observe_current_window_snapshot(Some(snapshot.clone())),
        Some(MinecraftEvent::WindowOpen(snapshot.clone()))
    );
    assert_eq!(
        tracker.observe_current_window_snapshot(Some(snapshot.clone())),
        None
    );
    assert_eq!(
        tracker.observe_current_window_snapshot(Some(WindowSnapshot {
            title: "Inventory".to_string(),
            slots: Vec::new(),
        })),
        Some(MinecraftEvent::WindowClosed)
    );
    assert_eq!(tracker.observe_current_window_snapshot(None), None);
}

#[test]
fn sign_update_payload_matches_node_sign_entry_shape() {
    assert_eq!(
        sign_update_lines("\"123456\""),
        [
            "\"123456\"".to_string(),
            "^^^^^^^^^^^^^^^".to_string(),
            "    Auction    ".to_string(),
            "     hours     ".to_string(),
        ]
    );
    assert_eq!(
        sign_update_position(azalea_crate::Vec3::new(10.7, 65.0, -3.2)),
        azalea_crate::BlockPos::new(9, 65, -4)
    );
}

fn team_parameters(
    prefix: &str,
    suffix: &str,
) -> azalea_crate::protocol::packets::game::c_set_player_team::Parameters {
    use azalea_crate::protocol::packets::game::c_set_player_team::{
        CollisionRule, NameTagVisibility, Parameters,
    };

    Parameters {
        display_name: String::new().into(),
        options: 0,
        nametag_visibility: NameTagVisibility::Always,
        collision_rule: CollisionRule::Always,
        color: azalea_crate::chat::style::ChatFormatting::Reset,
        player_prefix: prefix.to_string().into(),
        player_suffix: suffix.to_string().into(),
    }
}
