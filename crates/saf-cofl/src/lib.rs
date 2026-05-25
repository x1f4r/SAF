mod auth_link;
mod envelope;
mod errors;
mod execute;
mod recorded;
mod socket_link;
mod telemetry;
mod text;
mod wire;

pub use envelope::{
    CoflEnvelope, CoflSettingsMutation, CoflSettingsSummary, LOGGED_OUT_SETTINGS_RECOVERY_COMMAND,
};
pub use errors::{CoflCommandError, CoflParseError};
pub use execute::CoflExecuteInstruction;
pub use recorded::RecordedCoflClient;
pub use socket_link::{
    COFL_SOCKET_VERSION, build_cofl_socket_link, cofl_socket_session_id, redact_cofl_socket_link,
};
pub use telemetry::CoflTelemetryUpdate;
pub use wire::{
    CoflCommandMessage, encode_chat_batch, encode_cofl_command, encode_inventory_snapshot_upload,
    encode_inventory_upload, encode_scoreboard_upload,
};

#[cfg(feature = "http-client")]
pub mod http_client;

#[cfg(feature = "ws-client")]
pub mod ws_client;

#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use url::Url;

#[cfg(test)]
mod tests;
