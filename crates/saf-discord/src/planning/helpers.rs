use super::{DiscordCommandPlan, DiscordControllerAction};
use saf_core::LocalCommand;

pub(in crate::planning) fn controller(action: DiscordControllerAction) -> DiscordCommandPlan {
    DiscordCommandPlan::ControllerAction { action }
}

pub(in crate::planning) fn local_terminal(line: String) -> DiscordCommandPlan {
    DiscordCommandPlan::LocalCommand {
        command: LocalCommand::Terminal {
            line,
            created_at: None,
        },
    }
}

pub(in crate::planning) fn targeted_line<'a, I>(
    username: Option<&str>,
    command: impl Into<String>,
    args: I,
) -> String
where
    I: IntoIterator<Item = &'a str>,
{
    let mut parts = Vec::new();
    if let Some(username) = username {
        parts.push(username.to_string());
    }
    parts.push(command.into());
    parts.extend(
        args.into_iter()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string),
    );
    parts.join(" ")
}
