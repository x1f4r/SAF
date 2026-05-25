use super::super::super::formatting::format_coins;
use super::blacklist::format_blacklist_snapshot;
use super::diagnostics::format_gui_slot_diagnostics;
use super::helpers::{
    compact_json, format_auction_slots, format_ms, list_or_none, truncate_discord,
};
use super::planned::format_planned_directive;
use saf_core::{AccountId, QueueEntry, RuntimeOutcome};

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
            "Scheduled {:?} for `{}` in {}ms.",
            result.action, result.account, result.delay_ms
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
            format!("Processed flip for `{account}`.\n{outcome:?}")
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
