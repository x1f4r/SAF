use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MovementFlags {
    #[serde(default, rename = "onGround")]
    pub on_ground: Option<bool>,
    #[serde(default, rename = "hasHorizontalCollision")]
    pub has_horizontal_collision: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedMovementFlags {
    #[serde(rename = "onGround")]
    pub on_ground: bool,
    #[serde(rename = "hasHorizontalCollision")]
    pub has_horizontal_collision: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MovementPacket {
    #[serde(default, rename = "onGround")]
    pub on_ground: Option<bool>,
    #[serde(default, rename = "hasHorizontalCollision")]
    pub has_horizontal_collision: Option<bool>,
    #[serde(default)]
    pub flags: Option<MovementFlags>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NormalizedMovementPacket {
    #[serde(default, rename = "onGround")]
    pub on_ground: Option<bool>,
    #[serde(default, rename = "hasHorizontalCollision")]
    pub has_horizontal_collision: Option<bool>,
    pub flags: NormalizedMovementFlags,
}

pub fn normalize_movement_packet(packet: MovementPacket) -> NormalizedMovementPacket {
    let flags = packet.flags.unwrap_or_default();
    let on_ground = flags.on_ground.or(packet.on_ground).unwrap_or(false);
    let has_horizontal_collision = flags
        .has_horizontal_collision
        .or(packet.has_horizontal_collision)
        .unwrap_or(false);
    NormalizedMovementPacket {
        on_ground: packet.on_ground,
        has_horizontal_collision: packet.has_horizontal_collision,
        flags: NormalizedMovementFlags {
            on_ground,
            has_horizontal_collision,
        },
    }
}

pub fn is_movement_packet_name(name: &str) -> bool {
    matches!(name, "position" | "position_look" | "look" | "flying")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn movement_packets_are_normalized_for_modern_flags() {
        assert_eq!(
            normalize_movement_packet(MovementPacket {
                on_ground: Some(true),
                ..Default::default()
            })
            .flags,
            NormalizedMovementFlags {
                on_ground: true,
                has_horizontal_collision: false
            }
        );
        assert_eq!(
            normalize_movement_packet(MovementPacket {
                flags: Some(MovementFlags {
                    on_ground: Some(false),
                    has_horizontal_collision: Some(true)
                }),
                ..Default::default()
            })
            .flags,
            NormalizedMovementFlags {
                on_ground: false,
                has_horizontal_collision: true
            }
        );
        assert!(
            !normalize_movement_packet(MovementPacket {
                on_ground: Some(true),
                flags: Some(MovementFlags {
                    on_ground: Some(false),
                    has_horizontal_collision: None
                }),
                ..Default::default()
            })
            .flags
            .on_ground
        );
    }
}
