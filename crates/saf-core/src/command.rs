use crate::account::resolve_configured_ign;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountSelector {
    pub configured: Vec<String>,
    pub running: Vec<String>,
    pub ask_prefixes: Vec<(String, String)>,
    pub default_ign: Option<String>,
}

impl AccountSelector {
    pub fn default_running_ign(&self) -> Option<String> {
        if self.running.len() == 1 {
            return self.running.first().cloned();
        }

        self.default_ign.as_ref().and_then(|default| {
            self.running
                .iter()
                .find(|ign| ign.eq_ignore_ascii_case(default))
                .cloned()
        })
    }

    pub fn resolve_running(&self, requested: Option<&str>) -> Option<String> {
        if let Some(requested) = requested.filter(|value| !value.trim().is_empty()) {
            if let Some((_, account)) = self
                .ask_prefixes
                .iter()
                .find(|(prefix, _)| prefix.eq_ignore_ascii_case(requested.trim()))
            {
                return self
                    .running
                    .iter()
                    .find(|ign| ign.eq_ignore_ascii_case(account))
                    .cloned();
            }

            let resolved =
                resolve_configured_ign(requested.trim(), &self.configured).unwrap_or_default();
            return self
                .running
                .iter()
                .find(|ign| {
                    ign.eq_ignore_ascii_case(&resolved)
                        || ign.eq_ignore_ascii_case(requested.trim())
                })
                .cloned();
        }
        self.default_running_ign()
    }

    pub fn resolve_start_targets(&self, requested: Option<&str>) -> Vec<String> {
        if let Some(requested) = requested.filter(|value| !value.trim().is_empty()) {
            return self
                .configured
                .iter()
                .find(|ign| ign.eq_ignore_ascii_case(requested.trim()))
                .cloned()
                .into_iter()
                .collect();
        }

        self.default_ign
            .as_ref()
            .and_then(|default| {
                self.configured
                    .iter()
                    .find(|ign| ign.eq_ignore_ascii_case(default))
                    .cloned()
            })
            .or_else(|| self.configured.first().cloned())
            .into_iter()
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutedCommand {
    pub account: Option<String>,
    pub command: String,
    pub message: String,
}

impl RoutedCommand {
    pub fn parse(raw: &str, selector: &AccountSelector) -> Result<Self, String> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err("No command provided.".to_string());
        }

        if selector.running.is_empty() {
            return Err("No accounts are currently running.".to_string());
        }

        let mut parts = raw.splitn(2, char::is_whitespace);
        let first = parts.next().unwrap_or_default();
        let rest = parts.next().unwrap_or_default().trim();

        if let Some(account) = selector.resolve_running(Some(first)) {
            let mut command_parts = rest.splitn(2, char::is_whitespace);
            let command = command_parts.next().unwrap_or_default().to_string();
            if command.is_empty() {
                return Err(format!("No command provided for {account}."));
            }
            return Ok(Self {
                account: Some(account),
                command,
                message: command_parts.next().unwrap_or_default().trim().to_string(),
            });
        }
        if let Some((_, account)) = selector
            .ask_prefixes
            .iter()
            .find(|(prefix, _)| prefix.eq_ignore_ascii_case(first.trim()))
        {
            return Err(format!("{account} is not currently running."));
        }
        if let Some(account) = selector
            .configured
            .iter()
            .find(|account| account.eq_ignore_ascii_case(first.trim()))
        {
            return Err(format!("{account} is not currently running."));
        }

        let account = selector.default_running_ign().ok_or_else(|| {
            let prefixes = if selector.ask_prefixes.is_empty() {
                selector.running.join(", ")
            } else {
                selector
                    .ask_prefixes
                    .iter()
                    .map(|(prefix, _)| prefix.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            format!("Choose an account first. Available prefixes: {prefixes}")
        })?;

        Ok(Self {
            account: Some(account),
            command: first.to_string(),
            message: rest.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn untargeted_commands_prefer_default_running_account() {
        let selector = AccountSelector {
            configured: vec!["Main".to_string(), "Alt".to_string()],
            running: vec!["Main".to_string(), "Alt".to_string()],
            ask_prefixes: Vec::new(),
            default_ign: Some("Alt".to_string()),
        };

        assert_eq!(
            RoutedCommand::parse("/cofl switchregion US", &selector).unwrap(),
            RoutedCommand {
                account: Some("Alt".to_string()),
                command: "/cofl".to_string(),
                message: "switchregion US".to_string(),
            }
        );
    }

    #[test]
    fn explicit_prefix_targets_account() {
        let selector = AccountSelector {
            configured: vec!["MainAccount".to_string(), "AltAccount".to_string()],
            running: vec!["MainAccount".to_string(), "AltAccount".to_string()],
            ask_prefixes: vec![("alt".to_string(), "AltAccount".to_string())],
            default_ign: Some("MainAccount".to_string()),
        };

        assert_eq!(
            RoutedCommand::parse("alt blacklist list", &selector).unwrap(),
            RoutedCommand {
                account: Some("AltAccount".to_string()),
                command: "blacklist".to_string(),
                message: "list".to_string(),
            }
        );
    }

    #[test]
    fn stopped_configured_accounts_do_not_fall_back_to_default() {
        let selector = AccountSelector {
            configured: vec!["MainAccount".to_string(), "AltAccount".to_string()],
            running: vec!["MainAccount".to_string()],
            ask_prefixes: vec![("alt".to_string(), "AltAccount".to_string())],
            default_ign: Some("MainAccount".to_string()),
        };

        assert_eq!(
            RoutedCommand::parse("AltAccount ping", &selector).unwrap_err(),
            "AltAccount is not currently running."
        );
        assert_eq!(
            RoutedCommand::parse("alt ping", &selector).unwrap_err(),
            "AltAccount is not currently running."
        );
    }
}
