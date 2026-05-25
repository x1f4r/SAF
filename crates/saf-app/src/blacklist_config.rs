mod document;
mod patch;
mod rules;
mod snapshot;

use async_trait::async_trait;
use document::{config_array_or_effective, parse_config_value, set_config_array};
use patch::{patch_config_array, temp_config_path};
use rules::{entry_expired, entry_key, normalize_update_value, update_expires_at};
use saf_core::ports::{BlacklistStore, PortError};
use saf_core::{
    AccountId, BlacklistAction, BlacklistApplyResult, BlacklistField, BlacklistPolicy,
    BlacklistPolicyHandle, BlacklistRequest, BlacklistScope, BlacklistUpdate, SafConfig,
};
use snapshot::{blacklist_snapshot, update_summary};
use std::path::PathBuf;

#[cfg(test)]
use serde_json::{Value, json};

#[derive(Clone, Debug)]
pub struct FileBlacklistStore {
    config_path: PathBuf,
    policy: BlacklistPolicyHandle,
}

impl FileBlacklistStore {
    pub fn new(config_path: impl Into<PathBuf>, policy: BlacklistPolicyHandle) -> Self {
        Self {
            config_path: config_path.into(),
            policy,
        }
    }

    async fn load_raw(&self) -> Result<String, PortError> {
        match tokio::fs::read_to_string(&self.config_path).await {
            Ok(raw) => Ok(raw),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok("{}".to_string()),
            Err(error) => Err(PortError::Failed(format!(
                "read {}: {error}",
                self.config_path.display()
            ))),
        }
    }

    async fn write_raw(&self, raw: String) -> Result<(), PortError> {
        let temp_path = temp_config_path(&self.config_path);
        tokio::fs::write(&temp_path, raw).await.map_err(|error| {
            PortError::Failed(format!("write {}: {error}", temp_path.display()))
        })?;
        tokio::fs::rename(&temp_path, &self.config_path)
            .await
            .map_err(|error| {
                PortError::Failed(format!(
                    "replace {} with {}: {error}",
                    self.config_path.display(),
                    temp_path.display()
                ))
            })
    }

    fn reload_policy(&self, raw: &str) -> Result<(), PortError> {
        self.reload_policy_from_config(raw)?;
        Ok(())
    }

    fn reload_policy_from_config(&self, raw: &str) -> Result<SafConfig, PortError> {
        let config = SafConfig::from_json5_str(raw)
            .map_err(|error| PortError::Failed(format!("parse config for blacklist: {error}")))?;
        self.policy.replace(BlacklistPolicy::from_config(
            &config.do_not_buy,
            &config.do_not_relist,
        ));
        Ok(config)
    }

    async fn list(&self, request: BlacklistRequest) -> Result<BlacklistApplyResult, PortError> {
        let raw = self.load_raw().await?;
        let config = self.reload_policy_from_config(&raw)?;
        Ok(BlacklistApplyResult {
            request,
            changed: false,
            summary: serde_json::to_string_pretty(&blacklist_snapshot(&config))
                .unwrap_or_else(|_| "{}".to_string()),
        })
    }

    async fn update(
        &self,
        request: BlacklistRequest,
        update: &BlacklistUpdate,
    ) -> Result<BlacklistApplyResult, PortError> {
        let raw = self.load_raw().await?;
        let mut root = parse_config_value(&raw)?;
        let config = SafConfig::from_json5_str(&raw)
            .map_err(|error| PortError::Failed(format!("parse config for blacklist: {error}")))?;
        let field = field_name(&update.field);
        let section = section_name(&update.scope);
        let normalized = normalize_update_value(&update.field, &update.value)?;
        let expires_at = update_expires_at(update)?;
        let current = config_array_or_effective(&root, &config, section, field);
        let now = saf_core::time::unix_millis();
        let value_key = normalized.key(&update.field);
        let mut changed = false;

        let next_entries = match update.action {
            BlacklistAction::List => current,
            BlacklistAction::Add => {
                let mut active = current
                    .into_iter()
                    .filter(|entry| !entry_expired(entry, now))
                    .collect::<Vec<_>>();
                let already_present = active
                    .iter()
                    .filter_map(|entry| entry_key(&update.field, entry))
                    .any(|key| key == value_key);
                if !already_present {
                    active.push(normalized.to_entry(expires_at));
                    changed = true;
                }
                active
            }
            BlacklistAction::Remove => {
                let before = current.len();
                let filtered = current
                    .into_iter()
                    .filter(|entry| {
                        entry_key(&update.field, entry).is_none_or(|key| key != value_key)
                    })
                    .collect::<Vec<_>>();
                changed = filtered.len() != before;
                filtered
            }
        };

        set_config_array(&mut root, section, field, next_entries.clone())?;
        let next_raw = if changed {
            patch_config_array(&raw, section, field, &next_entries)
                .unwrap_or_else(|| serde_json::to_string_pretty(&root).unwrap_or_default())
        } else {
            raw
        };

        self.reload_policy(&next_raw)?;
        if changed {
            self.write_raw(next_raw).await?;
        }

        Ok(BlacklistApplyResult {
            request,
            changed,
            summary: update_summary(update, &normalized, expires_at, changed),
        })
    }
}

#[async_trait]
impl BlacklistStore for FileBlacklistStore {
    async fn apply(
        &self,
        _account: &AccountId,
        request: BlacklistRequest,
    ) -> Result<BlacklistApplyResult, PortError> {
        match &request {
            BlacklistRequest::List => self.list(request).await,
            BlacklistRequest::Update(update) => self.update(request.clone(), update).await,
        }
    }
}

fn section_name(scope: &BlacklistScope) -> &'static str {
    match scope {
        BlacklistScope::Buy => "doNotBuy",
        BlacklistScope::Relist => "doNotRelist",
    }
}

fn field_name(field: &BlacklistField) -> &'static str {
    match field {
        BlacklistField::Tag => "tags",
        BlacklistField::Name => "names",
        BlacklistField::Enchant => "enchantments",
        BlacklistField::ItemEnchant => "itemEnchantments",
    }
}

#[cfg(test)]
mod tests;
