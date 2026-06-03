use super::helpers::sanitize_inline;
use saf_core::RuntimeDirective;

pub(in crate::live_runtime) fn format_planned_directive(directive: &RuntimeDirective) -> String {
    match directive {
        RuntimeDirective::ShowInventory { account } => format!(
            "Inventory `{account}`\nNo live inventory provider is available. Enable the `production-runtime` feature bundle, set `SAF_RUST_MINECRAFT=azalea`, and complete Microsoft auth for this account before using live inventory snapshots."
        ),
        RuntimeDirective::SellInventory {
            account,
            include_hotbar,
        } => format!(
            "Inventory listing `{account}`\nNo live inventory provider is available, so Rust cannot preview or queue inventory listings{} yet. Enable `production-runtime`, set `SAF_RUST_MINECRAFT=azalea`, and complete Microsoft auth for this account.",
            if *include_hotbar {
                " with hotbar items"
            } else {
                ""
            }
        ),
        RuntimeDirective::QueueDelistAll { account } => format!(
            "Delist all `{account}`\nNo live active-auction provider is available. Start the Rust live runtime with Minecraft connected and open enough auction state for Rust to scan active auctions."
        ),
        RuntimeDirective::DiagnoseSlots { account, .. } => format!(
            "GUI diagnostics `{account}`\nNo live GUI snapshot provider is available. Connect the account with the Rust Minecraft runtime first."
        ),
        RuntimeDirective::SendCoflCommand { account, .. } => format!(
            "Cofl `{account}`\nNo live Cofl websocket is registered for this account. Configure a Cofl session/socket and run with the `live-cofl` or `production-runtime` feature."
        ),
        RuntimeDirective::SendMinecraftChat { account, .. } => format!(
            "Minecraft `{account}`\nNo live Minecraft client is registered for this account. Start the Rust live runtime with `production-runtime` and connect the account before sending chat commands."
        ),
        RuntimeDirective::TrackedListFlip {
            account,
            auction_id,
            ..
        } => format!(
            "List flip `{account}`\nNo tracked flip metadata provider found for `{auction_id}`. Rust needs a live Cofl flip cache or existing SavedData bidData before listing without a manual price."
        ),
        RuntimeDirective::ExternalBuy {
            account,
            auction_id,
        } => format!(
            "Buy flip `{account}`\nNo live queue provider accepted auction `{auction_id}`. Start the Rust live runtime with queue state available before queueing external buys."
        ),
        RuntimeDirective::QueueState { account, state, .. } => format!(
            "Queue `{account}`\nNo live queue provider accepted state `{}`. Start the Rust live runtime with SavedData queue storage available.",
            state.as_str()
        ),
        RuntimeDirective::CheckBids { account } => format!(
            "Bids `{account}`\nNo live queue provider is available to queue bid collection."
        ),
        RuntimeDirective::Bank { account, .. } => format!(
            "Bank `{account}`\nNo live queue provider is available to queue the bank action."
        ),
        RuntimeDirective::TransferCoins { from, to, .. } => format!(
            "Bank transfer `{from}` to `{to}`\nNo live queue and account-supervision path is available for the transfer workflow."
        ),
        RuntimeDirective::StartAccounts { .. }
        | RuntimeDirective::StopAccounts { .. }
        | RuntimeDirective::ScheduleAccount { .. } => {
            "Account control\nNo live account supervisor is registered for this runtime."
                .to_string()
        }
        RuntimeDirective::ShowStats { account }
        | RuntimeDirective::ShowProfit { account }
        | RuntimeDirective::ShowPing { account } => {
            format!("Stats `{account}`\nNo live stats provider is registered for this account.")
        }
        RuntimeDirective::ShowConnections => {
            "Connections\nNo live connection provider is registered.".to_string()
        }
        RuntimeDirective::ShowUsers => {
            "Users\nNo runtime account registry is available.".to_string()
        }
        RuntimeDirective::ShowGlobalStats => {
            "Global stats\nNo live stats provider is registered for the running accounts."
                .to_string()
        }
        RuntimeDirective::ShowLogs { .. } => {
            "Logs\nNo live log reader is registered for this runtime.".to_string()
        }
        RuntimeDirective::ShowQueue { account } => format!(
            "Queue `{account}`\nNo live queue provider is registered for this account."
        ),
        RuntimeDirective::ClearQueue { account } => format!(
            "Queue `{account}`\nNo live queue provider is registered, so Rust cannot clear this queue."
        ),
        RuntimeDirective::CancelQueueEntry { account, index } => format!(
            "Queue `{account}`\nNo live queue provider is registered, so Rust cannot cancel queue entry {index}."
        ),
        RuntimeDirective::ClearAllQueues => {
            "Queues\nNo live queue provider is registered, so Rust cannot clear configured account queues."
                .to_string()
        }
        RuntimeDirective::ClearData { account } => format!(
            "Saved data `{account}`\nNo live saved-data provider is registered for this account."
        ),
        RuntimeDirective::BlacklistCommand { account, .. } => format!(
            "Blacklist `{account}`\nNo live blacklist store is registered for this runtime."
        ),
        RuntimeDirective::TestWebhook { account } => {
            format!("Webhook `{account}`\nNo notifier is configured for this runtime.")
        }
        RuntimeDirective::Cookie { account } => format!(
            "Cookie `{account}`\nNo live cookie machinery is registered for this runtime. Start the Rust live runtime with `production-runtime` and the account connected."
        ),
        RuntimeDirective::UnknownTerminalCommand {
            command, message, ..
        } => {
            if message.trim().is_empty() {
                format!("Unknown command `{}`.", sanitize_inline(command))
            } else {
                format!(
                    "Unknown command `{} {}`.",
                    sanitize_inline(command),
                    sanitize_inline(message)
                )
            }
        }
    }
}
