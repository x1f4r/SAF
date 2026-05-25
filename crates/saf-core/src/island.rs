use crate::protocol_text::message_text;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Locraw {
    #[serde(default)]
    pub server: Option<String>,
    #[serde(default)]
    pub lobbyname: Option<String>,
    #[serde(default)]
    pub map: Option<String>,
}

pub fn parse_locraw_message(message: &Value) -> Option<Locraw> {
    let text = message
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| message_text(message))
        .trim()
        .to_string();
    if !text.starts_with('{') {
        return None;
    }
    serde_json::from_str::<Locraw>(&text).ok()
}

pub fn get_locraw_move(
    locraw: &Locraw,
    base_message: &str,
    use_cookie: bool,
) -> Option<&'static str> {
    if locraw.server.as_deref() == Some("limbo") {
        return Some("/l");
    }
    if locraw
        .lobbyname
        .as_ref()
        .is_some_and(|value| !value.is_empty())
    {
        return Some("/skyblock");
    }
    if locraw.map.as_deref() != Some(base_message) {
        return Some(if use_cookie { "/is" } else { "/hub" });
    }
    None
}

pub fn scoreboard_shows_own_island(lines: &[String]) -> bool {
    lines.iter().any(|line| line.contains("Your Island"))
}

pub fn scoreboard_shows_guest_island(lines: &[String]) -> bool {
    lines.iter().any(|line| line.contains('\u{270c}'))
}

pub fn scoreboard_matches_target_island(
    lines: &[String],
    use_cookie: bool,
    visit_friend: Option<&str>,
) -> bool {
    if !use_cookie {
        return false;
    }
    if visit_friend.is_some_and(|value| !value.trim().is_empty()) {
        return scoreboard_shows_guest_island(lines) && !scoreboard_shows_own_island(lines);
    }
    scoreboard_shows_own_island(lines)
}

pub fn should_visit_friend(lines: &[String], use_cookie: bool, visit_friend: Option<&str>) -> bool {
    use_cookie
        && visit_friend.is_some_and(|value| !value.trim().is_empty())
        && (!scoreboard_shows_guest_island(lines) || scoreboard_shows_own_island(lines))
}

pub fn use_stationary_island_mode(value: Option<&str>) -> bool {
    value != Some("0")
}

pub fn is_unsafe_spawn_block_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    name.contains("water") || name.contains("lava") || name.contains("bubble_column")
}

pub fn is_unsafe_spawn(block_names: &[String]) -> bool {
    block_names
        .iter()
        .any(|name| is_unsafe_spawn_block_name(name))
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum IslandReadiness {
    ReadyOnPrivateIsland,
    ConfirmLocation,
    MoveToHub,
}

pub fn skyblock_join_readiness(use_cookie: bool, scoreboard_lines: &[String]) -> IslandReadiness {
    if !use_cookie {
        return IslandReadiness::MoveToHub;
    }
    if scoreboard_shows_own_island(scoreboard_lines) {
        IslandReadiness::ReadyOnPrivateIsland
    } else {
        IslandReadiness::ConfirmLocation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn locraw_parsing_reads_json_from_messages() {
        assert_eq!(
            parse_locraw_message(&json!("{\"server\":\"mini1A\",\"map\":\"Private Island\"}")),
            Some(Locraw {
                server: Some("mini1A".to_string()),
                lobbyname: None,
                map: Some("Private Island".to_string())
            })
        );
        assert_eq!(parse_locraw_message(&json!({ "text": "Welcome" })), None);
        assert_eq!(parse_locraw_message(&json!("{\"map\":")), None);
    }

    #[test]
    fn locraw_movement_keeps_cookie_bots_on_private_island() {
        assert_eq!(
            get_locraw_move(
                &Locraw {
                    lobbyname: Some("mini1A".to_string()),
                    ..Default::default()
                },
                "Private Island",
                true
            ),
            Some("/skyblock")
        );
        assert_eq!(
            get_locraw_move(
                &Locraw {
                    map: Some("Hub".to_string()),
                    ..Default::default()
                },
                "Private Island",
                true
            ),
            Some("/is")
        );
        assert_eq!(
            get_locraw_move(
                &Locraw {
                    map: Some("Private Island".to_string()),
                    ..Default::default()
                },
                "Private Island",
                true
            ),
            None
        );
    }

    #[test]
    fn private_island_and_unsafe_spawn_detection_match_node_rules() {
        assert!(scoreboard_shows_own_island(&[
            "Your Island".to_string(),
            "Purse: 1,000".to_string()
        ]));
        assert!(!scoreboard_shows_own_island(&["Village".to_string()]));
        assert!(is_unsafe_spawn(&["water".to_string(), "air".to_string()]));
        assert!(!is_unsafe_spawn(&["stone".to_string(), "air".to_string()]));
    }

    #[test]
    fn visit_friend_target_detection_matches_node_scoreboard_rules() {
        let own = ["Your Island".to_string(), "Purse: 1,000".to_string()];
        let guest = ["Guest Island".to_string(), "\u{270c} Guests: 1".to_string()];

        assert!(should_visit_friend(&own, true, Some("Friend")));
        assert!(!scoreboard_matches_target_island(
            &own,
            true,
            Some("Friend")
        ));
        assert!(!should_visit_friend(&guest, true, Some("Friend")));
        assert!(scoreboard_matches_target_island(
            &guest,
            true,
            Some("Friend")
        ));
        assert!(scoreboard_matches_target_island(&own, true, None));
    }
}
