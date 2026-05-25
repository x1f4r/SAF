use crate::live_runtime::support::{number_value, string_value};
use saf_core::{AccountId, BotState, QueueEntry};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::live_runtime) struct TransferFollowup {
    pub(in crate::live_runtime) target: AccountId,
    pub(in crate::live_runtime) amount: u64,
    pub(in crate::live_runtime) stop_source: bool,
}

pub(in crate::live_runtime) fn transfer_followup(entry: &QueueEntry) -> Option<TransferFollowup> {
    if !matches!(&entry.state, BotState::Custom(name) if name == "bank")
        || entry
            .action
            .get("withdraw")
            .and_then(serde_json::Value::as_bool)
            .is_none_or(|withdraw| withdraw)
    {
        return None;
    }
    let transfer = entry.action.get("transfer")?;
    let target = string_value(transfer, &["to"]).and_then(AccountId::new)?;
    let amount = number_value(&entry.action, &["amount"])?;
    if !amount.is_finite() || amount <= 0.0 {
        return None;
    }
    Some(TransferFollowup {
        target,
        amount: amount.floor() as u64,
        stop_source: transfer
            .get("stopSource")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
    })
}

pub(in crate::live_runtime) fn is_transfer_withdrawal_entry(
    entry: &QueueEntry,
    source: &AccountId,
    transfer: &TransferFollowup,
) -> bool {
    if !matches!(&entry.state, BotState::Custom(name) if name == "bank")
        || entry
            .action
            .get("withdraw")
            .and_then(serde_json::Value::as_bool)
            != Some(true)
        || entry
            .action
            .get("personal")
            .and_then(serde_json::Value::as_bool)
            != Some(false)
    {
        return false;
    }
    let amount_matches = number_value(&entry.action, &["amount"])
        .is_some_and(|amount| amount.floor() as u64 == transfer.amount);
    let Some(action_transfer) = entry.action.get("transfer") else {
        return false;
    };
    amount_matches
        && string_value(action_transfer, &["from"]).as_deref() == Some(source.as_str())
        && action_transfer
            .get("complete")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
}
