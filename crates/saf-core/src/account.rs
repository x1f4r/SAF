use crate::SafConfig;
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccountEnvironment {
    values: BTreeMap<String, String>,
}

impl AccountEnvironment {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn current() -> Self {
        Self::from_entries(
            [
                "SAF_DEFAULT_IGN",
                "SAF_ONLY_IGNS",
                "SAF_ONLY_IGN",
                "SAF_START_DEFAULT_ONLY",
            ]
            .into_iter()
            .filter_map(|key| std::env::var(key).ok().map(|value| (key, value))),
        )
    }

    pub fn from_pairs<const N: usize>(pairs: [(&str, &str); N]) -> Self {
        Self::from_entries(pairs)
    }

    pub fn from_entries<K, V, I>(pairs: I) -> Self
    where
        K: Into<String>,
        V: Into<String>,
        I: IntoIterator<Item = (K, V)>,
    {
        Self {
            values: pairs
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    fn start_default_only_override(&self) -> Option<bool> {
        self.get("SAF_START_DEFAULT_ONLY").map(env_truthy)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountResolver {
    configured: Vec<String>,
    default_ign: String,
    start_default_only: bool,
}

impl AccountResolver {
    pub fn from_config(config: &SafConfig) -> Self {
        Self {
            configured: configured_igns(&config.igns),
            default_ign: config.default_ign.clone(),
            start_default_only: config.start_default_only,
        }
    }

    pub fn configured_igns(&self) -> Vec<String> {
        self.configured.clone()
    }

    pub fn resolve_configured_ign(&self, ign: &str) -> Option<String> {
        resolve_configured_ign(ign, &self.configured)
    }

    pub fn resolve_default_ign(&self, env: &AccountEnvironment) -> Option<String> {
        let configured_default = normalize_ign(
            env.get("SAF_DEFAULT_IGN")
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(&self.default_ign),
        );
        if configured_default.is_empty() {
            return self.configured.first().cloned();
        }
        self.resolve_configured_ign(&configured_default)
    }

    pub fn resolve_only_igns(&self, env: &AccountEnvironment) -> Vec<String> {
        dedupe_igns(
            env.get("SAF_ONLY_IGNS")
                .or_else(|| env.get("SAF_ONLY_IGN"))
                .unwrap_or_default()
                .split(',')
                .map(normalize_ign)
                .filter(|ign| !ign.is_empty())
                .filter_map(|ign| self.resolve_configured_ign(&ign)),
        )
    }

    pub fn should_start_default_only(&self, env: &AccountEnvironment) -> bool {
        if let Some(value) = env.get("SAF_START_DEFAULT_ONLY") {
            return env_truthy(value);
        }
        self.start_default_only
    }

    pub fn resolve_startup_igns(&self, env: &AccountEnvironment) -> Vec<String> {
        let only_igns = self.resolve_only_igns(env);
        if !only_igns.is_empty() {
            return only_igns;
        }

        if self.should_start_default_only(env) {
            return self.resolve_default_ign(env).into_iter().collect();
        }

        self.configured.clone()
    }

    pub fn resolve_start_command_igns(
        &self,
        requested_ign: Option<&str>,
        env: &AccountEnvironment,
    ) -> Vec<String> {
        let requested = requested_ign.map(normalize_ign).unwrap_or_default();
        if !requested.is_empty() {
            return self
                .resolve_configured_ign(&requested)
                .into_iter()
                .collect();
        }

        self.resolve_default_ign(env).into_iter().collect()
    }
}

pub fn normalize_ign(value: impl AsRef<str>) -> String {
    value.as_ref().trim().to_string()
}

pub fn configured_igns(configured: &[String]) -> Vec<String> {
    dedupe_igns(
        configured
            .iter()
            .map(normalize_ign)
            .filter(|ign| !ign.is_empty()),
    )
}

pub fn resolve_configured_ign(ign: &str, configured: &[String]) -> Option<String> {
    let requested = normalize_ign(ign);
    if requested.is_empty() {
        return None;
    }
    configured_igns(configured)
        .into_iter()
        .find(|entry| entry.eq_ignore_ascii_case(&requested))
        .or(Some(requested))
}

fn env_truthy(value: &str) -> bool {
    let value = value.trim();
    value == "1" || value.eq_ignore_ascii_case("true")
}

fn dedupe_igns<I>(igns: I) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    igns.into_iter().fold(Vec::new(), |mut unique, ign| {
        if !unique
            .iter()
            .any(|existing: &String| existing.eq_ignore_ascii_case(&ign))
        {
            unique.push(ign);
        }
        unique
    })
}

pub fn config_with_account_environment(config: &SafConfig, env: &AccountEnvironment) -> SafConfig {
    let resolver = AccountResolver::from_config(config);
    let mut updated = config.clone();
    let only_igns = resolver.resolve_only_igns(env);
    if !only_igns.is_empty() {
        updated.igns = only_igns;
    }

    let configured = configured_igns(&updated.igns);
    let default_candidate = env
        .get("SAF_DEFAULT_IGN")
        .filter(|value| !value.trim().is_empty())
        .map(normalize_ign)
        .unwrap_or_else(|| normalize_ign(&updated.default_ign));
    updated.default_ign = configured
        .iter()
        .find(|ign| ign.eq_ignore_ascii_case(&default_candidate))
        .cloned()
        .or_else(|| configured.first().cloned())
        .unwrap_or_default();

    if let Some(start_default_only) = env.start_default_only_override() {
        updated.start_default_only = start_default_only;
    }

    updated
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SafConfig;

    fn resolver() -> AccountResolver {
        AccountResolver::from_config(&SafConfig {
            igns: vec!["MainAccount".to_string(), "AltAccount".to_string()],
            default_ign: "MainAccount".to_string(),
            ..Default::default()
        })
    }

    #[test]
    fn plain_start_command_resolves_default_account() {
        let resolver = resolver();
        let env = AccountEnvironment::empty();

        assert_eq!(
            resolver.resolve_start_command_igns(None, &env),
            vec!["MainAccount"]
        );
        assert_eq!(
            resolver.resolve_start_command_igns(Some("altaccount"), &env),
            vec!["AltAccount"]
        );
    }

    #[test]
    fn startup_honors_env_limits_and_default_only() {
        let resolver = resolver();

        assert_eq!(
            resolver.resolve_startup_igns(&AccountEnvironment::from_pairs([(
                "SAF_START_DEFAULT_ONLY",
                " TRUE "
            )])),
            vec!["MainAccount"]
        );
        assert_eq!(
            resolver.resolve_startup_igns(&AccountEnvironment::from_pairs([(
                "SAF_ONLY_IGNS",
                "AltAccount, altaccount, MainAccount"
            )])),
            vec!["AltAccount", "MainAccount"]
        );
    }

    #[test]
    fn config_environment_applies_node_startup_overrides() {
        let config = SafConfig {
            igns: vec![
                "MainAccount".to_string(),
                "AltAccount".to_string(),
                " altaccount ".to_string(),
            ],
            default_ign: "MainAccount".to_string(),
            ..Default::default()
        };

        let updated = config_with_account_environment(
            &config,
            &AccountEnvironment::from_pairs([
                ("SAF_DEFAULT_IGN", "AltAccount"),
                ("SAF_START_DEFAULT_ONLY", "1"),
            ]),
        );
        assert_eq!(updated.default_ign, "AltAccount");
        assert!(updated.start_default_only);
        assert_eq!(updated.startup_igns(), vec!["AltAccount"]);

        let limited = config_with_account_environment(
            &config,
            &AccountEnvironment::from_pairs([
                ("SAF_ONLY_IGNS", "AltAccount"),
                ("SAF_DEFAULT_IGN", "MainAccount"),
            ]),
        );
        assert_eq!(limited.igns, vec!["AltAccount"]);
        assert_eq!(limited.default_ign, "AltAccount");
        assert_eq!(limited.startup_igns(), vec!["AltAccount"]);
    }

    #[test]
    fn configured_accounts_dedupe_case_insensitively() {
        assert_eq!(
            configured_igns(&[
                " MainAccount ".to_string(),
                "mainaccount".to_string(),
                "AltAccount".to_string(),
                " ".to_string(),
            ]),
            vec!["MainAccount", "AltAccount"]
        );
    }
}
