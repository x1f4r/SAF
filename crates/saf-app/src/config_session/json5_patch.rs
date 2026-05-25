use crate::json5_edit::patch_string_value;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn persist_config_session(config_path: &Path, session: &str) -> Result<()> {
    let raw = fs::read_to_string(config_path)
        .with_context(|| format!("reading {}", config_path.display()))?;
    let next = patch_config_session(&raw, session)
        .with_context(|| format!("patching session in {}", config_path.display()))?;
    if next == raw {
        return Ok(());
    }

    let temp_path = temp_config_path(config_path);
    fs::write(&temp_path, next).with_context(|| format!("writing {}", temp_path.display()))?;
    fs::rename(&temp_path, config_path)
        .with_context(|| format!("replacing {}", config_path.display()))?;
    Ok(())
}

fn patch_config_session(raw: &str, session: &str) -> Option<String> {
    patch_string_value(raw, "session", session)
}

fn temp_config_path(path: &Path) -> PathBuf {
    let mut temp = path.to_path_buf();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| format!("{extension}.tmp"))
        .unwrap_or_else(|| "tmp".to_string());
    temp.set_extension(extension);
    temp
}
