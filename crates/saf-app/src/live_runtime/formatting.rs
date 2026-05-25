use saf_core::ports::AccountStats;

pub(super) fn format_coins(value: f64) -> String {
    let abs = value.abs();
    if abs >= 1_000_000_000.0 {
        format!("{:.2}b", value / 1_000_000_000.0)
    } else if abs >= 1_000_000.0 {
        format!("{:.2}m", value / 1_000_000.0)
    } else if abs >= 1_000.0 {
        format!("{:.2}k", value / 1_000.0)
    } else {
        format!("{value:.0}")
    }
}

pub(super) fn format_auction_slot_values(used: Option<usize>, max: Option<usize>) -> String {
    match (used, max) {
        (Some(used), Some(max)) => format!("{used}/{max}"),
        (Some(used), None) => used.to_string(),
        (None, Some(max)) => format!("unknown/{max}"),
        (None, None) => "unknown".to_string(),
    }
}

pub(super) fn startup_ready_notification_body(
    stats: &AccountStats,
    connection_id: Option<&str>,
) -> String {
    let auction_slots =
        format_auction_slot_values(stats.auction_slots_used, stats.auction_slots_max);
    let purse = stats
        .purse
        .map(format_coins)
        .unwrap_or_else(|| "unknown".to_string());
    let cofl_tier = stats.cofl_tier.as_deref().unwrap_or("unknown");
    let cofl_expires = stats
        .cofl_expires_at
        .map(|expires_at| format!("<t:{expires_at}:R>"))
        .unwrap_or_else(|| "unknown".to_string());
    let cookie_expires = stats
        .cookie_expires_at
        .map(|expires_at| format!("<t:{expires_at}:R>"))
        .unwrap_or_else(|| "unknown".to_string());
    let connection_id = connection_id.unwrap_or("pending");
    format!(
        "Auction Slots: `{auction_slots}`\nPurse: `{purse}`\nCofl Tier: `{cofl_tier}`\nCofl expires: {cofl_expires}\nBooster Cookie ends: {cookie_expires}\nConnection ID: `{connection_id}`"
    )
}
