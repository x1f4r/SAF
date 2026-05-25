use crate::socket_link::build_cofl_socket_link;
use crate::text::no_color_codes;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum CoflExecuteInstruction {
    SwitchSocket { link: String },
    SendCoflCommand { command: String },
    BlockedChat { command: String },
}

impl CoflExecuteInstruction {
    pub fn from_command(command: &str, ign: &str, session_id: &str) -> Option<Self> {
        let cleaned = no_color_codes(command).trim().to_string();
        if cleaned.is_empty() {
            return None;
        }
        if !cleaned
            .split_whitespace()
            .next()
            .is_some_and(|first| first.eq_ignore_ascii_case("/cofl"))
        {
            return Some(Self::BlockedChat { command: cleaned });
        }
        if cleaned
            .split_whitespace()
            .nth(1)
            .is_some_and(|second| second.eq_ignore_ascii_case("connect"))
        {
            return build_cofl_socket_link(&cleaned, ign, session_id)
                .map(|link| Self::SwitchSocket { link });
        }
        Some(Self::SendCoflCommand { command: cleaned })
    }
}
