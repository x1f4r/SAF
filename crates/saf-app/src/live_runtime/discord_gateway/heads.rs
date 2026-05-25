use super::rendering::truncate_discord;
use super::replies::DiscordInteractionReply;
use super::resolve_discord_account;
use crate::player_head::{PlayerHeadRefreshReport, PlayerHeadRefresher};
use saf_core::{AccountId, RuntimeSession};

pub(super) async fn refresh_heads_reply(
    session: &RuntimeSession,
    head_refresher: Option<&PlayerHeadRefresher>,
    username: Option<&str>,
) -> DiscordInteractionReply {
    let Some(head_refresher) = head_refresher else {
        return DiscordInteractionReply::content("Player-head refresh is unavailable.");
    };
    let (targets, unresolved) = refresh_head_targets(session, username);
    match head_refresher.refresh(&targets).await {
        Ok(mut report) => {
            if let Some(unresolved) = unresolved {
                report.skipped.insert(0, unresolved);
            }
            DiscordInteractionReply::content(format_player_head_report(&report, username))
        }
        Err(error) => DiscordInteractionReply::content(format!("Runtime error: {error}")),
    }
}

fn refresh_head_targets(
    session: &RuntimeSession,
    username: Option<&str>,
) -> (Vec<AccountId>, Option<String>) {
    if let Some(username) = username.filter(|value| !value.trim().is_empty()) {
        if let Some(account) = resolve_discord_account(session, Some(username))
            && let Some(account) = AccountId::new(account)
        {
            return (vec![account], None);
        }
        return (Vec::new(), Some(username.trim().to_string()));
    }

    (
        session
            .running_accounts()
            .into_iter()
            .filter_map(AccountId::new)
            .collect(),
        None,
    )
}

fn format_player_head_report(report: &PlayerHeadRefreshReport, requested: Option<&str>) -> String {
    let mut lines = Vec::new();
    lines.push("Player heads refreshed.".to_string());
    lines.push(format!("Cache version: `{}`", report.version));
    lines.push(if report.config_updated {
        "Config: `branding.playerHeadVersion` updated.".to_string()
    } else {
        "Config: `branding.playerHeadVersion` was not found.".to_string()
    });

    if report.refreshed.is_empty() {
        lines.push("Refreshed accounts: none.".to_string());
    } else {
        lines.push("Refreshed accounts:".to_string());
        lines.extend(
            report
                .refreshed
                .iter()
                .take(8)
                .map(|entry| format!("`{}` -> {}", entry.username, entry.head_url)),
        );
        if report.refreshed.len() > 8 {
            lines.push(format!(
                "...and {} more account(s).",
                report.refreshed.len() - 8
            ));
        }
    }

    if !report.skipped.is_empty() {
        lines.push(format!(
            "Skipped: {}.",
            report
                .skipped
                .iter()
                .take(12)
                .map(|account| format!("`{account}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if requested.is_some() && report.refreshed.is_empty() {
        lines.push(
            "The cache version was still saved, so the account will use a fresh URL after it is available."
                .to_string(),
        );
    }

    truncate_discord(&lines.join("\n"))
}
