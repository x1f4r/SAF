use super::super::super::formatting::format_coins;
use super::blacklist::format_blacklist_snapshot;
use super::diagnostics::format_gui_slot_diagnostics;
use super::embed::EmbedCard;
use super::helpers::{
    compact_json, format_auction_slots, format_ms, list_or_none, truncate_discord,
};
use super::planned::format_planned_directive;
use saf_core::engine::FlipOutcome;
use saf_core::ports::{NotificationKind, ScheduledAccountAction};
use saf_core::{AccountId, QueueEntry, RuntimeOutcome};
use serenity::builder::CreateEmbed;

pub(in crate::live_runtime::discord_gateway) fn format_runtime_outcome(
    outcome: &RuntimeOutcome,
) -> String {
    let content = match outcome {
        RuntimeOutcome::UsersSnapshot {
            configured,
            running,
            default,
        } => format!(
            "Users\nConfigured: {}\nRunning: {}\nDefault: {}",
            list_or_none(configured),
            list_or_none(running),
            default.as_deref().unwrap_or("none")
        ),
        RuntimeOutcome::GlobalStatsSnapshot {
            accounts,
            total_profit,
            bought,
            sold,
        } => {
            let mut lines = vec![
                "Global Stats".to_string(),
                format!("Total profit: {}", format_coins(*total_profit)),
                format!("Bought: {bought}"),
                format!("Sold: {sold}"),
            ];
            lines.extend(accounts.iter().take(10).map(|snapshot| {
                format!(
                    "`{}` profit {} auctions {} purse {}",
                    snapshot.account,
                    format_coins(snapshot.stats.total_profit),
                    format_auction_slots(Some(&snapshot.stats)),
                    snapshot
                        .stats
                        .purse
                        .map(format_coins)
                        .unwrap_or_else(|| "unknown".to_string())
                )
            }));
            lines.join("\n")
        }
        RuntimeOutcome::StatsSnapshot { account, stats }
        | RuntimeOutcome::ProfitSnapshot { account, stats } => format_account_stats(account, stats),
        RuntimeOutcome::PingSnapshot { account, ping } => format!(
            "Ping `{account}`\nCofl: {}\nDelay: {}\nHypixel: {}",
            format_ms(ping.cofl_ping_ms),
            format_ms(ping.cofl_delay_ms),
            format_ms(ping.hypixel_ping_ms)
        ),
        RuntimeOutcome::QueueSnapshot { account, queue } => format_queue_snapshot(account, queue),
        RuntimeOutcome::ConnectionsSnapshot { connections } => {
            let mut lines = vec!["Connections".to_string()];
            if connections.is_empty() {
                lines.push("none".to_string());
            }
            lines.extend(connections.iter().map(|connection| {
                format!(
                    "`{}`: {}",
                    connection.account,
                    connection.connection_id.as_deref().unwrap_or("pending")
                )
            }));
            lines.join("\n")
        }
        RuntimeOutcome::LogSnapshot { snapshot } => {
            if !snapshot.exists {
                format!("Logs\n{} does not exist.", snapshot.path)
            } else if snapshot.lines.is_empty() {
                format!("Logs\n{} is empty.", snapshot.path)
            } else {
                format!("Logs\n{}", snapshot.lines.join("\n"))
            }
        }
        RuntimeOutcome::InventorySnapshot { snapshot } => {
            let mut lines = vec![format!(
                "Inventory `{}`\nItems: {}",
                snapshot.account,
                snapshot.items.len()
            )];
            lines.extend(snapshot.items.iter().take(12).map(|item| {
                format!(
                    "- {}{}{}",
                    item.item_name,
                    item.uuid
                        .as_deref()
                        .map(|uuid| format!(" `{uuid}`"))
                        .unwrap_or_default(),
                    item.slot
                        .map(|slot| format!(" slot {slot}"))
                        .unwrap_or_default()
                )
            }));
            lines.join("\n")
        }
        RuntimeOutcome::InventoryListingsQueued { account, queued } => {
            format!("Queued {queued} inventory listing(s) for `{account}`.")
        }
        RuntimeOutcome::DelistAllQueued { account, queued } => {
            format!("Queued {queued} delist action(s) for `{account}`.")
        }
        RuntimeOutcome::GuiSlotDiagnostics { diagnostics } => {
            format_gui_slot_diagnostics(diagnostics)
        }
        RuntimeOutcome::QueueCleared { account, removed } => {
            format!("Cleared {removed} queued action(s) for `{account}`.")
        }
        RuntimeOutcome::QueueEntryCancelled {
            account,
            entry,
            index,
        } => match entry {
            Some(entry) => format!(
                "Cancelled queue entry {index} for `{account}` ({} priority {}).",
                entry.state.as_str(),
                entry.priority
            ),
            None => format!("No queue entry at index {index} for `{account}`."),
        },
        RuntimeOutcome::QueuesCleared { accounts } => {
            let total = accounts.iter().map(|entry| entry.removed).sum::<usize>();
            let mut lines = vec![format!("Cleared {total} queued action(s).")];
            lines.extend(
                accounts
                    .iter()
                    .map(|entry| format!("`{}`: {}", entry.account, entry.removed)),
            );
            lines.join("\n")
        }
        RuntimeOutcome::SavedDataCleared {
            account,
            queue_removed,
            bid_data_cleared,
        } => format!(
            "Cleared saved data for `{account}`.\nQueue removed: {queue_removed}\nBid data cleared: {bid_data_cleared}"
        ),
        RuntimeOutcome::Queued { changed, .. } => {
            if *changed {
                "Queued action.".to_string()
            } else {
                "Action was already queued.".to_string()
            }
        }
        RuntimeOutcome::Executed { .. } => "Command executed.".to_string(),
        RuntimeOutcome::Planned { directive } => format_planned_directive(directive),
        RuntimeOutcome::AccountScheduled { result } => format!(
            "Scheduled {} for `{}` in {}ms.",
            scheduled_action_label(&result.action),
            result.account,
            result.delay_ms
        ),
        RuntimeOutcome::BlacklistApplied { account, result } => {
            if matches!(result.request, saf_core::BlacklistRequest::List) {
                format_blacklist_snapshot(&result.summary).unwrap_or_else(|| {
                    format!("Blacklist rules for `{account}`\n{}", result.summary)
                })
            } else if result.changed {
                format!("Blacklist updated for `{account}`.\n{}", result.summary)
            } else {
                format!("Blacklist unchanged for `{account}`.\n{}", result.summary)
            }
        }
        RuntimeOutcome::FlipProcessed { account, outcome } => {
            format!(
                "Processed flip for `{account}`.\n{}",
                format_flip_outcome(outcome)
            )
        }
    };
    truncate_discord(&content)
}

fn format_account_stats(account: &AccountId, stats: &saf_core::ports::AccountStats) -> String {
    let mut lines = vec![
        format!("Stats `{account}`"),
        format!("Profit: {}", format_coins(stats.total_profit)),
        format!("Bought: {}", stats.bought),
        format!("Sold: {}", stats.sold),
        format!("Auctions: {}", format_auction_slots(Some(stats))),
        format!(
            "Purse: {}",
            stats
                .purse
                .map(format_coins)
                .unwrap_or_else(|| "unknown".to_string())
        ),
        format!("Cofl ping: {}", format_ms(stats.cofl_ping_ms)),
        format!("Cofl delay: {}", format_ms(stats.cofl_delay_ms)),
        format!("Hypixel ping: {}", format_ms(stats.hypixel_ping_ms)),
    ];
    if let Some(tier) = &stats.cofl_tier {
        lines.push(format!("Cofl tier: {tier}"));
    }
    if let Some(expires_at) = stats.cofl_expires_at {
        lines.push(format!("Cofl expires: <t:{expires_at}:R>"));
    }
    lines.join("\n")
}

fn format_queue_snapshot(account: &AccountId, queue: &[QueueEntry]) -> String {
    if queue.is_empty() {
        return format!("Queue `{account}`\nNo queued actions.");
    }
    let mut lines = vec![format!(
        "Queue `{account}`\nQueued actions: {}",
        queue.len()
    )];
    lines.extend(queue.iter().take(12).map(|entry| {
        format!(
            "- {} priority {} {}",
            entry.state.as_str(),
            entry.priority,
            compact_json(&entry.action)
        )
    }));
    lines.join("\n")
}

pub(in crate::live_runtime::discord_gateway) fn scheduled_action_label(
    action: &ScheduledAccountAction,
) -> &'static str {
    match action {
        ScheduledAccountAction::Start => "start",
        ScheduledAccountAction::Stop => "stop",
    }
}

fn format_flip_outcome(outcome: &FlipOutcome) -> String {
    match outcome {
        FlipOutcome::IgnoredInvalid { item_name, reason } => {
            format!("Ignored {item_name}: invalid ({reason}).")
        }
        FlipOutcome::IgnoredBlocked { item_name, reason } => {
            format!("Ignored {item_name}: blocked ({reason}).")
        }
        FlipOutcome::IgnoredSkipped {
            item_name,
            starting_bid,
            ..
        } => format!("Skipped {item_name} at {}.", format_coins(*starting_bid)),
        FlipOutcome::OpenedAuction {
            item_name,
            starting_bid,
            ..
        } => format!(
            "Opened auction for {item_name} at {}.",
            format_coins(*starting_bid)
        ),
    }
}

/// Pick the card colour/icon for an outcome and reuse the prose render as the
/// description so the embed mirrors the text exactly. Account-scoped cards get
/// the account player head as the thumbnail.
pub(in crate::live_runtime::discord_gateway) fn outcome_embed(
    outcome: &RuntimeOutcome,
) -> CreateEmbed {
    let description = format_runtime_outcome(outcome);
    let (kind, title, account) = outcome_card_meta(outcome);
    let mut card = EmbedCard::for_kind(kind, title, description);
    if let Some(account) = account {
        card = card.thumbnail(crate::player_head::account_head_thumbnail_url(
            account.as_str(),
        ));
    }
    card.build()
}

fn outcome_card_meta(
    outcome: &RuntimeOutcome,
) -> (NotificationKind, &'static str, Option<&AccountId>) {
    match outcome {
        RuntimeOutcome::UsersSnapshot { .. } => (NotificationKind::Info, "Users", None),
        RuntimeOutcome::GlobalStatsSnapshot { .. } => {
            (NotificationKind::Info, "Global Stats", None)
        }
        RuntimeOutcome::StatsSnapshot { account, .. } => {
            (NotificationKind::Info, "Stats", Some(account))
        }
        RuntimeOutcome::ProfitSnapshot { account, .. } => {
            (NotificationKind::Info, "Profit", Some(account))
        }
        RuntimeOutcome::PingSnapshot { account, .. } => {
            (NotificationKind::Info, "Ping", Some(account))
        }
        RuntimeOutcome::QueueSnapshot { account, .. } => {
            (NotificationKind::Info, "Queue", Some(account))
        }
        RuntimeOutcome::ConnectionsSnapshot { .. } => (NotificationKind::Info, "Connections", None),
        RuntimeOutcome::LogSnapshot { .. } => (NotificationKind::Info, "Logs", None),
        RuntimeOutcome::InventorySnapshot { snapshot } => {
            (NotificationKind::Info, "Inventory", Some(&snapshot.account))
        }
        RuntimeOutcome::InventoryListingsQueued { account, .. } => {
            (NotificationKind::Listed, "Listings queued", Some(account))
        }
        RuntimeOutcome::DelistAllQueued { account, .. } => {
            (NotificationKind::Started, "Delist queued", Some(account))
        }
        RuntimeOutcome::GuiSlotDiagnostics { diagnostics } => (
            NotificationKind::Info,
            "GUI Slots",
            Some(&diagnostics.account),
        ),
        RuntimeOutcome::QueueCleared { account, .. } => {
            (NotificationKind::Started, "Queue cleared", Some(account))
        }
        RuntimeOutcome::QueueEntryCancelled { account, .. } => {
            (NotificationKind::Started, "Queue entry cancelled", Some(account))
        }
        RuntimeOutcome::QueuesCleared { .. } => (NotificationKind::Started, "Queues cleared", None),
        RuntimeOutcome::SavedDataCleared { account, .. } => (
            NotificationKind::Started,
            "Saved data cleared",
            Some(account),
        ),
        RuntimeOutcome::Queued { .. } => (NotificationKind::Started, "Queued", None),
        RuntimeOutcome::Executed { .. } => (NotificationKind::Started, "Executed", None),
        RuntimeOutcome::Planned { .. } => (NotificationKind::Info, "Planned", None),
        RuntimeOutcome::AccountScheduled { result } => {
            let kind = match &result.action {
                ScheduledAccountAction::Start => NotificationKind::Started,
                ScheduledAccountAction::Stop => NotificationKind::Stopped,
            };
            (kind, "Scheduled", Some(&result.account))
        }
        RuntimeOutcome::BlacklistApplied { account, .. } => {
            (NotificationKind::Info, "Blacklist", Some(account))
        }
        RuntimeOutcome::FlipProcessed { account, .. } => {
            (NotificationKind::FlipFound, "Flip processed", Some(account))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saf_core::ports::AccountScheduleResult;

    #[test]
    fn account_scheduled_renders_human_readable_text() {
        let outcome = RuntimeOutcome::AccountScheduled {
            result: AccountScheduleResult {
                account: AccountId::new("Main").unwrap(),
                action: ScheduledAccountAction::Stop,
                delay_ms: 250,
            },
        };

        let rendered = format_runtime_outcome(&outcome);

        assert!(rendered.contains("Scheduled stop for `Main` in 250ms."));
        assert!(!rendered.contains("Stop"));
        assert!(!rendered.contains('{'));
    }

    #[test]
    fn flip_processed_renders_human_readable_text() {
        let outcome = RuntimeOutcome::FlipProcessed {
            account: AccountId::new("Main").unwrap(),
            outcome: FlipOutcome::IgnoredBlocked {
                item_name: "Hyperion".to_string(),
                reason: "do-not-buy".to_string(),
            },
        };

        let rendered = format_runtime_outcome(&outcome);

        assert!(rendered.contains("Processed flip for `Main`."));
        assert!(rendered.contains("Ignored Hyperion: blocked (do-not-buy)."));
        assert!(!rendered.contains("IgnoredBlocked"));
        assert!(!rendered.contains("item_name"));
    }
}
