use saf_core::AccountId;

pub fn account_env_key(prefix: &str, account: &AccountId) -> String {
    let suffix = account
        .as_str()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    format!("{prefix}_{suffix}")
}

pub(super) fn non_empty_env<F>(env: &F, name: &str) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    env(name).is_some_and(|value| !value.trim().is_empty())
}
