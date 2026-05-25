use super::super::super::formatting::format_auction_slot_values;
use saf_core::ports::AccountStats;
use serde_json::Value;

pub(in crate::live_runtime::discord_gateway) fn format_auction_slots(
    stats: Option<&AccountStats>,
) -> String {
    stats
        .map(|stats| format_auction_slot_values(stats.auction_slots_used, stats.auction_slots_max))
        .unwrap_or_else(|| "unknown".to_string())
}

pub(in crate::live_runtime::discord_gateway) fn list_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

pub(in crate::live_runtime::discord_gateway) fn truncate_discord(content: &str) -> String {
    const LIMIT: usize = 1900;
    if content.chars().count() <= LIMIT {
        return content.to_string();
    }
    content.chars().take(LIMIT - 14).collect::<String>() + "\n...truncated"
}

pub(super) fn format_ms(value: Option<u64>) -> String {
    value
        .map(|value| format!("{value}ms"))
        .unwrap_or_else(|| "unknown".to_string())
}

pub(super) fn compact_json(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string())
}

pub(super) fn sanitize_inline(value: &str) -> String {
    value.replace('`', "'")
}
