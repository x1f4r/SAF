use std::path::Path;

use crate::live_preflight::{LivePreflightCheck, LivePreflightStatus};

pub(super) fn feature_check(
    checks: &mut Vec<LivePreflightCheck>,
    name: &str,
    enabled: bool,
    required: bool,
    pass_message: &str,
    fail_message: &str,
) {
    if enabled {
        checks.push(pass(name, pass_message));
    } else if required {
        checks.push(fail(name, fail_message));
    } else {
        checks.push(warn(name, fail_message));
    }
}

pub(super) fn non_empty_env<F>(env: &F, name: &str) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    env(name).is_some_and(|value| !value.trim().is_empty())
}

pub(super) fn truthy_env<F>(env: &F, name: &str) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    env(name).is_some_and(|value| {
        let value = value.trim();
        value == "1" || value.eq_ignore_ascii_case("true")
    })
}

pub(super) fn parent_exists(path: &Path) -> bool {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .exists()
}

pub(super) fn pass(name: impl Into<String>, message: impl Into<String>) -> LivePreflightCheck {
    LivePreflightCheck {
        name: name.into(),
        status: LivePreflightStatus::Pass,
        message: message.into(),
    }
}

pub(super) fn warn(name: impl Into<String>, message: impl Into<String>) -> LivePreflightCheck {
    LivePreflightCheck {
        name: name.into(),
        status: LivePreflightStatus::Warn,
        message: message.into(),
    }
}

pub(super) fn fail(name: impl Into<String>, message: impl Into<String>) -> LivePreflightCheck {
    LivePreflightCheck {
        name: name.into(),
        status: LivePreflightStatus::Fail,
        message: message.into(),
    }
}
