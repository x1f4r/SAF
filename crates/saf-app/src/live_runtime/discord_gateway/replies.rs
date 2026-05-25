use super::controls::{
    account_action_controls, blacklist_controls, dashboard_return_controls, inventory_controls,
    log_controls, queue_controls,
};
use super::rendering::{format_runtime_outcome, truncate_discord};
use saf_core::ports::PortError;
use saf_core::{AccountId, RuntimeDirective, RuntimeOutcome};
use std::path::Path;

#[derive(Clone, Debug)]
pub(in crate::live_runtime) struct DiscordInteractionReply {
    pub(in crate::live_runtime) content: String,
    pub(in crate::live_runtime) components: Vec<serenity::builder::CreateActionRow>,
    pub(in crate::live_runtime) files: Vec<serenity::builder::CreateAttachment>,
}

impl DiscordInteractionReply {
    pub(in crate::live_runtime::discord_gateway) fn content(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            components: Vec::new(),
            files: Vec::new(),
        }
    }

    pub(in crate::live_runtime::discord_gateway) fn with_components(
        content: impl Into<String>,
        components: Vec<serenity::builder::CreateActionRow>,
    ) -> Self {
        Self {
            content: content.into(),
            components,
            files: Vec::new(),
        }
    }

    fn add_file(&mut self, file: serenity::builder::CreateAttachment) {
        self.files.push(file);
    }

    fn set_components(&mut self, components: Vec<serenity::builder::CreateActionRow>) {
        self.components = components;
    }

    pub(in crate::live_runtime::discord_gateway) fn into_edit_response(
        self,
    ) -> serenity::builder::EditInteractionResponse {
        let Self {
            content,
            components,
            files,
        } = self;
        let mut message =
            serenity::builder::EditInteractionResponse::new().content(truncate_discord(&content));
        if !components.is_empty() {
            message = message.components(components);
        }
        for file in files {
            message = message.new_attachment(file);
        }
        message
    }
}

pub(super) fn deferred_interaction_response(
    ephemeral: bool,
) -> serenity::builder::CreateInteractionResponse {
    let message = serenity::builder::CreateInteractionResponseMessage::new().ephemeral(ephemeral);
    serenity::builder::CreateInteractionResponse::Defer(message)
}

pub(super) async fn discord_reply_for_outcome(outcome: &RuntimeOutcome) -> DiscordInteractionReply {
    let mut reply = DiscordInteractionReply::content(format_runtime_outcome(outcome));
    match outcome {
        RuntimeOutcome::LogSnapshot { snapshot } if snapshot.exists => {
            if let Some(file) = log_attachment(snapshot).await {
                reply.add_file(file);
            }
            reply.set_components(log_controls());
        }
        RuntimeOutcome::LogSnapshot { .. } => {
            reply.set_components(log_controls());
        }
        RuntimeOutcome::InventorySnapshot { snapshot } => {
            if let Ok(data) = serde_json::to_vec_pretty(snapshot) {
                reply.add_file(serenity::builder::CreateAttachment::bytes(
                    data,
                    format!("inventory-{}.json", snapshot.account),
                ));
            }
            reply.set_components(inventory_controls(&snapshot.account));
        }
        RuntimeOutcome::QueueSnapshot { account, .. } => {
            reply.set_components(queue_controls(account));
        }
        RuntimeOutcome::BlacklistApplied { result, .. }
            if matches!(result.request, saf_core::BlacklistRequest::List) =>
        {
            reply.set_components(blacklist_controls());
        }
        RuntimeOutcome::BlacklistApplied {
            account, result, ..
        } if !matches!(result.request, saf_core::BlacklistRequest::List) => {
            reply.set_components(account_action_controls(account));
        }
        RuntimeOutcome::Queued { directive, .. } => {
            if let Some(account) = directive_account(directive) {
                reply.set_components(account_action_controls(account));
            }
        }
        RuntimeOutcome::QueueCleared { account, .. }
        | RuntimeOutcome::SavedDataCleared { account, .. }
        | RuntimeOutcome::StatsSnapshot { account, .. }
        | RuntimeOutcome::ProfitSnapshot { account, .. }
        | RuntimeOutcome::PingSnapshot { account, .. }
        | RuntimeOutcome::InventoryListingsQueued { account, .. }
        | RuntimeOutcome::DelistAllQueued { account, .. } => {
            reply.set_components(account_action_controls(account));
        }
        RuntimeOutcome::GuiSlotDiagnostics { diagnostics } => {
            reply.set_components(account_action_controls(&diagnostics.account));
        }
        RuntimeOutcome::QueuesCleared { .. } => {
            reply.set_components(dashboard_return_controls());
        }
        _ => {}
    }
    reply
}

pub(super) fn format_runtime_error(error: &saf_core::RuntimeError) -> String {
    match error {
        saf_core::RuntimeError::PortError(PortError::Unavailable(message))
            if is_dry_run_active_auction_cache_error(message) =>
        {
            "Delist all\nDry-run mode needs a cached Manage Auctions window before Rust can queue delist actions. Rust did not open `/ah` or click auction menus. Connect the account with the Rust Minecraft runtime and cache that window first, or rerun with `--market-actions live` only when real auction navigation is intended.".to_string()
        }
        saf_core::RuntimeError::PortError(PortError::Unavailable(message)) => {
            format!("Runtime unavailable: {message}")
        }
        _ => format!("Runtime error: {error}"),
    }
}

fn is_dry_run_active_auction_cache_error(message: &str) -> bool {
    message.contains("dry-run active auction scan")
        && message.contains("cached Manage Auctions window")
}

async fn log_attachment(
    snapshot: &saf_core::ports::LogSnapshot,
) -> Option<serenity::builder::CreateAttachment> {
    let filename = Path::new(&snapshot.path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or("latest.log")
        .to_string();
    match tokio::fs::read(&snapshot.path).await {
        Ok(data) if !data.is_empty() => {
            Some(serenity::builder::CreateAttachment::bytes(data, filename))
        }
        _ if !snapshot.lines.is_empty() => Some(serenity::builder::CreateAttachment::bytes(
            snapshot.lines.join("\n").into_bytes(),
            filename,
        )),
        _ => None,
    }
}

fn directive_account(directive: &RuntimeDirective) -> Option<&AccountId> {
    match directive {
        RuntimeDirective::TransferCoins { from, .. }
        | RuntimeDirective::SendMinecraftChat { account: from, .. }
        | RuntimeDirective::SendCoflCommand { account: from, .. }
        | RuntimeDirective::ShowStats { account: from }
        | RuntimeDirective::ShowProfit { account: from }
        | RuntimeDirective::ShowPing { account: from }
        | RuntimeDirective::ShowQueue { account: from }
        | RuntimeDirective::ClearQueue { account: from }
        | RuntimeDirective::ClearData { account: from }
        | RuntimeDirective::BlacklistCommand { account: from, .. }
        | RuntimeDirective::CheckBids { account: from }
        | RuntimeDirective::Bank { account: from, .. }
        | RuntimeDirective::QueueState { account: from, .. }
        | RuntimeDirective::ExternalBuy { account: from, .. }
        | RuntimeDirective::TrackedListFlip { account: from, .. }
        | RuntimeDirective::ShowInventory { account: from }
        | RuntimeDirective::SellInventory { account: from, .. }
        | RuntimeDirective::QueueDelistAll { account: from }
        | RuntimeDirective::DiagnoseSlots { account: from, .. }
        | RuntimeDirective::TestWebhook { account: from }
        | RuntimeDirective::ScheduleAccount { account: from, .. }
        | RuntimeDirective::UnknownTerminalCommand { account: from, .. } => Some(from),
        RuntimeDirective::StartAccounts { .. }
        | RuntimeDirective::StopAccounts { account: None }
        | RuntimeDirective::ShowUsers
        | RuntimeDirective::ShowGlobalStats
        | RuntimeDirective::ShowConnections
        | RuntimeDirective::ClearAllQueues
        | RuntimeDirective::ShowLogs { .. } => None,
        RuntimeDirective::StopAccounts {
            account: Some(account),
        } => Some(account),
    }
}
