use crate::json5_edit::patch_string_value;
use anyhow::Result;
use std::path::Path;

pub(super) async fn update_config_version(config_path: &Path, version: &str) -> Result<bool> {
    let raw = match tokio::fs::read_to_string(config_path).await {
        Ok(raw) => raw,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let Some(next) = patch_player_head_version(&raw, version) else {
        return Ok(false);
    };
    if next == raw {
        return Ok(true);
    }
    let temp_path = config_path.with_extension("json5.tmp");
    tokio::fs::write(&temp_path, next).await?;
    tokio::fs::rename(&temp_path, config_path).await?;
    Ok(true)
}

#[cfg(test)]
pub(super) fn patch_player_head_version(raw: &str, version: &str) -> Option<String> {
    patch_string_value(raw, "playerHeadVersion", version)
}

#[cfg(not(test))]
fn patch_player_head_version(raw: &str, version: &str) -> Option<String> {
    patch_string_value(raw, "playerHeadVersion", version)
}
