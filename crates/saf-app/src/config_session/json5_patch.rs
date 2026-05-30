use crate::json5_edit::patch_string_value;
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

pub(super) fn persist_config_session(config_path: &Path, session: &str) -> Result<()> {
    let raw = fs::read_to_string(config_path)
        .with_context(|| format!("reading {}", config_path.display()))?;
    let next = patch_config_session(&raw, session)
        .with_context(|| format!("patching session in {}", config_path.display()))?;
    if next == raw {
        return Ok(());
    }

    crate::config_write::write_config_atomic_blocking(config_path, &next)
        .with_context(|| format!("writing {}", config_path.display()))?;
    Ok(())
}

fn patch_config_session(raw: &str, session: &str) -> Option<String> {
    patch_string_value(raw, "session", session)
}
