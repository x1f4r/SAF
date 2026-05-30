use super::controls::{account_action_controls, discord_button};
use super::rendering::{EmbedCard, format_auction_slots, list_or_none};
use super::replies::DiscordInteractionReply;
use super::{account_selection_error, resolve_discord_account};
use crate::live_runtime::formatting::format_coins;
use saf_core::ports::{AccountStats, NotificationKind};
use saf_core::{AccountId, RuntimeOutcome, RuntimeSession};

pub(super) fn help_reply() -> DiscordInteractionReply {
    let names = saf_discord::command_definitions()
        .into_iter()
        .map(|command| format!("/{}", command.name))
        .collect::<Vec<_>>()
        .join(", ");
    let content = format!("SAF commands\n{names}");
    DiscordInteractionReply::embed_with_components(
        content,
        EmbedCard::for_kind(NotificationKind::Info, "SAF commands", names).build(),
        vec![serenity::builder::CreateActionRow::Buttons(vec![
            discord_button(
                "saf:dashboard",
                "Dashboard",
                serenity::all::ButtonStyle::Primary,
            ),
            discord_button("saf:users", "Users", serenity::all::ButtonStyle::Secondary),
            discord_button(
                "saf:messages",
                "Messages",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button("saf:logs", "Logs", serenity::all::ButtonStyle::Secondary),
        ])],
    )
}

pub(super) async fn dashboard_reply(
    session: &RuntimeSession,
    title: &str,
) -> DiscordInteractionReply {
    let selector = session.selector();
    let mut lines = vec![
        title.to_string(),
        format!("Configured: {}", list_or_none(&selector.configured)),
        format!("Running: {}", list_or_none(&selector.running)),
        format!(
            "Default: {}",
            selector.default_ign.as_deref().unwrap_or("none")
        ),
    ];
    if selector.running.len() > 3 {
        lines.push(format!(
            "Showing controls for first 3 running accounts out of {}.",
            selector.running.len()
        ));
    }
    for account in selector.running.iter().take(3) {
        if let Some(account) = AccountId::new(account.clone()) {
            let metrics = discord_account_metrics(session, &account).await;
            lines.push(format_account_status_line(&metrics));
        }
    }

    let mut rows = vec![
        serenity::builder::CreateActionRow::Buttons(vec![
            discord_button(
                "saf:dashboard",
                "Refresh",
                serenity::all::ButtonStyle::Primary,
            ),
            discord_button("saf:users", "Users", serenity::all::ButtonStyle::Secondary),
            discord_button("saf:logs", "Logs", serenity::all::ButtonStyle::Secondary),
            discord_button(
                "saf:connections",
                "Connections",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                "saf:globalStats",
                "Global Stats",
                serenity::all::ButtonStyle::Secondary,
            ),
        ]),
        serenity::builder::CreateActionRow::Buttons(vec![
            discord_button(
                "saf:start",
                "Start Default",
                serenity::all::ButtonStyle::Success,
            ),
            discord_button(
                "saf:stop:all",
                "Stop All",
                serenity::all::ButtonStyle::Danger,
            ),
            discord_button("saf:ping", "Ping", serenity::all::ButtonStyle::Secondary),
            discord_button(
                "saf:messages",
                "Messages",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                "saf:blacklist",
                "Blacklist",
                serenity::all::ButtonStyle::Secondary,
            ),
        ]),
    ];

    rows.extend(selector.running.iter().take(3).map(|account| {
        serenity::builder::CreateActionRow::Buttons(vec![
            discord_button(
                format!("saf:account:{account}"),
                account,
                serenity::all::ButtonStyle::Primary,
            ),
            discord_button(
                format!("saf:queue:{account}"),
                "Queue",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                format!("saf:inventory:{account}"),
                "Inventory",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                format!("saf:reconcile:{account}"),
                "Reconcile",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                format!("saf:stop:{account}"),
                "Stop",
                serenity::all::ButtonStyle::Danger,
            ),
        ])
    }));

    let content = lines.join("\n");
    let description = content
        .split_once('\n')
        .map(|(_, rest)| rest.to_string())
        .unwrap_or_default();
    DiscordInteractionReply::embed_with_components(
        content,
        EmbedCard::for_kind(NotificationKind::Info, title, description).build(),
        rows,
    )
}

pub(super) async fn account_panel_reply(
    session: &RuntimeSession,
    username: &str,
) -> DiscordInteractionReply {
    let Some(account) = resolve_discord_account(session, Some(username)) else {
        return DiscordInteractionReply::content(account_selection_error(session, Some(username)));
    };
    let Some(account_id) = AccountId::new(account.clone()) else {
        return DiscordInteractionReply::content(account_selection_error(session, Some(username)));
    };
    let metrics = discord_account_metrics(session, &account_id).await;
    let content = format_account_panel_content(&metrics);
    DiscordInteractionReply::embed_with_components(
        content,
        account_panel_embed(&metrics),
        account_action_controls(&account_id),
    )
}

fn account_panel_embed(metrics: &DiscordAccountMetrics) -> serenity::builder::CreateEmbed {
    let mut card = EmbedCard::for_kind(
        NotificationKind::Info,
        format!("Account {}", metrics.account),
        "Running: yes",
    )
    .thumbnail(crate::player_head::account_head_thumbnail_url(
        metrics.account.as_str(),
    ))
    .field(
        "Queue",
        metrics
            .queue_size
            .map(|count| count.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        true,
    );
    if let Some(stats) = &metrics.stats {
        card = card
            .field("Profit", format_coins(stats.total_profit), true)
            .field(
                "Bought/Sold",
                format!("{}/{}", stats.bought, stats.sold),
                true,
            )
            .field("Auctions", format_auction_slots(Some(stats)), true)
            .field(
                "Purse",
                stats
                    .purse
                    .map(format_coins)
                    .unwrap_or_else(|| "unknown".to_string()),
                true,
            );
    }
    card = card.field(
        "Cofl",
        metrics
            .connection_id
            .as_ref()
            .and_then(|connection| connection.as_deref())
            .unwrap_or("pending"),
        true,
    );
    card.build()
}

#[derive(Clone, Debug)]
struct DiscordAccountMetrics {
    account: AccountId,
    queue_size: Option<usize>,
    stats: Option<AccountStats>,
    connection_id: Option<Option<String>>,
}

async fn discord_account_metrics(
    session: &RuntimeSession,
    account: &AccountId,
) -> DiscordAccountMetrics {
    let queue_size = match discord_terminal_outcome(session, &format!("{account} queue")).await {
        Some(RuntimeOutcome::QueueSnapshot { queue, .. }) => Some(queue.len()),
        _ => None,
    };
    let stats = match discord_terminal_outcome(session, &format!("{account} stats")).await {
        Some(RuntimeOutcome::StatsSnapshot { stats, .. }) => Some(stats),
        _ => None,
    };
    let connection_id = match discord_terminal_outcome(session, "connections").await {
        Some(RuntimeOutcome::ConnectionsSnapshot { connections }) => connections
            .into_iter()
            .find(|connection| connection.account == *account)
            .map(|connection| connection.connection_id),
        _ => None,
    };

    DiscordAccountMetrics {
        account: account.clone(),
        queue_size,
        stats,
        connection_id,
    }
}

async fn discord_terminal_outcome(session: &RuntimeSession, line: &str) -> Option<RuntimeOutcome> {
    session
        .process_local_command(saf_core::LocalCommand::Terminal {
            line: line.to_string(),
            created_at: None,
        })
        .await
        .ok()
}

fn format_account_status_line(metrics: &DiscordAccountMetrics) -> String {
    let queue = metrics
        .queue_size
        .map(|count| count.to_string())
        .unwrap_or_else(|| "unknown".to_string());
    let purse = metrics
        .stats
        .as_ref()
        .and_then(|stats| stats.purse)
        .map(format_coins)
        .unwrap_or_else(|| "unknown".to_string());
    let auctions = format_auction_slots(metrics.stats.as_ref());
    let connection = metrics
        .connection_id
        .as_ref()
        .and_then(|connection| connection.as_deref())
        .unwrap_or("pending");
    format!(
        "`{}` queue {} auctions {} purse {} Cofl {}",
        metrics.account, queue, auctions, purse, connection
    )
}

fn format_account_panel_content(metrics: &DiscordAccountMetrics) -> String {
    let mut lines = vec![
        format!("Account `{}`", metrics.account),
        "Running: yes".to_string(),
        format!(
            "Queue: {}",
            metrics
                .queue_size
                .map(|count| count.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
    ];
    if let Some(stats) = &metrics.stats {
        lines.push(format!("Profit: {}", format_coins(stats.total_profit)));
        lines.push(format!("Bought/Sold: {}/{}", stats.bought, stats.sold));
        lines.push(format!("Auctions: {}", format_auction_slots(Some(stats))));
        lines.push(format!(
            "Purse: {}",
            stats
                .purse
                .map(format_coins)
                .unwrap_or_else(|| "unknown".to_string())
        ));
    }
    lines.push(format!(
        "Cofl: {}",
        metrics
            .connection_id
            .as_ref()
            .and_then(|connection| connection.as_deref())
            .unwrap_or("pending")
    ));
    lines.join("\n")
}
