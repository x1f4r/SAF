mod env;
mod json5_patch;

use anyhow::Result;
pub use env::account_env_key;
use env::non_empty_env;
use json5_patch::persist_config_session;
use saf_core::{AccountId, SafConfig};
use std::path::Path;

pub fn ensure_cofl_session<F>(
    config: &mut SafConfig,
    accounts: &[AccountId],
    config_path: &Path,
    env: F,
) -> Result<Option<String>>
where
    F: Fn(&str) -> Option<String>,
{
    if !should_generate_cofl_session(config, accounts, &env) {
        return Ok(None);
    }

    let session = uuid::Uuid::new_v4().to_string();
    persist_config_session(config_path, &session)?;
    config.session = session.clone();
    Ok(Some(session))
}

pub fn should_generate_cofl_session<F>(config: &SafConfig, accounts: &[AccountId], env: &F) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    if !config.session.trim().is_empty()
        || non_empty_env(env, "SAF_COFL_SESSION")
        || accounts.is_empty()
    {
        return false;
    }

    accounts.iter().any(|account| {
        let key = account_env_key("SAF_COFL_SOCKET", account);
        let Some(link) = env(&key)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        else {
            return true;
        };

        saf_cofl::cofl_socket_session_id(&link).is_none()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, fs};

    fn env(values: BTreeMap<&'static str, &'static str>) -> impl Fn(&str) -> Option<String> {
        move |name| values.get(name).map(|value| value.to_string())
    }

    fn config() -> SafConfig {
        SafConfig {
            igns: vec!["Main".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        }
    }

    #[test]
    fn generated_cofl_session_is_persisted_without_losing_json5_context() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("config.json5");
        fs::write(
            &config_path,
            r#"{
  // keep this operator note
  igns: ["Main"],
  session:
    "",
}"#,
        )
        .unwrap();

        let mut config = config();
        let account = AccountId::new("Main").unwrap();
        let generated =
            ensure_cofl_session(&mut config, &[account], &config_path, env(BTreeMap::new()))
                .unwrap()
                .unwrap();
        let raw = fs::read_to_string(config_path).unwrap();

        assert_eq!(config.session, generated);
        assert!(uuid::Uuid::parse_str(&generated).is_ok());
        assert!(raw.contains("// keep this operator note"));
        assert!(raw.contains(&format!("\"{generated}\"")));
    }

    #[test]
    fn generated_cofl_session_ignores_comments_and_string_values() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("config.json5");
        fs::write(
            &config_path,
            r#"{
  // session: "comment-only"
  note: "session: not the config field",
  session: "",
}"#,
        )
        .unwrap();

        let mut config = config();
        let account = AccountId::new("Main").unwrap();
        let generated =
            ensure_cofl_session(&mut config, &[account], &config_path, env(BTreeMap::new()))
                .unwrap()
                .unwrap();
        let raw = fs::read_to_string(config_path).unwrap();

        assert_eq!(config.session, generated);
        assert!(raw.contains(r#"// session: "comment-only""#));
        assert!(raw.contains(r#"note: "session: not the config field""#));
        assert!(raw.contains(&format!("session: \"{generated}\"")));
    }

    #[test]
    fn cofl_session_is_not_generated_when_env_session_exists() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("config.json5");
        let raw = r#"{ igns: ["Main"], session: "" }"#;
        fs::write(&config_path, raw).unwrap();

        let mut config = config();
        let account = AccountId::new("Main").unwrap();
        let generated = ensure_cofl_session(
            &mut config,
            &[account],
            &config_path,
            env(BTreeMap::from([("SAF_COFL_SESSION", "configured")])),
        )
        .unwrap();

        assert!(generated.is_none());
        assert!(config.session.is_empty());
        assert_eq!(fs::read_to_string(config_path).unwrap(), raw);
    }

    #[test]
    fn cofl_session_is_not_generated_when_every_account_socket_has_embedded_session() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("config.json5");
        let raw = r#"{ igns: ["Main"], session: "" }"#;
        fs::write(&config_path, raw).unwrap();

        let mut config = config();
        let account = AccountId::new("Main").unwrap();
        let generated = ensure_cofl_session(
            &mut config,
            &[account],
            &config_path,
            env(BTreeMap::from([(
                "SAF_COFL_SOCKET_MAIN",
                "wss://sky-us.coflnet.com/modsocket?SId=fake",
            )])),
        )
        .unwrap();

        assert!(generated.is_none());
        assert!(config.session.is_empty());
        assert_eq!(fs::read_to_string(config_path).unwrap(), raw);
    }

    #[test]
    fn cofl_session_is_generated_for_accounts_without_embedded_socket_session() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("config.json5");
        fs::write(&config_path, r#"{ igns: ["Main", "Alt"], session: "" }"#).unwrap();

        let mut config = SafConfig {
            igns: vec!["Main".to_string(), "Alt".to_string()],
            default_ign: "Main".to_string(),
            ..SafConfig::default()
        };
        let accounts = vec![
            AccountId::new("Main").unwrap(),
            AccountId::new("Alt").unwrap(),
        ];
        let generated = ensure_cofl_session(
            &mut config,
            &accounts,
            &config_path,
            env(BTreeMap::from([(
                "SAF_COFL_SOCKET_MAIN",
                "wss://sky-us.coflnet.com/modsocket?SId=fake",
            )])),
        )
        .unwrap()
        .unwrap();

        assert_eq!(config.session, generated);
        assert!(uuid::Uuid::parse_str(&generated).is_ok());
        assert!(
            fs::read_to_string(config_path)
                .unwrap()
                .contains(&generated)
        );
    }

    #[test]
    fn cofl_session_is_generated_when_account_socket_lacks_embedded_session() {
        let temp = tempfile::tempdir().unwrap();
        let config_path = temp.path().join("config.json5");
        fs::write(&config_path, r#"{ igns: ["Main"], session: "" }"#).unwrap();

        let mut config = config();
        let account = AccountId::new("Main").unwrap();
        let generated = ensure_cofl_session(
            &mut config,
            &[account],
            &config_path,
            env(BTreeMap::from([(
                "SAF_COFL_SOCKET_MAIN",
                "wss://sky-us.coflnet.com/modsocket?region=us",
            )])),
        )
        .unwrap()
        .unwrap();

        assert_eq!(config.session, generated);
        assert!(uuid::Uuid::parse_str(&generated).is_ok());
    }

    #[test]
    fn account_env_keys_match_runtime_convention() {
        let account = AccountId::new("Main-Alt").unwrap();
        assert_eq!(
            account_env_key("SAF_COFL_SOCKET", &account),
            "SAF_COFL_SOCKET_MAIN_ALT"
        );
    }
}
