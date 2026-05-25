use super::{DEFAULT_BANK_COOLDOWN_MS, env_u64, now_ms};
use anyhow::{Context, Result};
use saf_core::AccountId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(super) struct BankCooldownStore {
    path: PathBuf,
    cooldown_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum BankCooldownDecision {
    Ready,
    Wait { remaining_ms: u64 },
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct BankCooldownEntry {
    #[serde(default, rename = "lastUse")]
    last_use: u64,
}

impl BankCooldownStore {
    pub(super) fn for_base_dir(base_dir: &Path) -> Self {
        let path = std::env::var("SAF_BANK_COOLDOWN_FILE")
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| default_bank_cooldown_path(base_dir));
        Self {
            path,
            cooldown_ms: env_u64("SAF_BANK_COOLDOWN_MS", DEFAULT_BANK_COOLDOWN_MS).max(1),
        }
    }

    #[cfg(test)]
    pub(super) fn new(path: PathBuf, cooldown_ms: u64) -> Self {
        Self {
            path,
            cooldown_ms: cooldown_ms.max(1),
        }
    }

    pub(super) fn try_mark_use(&self, account: &AccountId) -> Result<BankCooldownDecision> {
        self.try_mark_use_at(account, now_ms())
    }

    pub(super) fn try_mark_use_at(
        &self,
        account: &AccountId,
        now: u64,
    ) -> Result<BankCooldownDecision> {
        let key = bank_cooldown_key(account);
        if key.is_empty() {
            return Ok(BankCooldownDecision::Ready);
        }

        let mut data = self.read()?;
        let last_use = data.get(&key).map(|entry| entry.last_use).unwrap_or(0);
        let next_use = last_use.saturating_add(self.cooldown_ms);
        if last_use > 0 && next_use > now {
            return Ok(BankCooldownDecision::Wait {
                remaining_ms: next_use - now,
            });
        }

        data.insert(key, BankCooldownEntry { last_use: now });
        self.write(&data)?;
        Ok(BankCooldownDecision::Ready)
    }

    fn read(&self) -> Result<BTreeMap<String, BankCooldownEntry>> {
        match std::fs::read_to_string(&self.path) {
            Ok(raw) => Ok(serde_json::from_str(&raw).unwrap_or_default()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(error) => Err(error)
                .with_context(|| format!("reading bank cooldown file {}", self.path.display())),
        }
    }

    fn write(&self, data: &BTreeMap<String, BankCooldownEntry>) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("creating bank cooldown directory {}", parent.display())
            })?;
        }
        let file_name = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("bank-cooldowns.json");
        let temp_path = self.path.with_file_name(format!("{file_name}.tmp"));
        let bytes = serde_json::to_vec_pretty(data).context("serializing bank cooldowns")?;
        std::fs::write(&temp_path, bytes)
            .with_context(|| format!("writing bank cooldown temp file {}", temp_path.display()))?;
        std::fs::rename(&temp_path, &self.path).with_context(|| {
            format!(
                "renaming bank cooldown temp file {} to {}",
                temp_path.display(),
                self.path.display()
            )
        })?;
        Ok(())
    }
}

fn default_bank_cooldown_path(base_dir: &Path) -> PathBuf {
    let node_saved_data = base_dir.join("SAF-bot").join("SavedData");
    if node_saved_data.exists() || base_dir.join("SAF-bot").exists() {
        node_saved_data.join("bank-cooldowns.json")
    } else {
        base_dir.join("SavedData").join("bank-cooldowns.json")
    }
}

fn bank_cooldown_key(account: &AccountId) -> String {
    account.as_str().trim().to_ascii_lowercase()
}
