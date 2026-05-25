use super::env_truthy;
use crate::player_head::PlayerHeadRefresher;
use anyhow::{Context, Result};
use async_trait::async_trait;
use saf_core::{RuntimeSession, SafConfig};
use saf_discord::{DiscordCommandPlan, DiscordControllerAction};
use std::collections::BTreeSet;
use std::sync::Arc;

#[cfg(feature = "live-discord")]
mod confirmations;
#[cfg(feature = "live-discord")]
mod controls;
#[cfg(feature = "live-discord")]
mod dashboard;
#[cfg(feature = "live-discord")]
mod heads;
#[cfg(feature = "live-discord")]
mod rendering;
#[cfg(feature = "live-discord")]
mod replies;
#[cfg(all(test, feature = "live-discord"))]
pub(super) use confirmations::format_inventory_listing_preview;
#[cfg(feature = "live-discord")]
use confirmations::{
    confirm_delist_everything_reply, confirm_sell_inventory_reply, confirm_status_action_reply,
};
#[cfg(feature = "live-discord")]
use dashboard::{account_panel_reply, dashboard_reply, help_reply};
#[cfg(feature = "live-discord")]
use heads::refresh_heads_reply;
#[cfg(all(test, feature = "live-discord"))]
pub(super) use rendering::format_planned_directive;
#[cfg(feature = "live-discord")]
use replies::{
    DiscordInteractionReply, deferred_interaction_response, discord_reply_for_outcome,
    format_runtime_error,
};

#[cfg(feature = "live-discord")]
#[derive(Clone, Debug)]
pub(super) struct DiscordGatewayConfig {
    pub(super) token: String,
    pub(super) guild_id: Option<u64>,
    pub(super) ephemeral: bool,
    pub(super) allowed_ids: BTreeSet<String>,
}

#[cfg(feature = "live-discord")]
impl DiscordGatewayConfig {
    pub(super) fn from_config(config: &SafConfig) -> Option<Self> {
        let token = std::env::var("SAF_DISCORD_TOKEN")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| config.discord_bot.token.clone())
            .trim()
            .to_string();
        let enabled = std::env::var("SAF_DISCORD_BOT_ENABLED")
            .ok()
            .is_some_and(|value| env_truthy(&value))
            || config.discord_bot.enabled
            || !token.trim().is_empty();
        if !enabled || token.trim().is_empty() {
            return None;
        }

        let guild_id = std::env::var("SAF_DISCORD_GUILD_ID")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                (!config.discord_bot.guild_id.trim().is_empty())
                    .then(|| config.discord_bot.guild_id.clone())
            })
            .and_then(|value| value.trim().parse::<u64>().ok());
        let mut allowed_ids = BTreeSet::new();
        allowed_ids.extend(
            config
                .allowed_i_ds
                .iter()
                .filter_map(|id| normalize_discord_user_id(id)),
        );
        allowed_ids.extend(
            config
                .discord_bot
                .allowed_i_ds
                .iter()
                .filter_map(|id| normalize_discord_user_id(id)),
        );
        if let Some(discord_id) = normalize_discord_user_id(&config.discord_id) {
            allowed_ids.insert(discord_id);
        }
        if let Ok(env_ids) = std::env::var("SAF_DISCORD_ALLOWED_IDS") {
            allowed_ids.extend(env_ids.split(',').filter_map(normalize_discord_user_id));
        }

        Some(Self {
            token,
            guild_id,
            ephemeral: config.discord_bot.ephemeral,
            allowed_ids,
        })
    }

    pub(super) fn is_allowed(&self, user_id: u64) -> bool {
        self.allowed_ids.contains(&user_id.to_string())
    }
}

#[cfg(feature = "live-discord")]
fn normalize_discord_user_id(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    trimmed.parse::<u64>().ok().map(|id| id.to_string())
}

#[cfg(feature = "live-discord")]
pub(super) async fn start_discord_gateway(
    session: Arc<RuntimeSession>,
    config: &SafConfig,
    head_refresher: Arc<PlayerHeadRefresher>,
) -> Result<Option<tokio::task::JoinHandle<()>>> {
    let Some(discord) = DiscordGatewayConfig::from_config(config) else {
        return Ok(None);
    };
    let token = discord.token.clone();
    let handler = DiscordGatewayHandler {
        session,
        discord,
        head_refresher,
    };
    let mut client = serenity::Client::builder(token, serenity::all::GatewayIntents::GUILDS)
        .event_handler(handler)
        .await
        .context("starting Discord gateway client")?;
    Ok(Some(tokio::spawn(async move {
        if let Err(error) = client.start().await {
            tracing::error!(error = %error, "Discord gateway stopped");
        }
    })))
}

#[cfg(feature = "live-discord")]
struct DiscordGatewayHandler {
    session: Arc<RuntimeSession>,
    discord: DiscordGatewayConfig,
    head_refresher: Arc<PlayerHeadRefresher>,
}

#[cfg(feature = "live-discord")]
#[async_trait]
impl serenity::prelude::EventHandler for DiscordGatewayHandler {
    async fn ready(&self, ctx: serenity::prelude::Context, _data_about_bot: serenity::all::Ready) {
        let commands = saf_discord::serenity_controller::to_serenity_commands(
            &saf_discord::command_definitions(),
        );
        let result = if let Some(guild_id) = self.discord.guild_id {
            serenity::all::GuildId::new(guild_id)
                .set_commands(&ctx.http, commands)
                .await
                .map(|commands| commands.len())
        } else {
            serenity::all::Command::set_global_commands(&ctx.http, commands)
                .await
                .map(|commands| commands.len())
        };
        match result {
            Ok(count) => tracing::info!(count, "registered Discord application commands"),
            Err(error) => tracing::error!(error = %error, "failed to register Discord commands"),
        }
    }

    async fn interaction_create(
        &self,
        ctx: serenity::prelude::Context,
        interaction: serenity::all::Interaction,
    ) {
        match interaction {
            serenity::all::Interaction::Command(command) => {
                let user_id = command.user.id.get();
                let allowed = self.discord.is_allowed(user_id);
                tracing::info!(
                    user_id,
                    command = %command.data.name,
                    allowed,
                    "received Discord command interaction"
                );
                let ephemeral = self.discord.ephemeral || !allowed;
                if let Err(error) = command
                    .create_response(&ctx.http, deferred_interaction_response(ephemeral))
                    .await
                {
                    tracing::warn!(error = %error, "failed to defer Discord command interaction");
                    return;
                }
                let reply = self.handle_command(&command).await;
                if let Err(error) = command
                    .edit_response(&ctx.http, reply.into_edit_response())
                    .await
                {
                    tracing::warn!(error = %error, "failed to edit deferred Discord command response");
                }
            }
            serenity::all::Interaction::Component(component) => {
                let user_id = component.user.id.get();
                let allowed = self.discord.is_allowed(user_id);
                tracing::info!(
                    user_id,
                    custom_id = %component.data.custom_id,
                    allowed,
                    "received Discord component interaction"
                );
                let ephemeral = self.discord.ephemeral || !allowed;
                if let Err(error) = component
                    .create_response(&ctx.http, deferred_interaction_response(ephemeral))
                    .await
                {
                    tracing::warn!(error = %error, "failed to defer Discord component interaction");
                    return;
                }
                let reply = self.handle_component(&component).await;
                if let Err(error) = component
                    .edit_response(&ctx.http, reply.into_edit_response())
                    .await
                {
                    tracing::warn!(error = %error, "failed to edit deferred Discord component response");
                }
            }
            _ => {}
        }
    }
}

#[cfg(feature = "live-discord")]
impl DiscordGatewayHandler {
    async fn handle_command(
        &self,
        command: &serenity::all::CommandInteraction,
    ) -> DiscordInteractionReply {
        if !self.discord.is_allowed(command.user.id.get()) {
            return DiscordInteractionReply::content("Not authorized.");
        }
        let invocation = command_invocation(command);
        match saf_discord::plan_invocation(&invocation) {
            Ok(plan) => {
                execute_discord_plan(
                    &self.session,
                    plan,
                    Some(command.user.id.get()),
                    Some(self.head_refresher.as_ref()),
                )
                .await
            }
            Err(error) => DiscordInteractionReply::content(format!("Command error: {error}")),
        }
    }

    async fn handle_component(
        &self,
        component: &serenity::all::ComponentInteraction,
    ) -> DiscordInteractionReply {
        if !self.discord.is_allowed(component.user.id.get()) {
            return DiscordInteractionReply::content("Not authorized.");
        }
        match saf_discord::plan_button(&component.data.custom_id) {
            Ok(Some(plan)) => {
                execute_discord_plan(
                    &self.session,
                    plan,
                    Some(component.user.id.get()),
                    Some(self.head_refresher.as_ref()),
                )
                .await
            }
            Ok(None) => DiscordInteractionReply::content("Canceled."),
            Err(error) => DiscordInteractionReply::content(format!("Button error: {error}")),
        }
    }
}

#[cfg(feature = "live-discord")]
fn command_invocation(
    command: &serenity::all::CommandInteraction,
) -> saf_discord::CommandInvocation {
    let mut invocation = saf_discord::CommandInvocation::new(command.data.name.clone());
    for option in &command.data.options {
        if let Some(value) = command_option_value(&option.value) {
            invocation.options.insert(option.name.clone(), value);
        }
    }
    invocation
}

#[cfg(feature = "live-discord")]
fn command_option_value(
    value: &serenity::all::CommandDataOptionValue,
) -> Option<saf_discord::CommandOptionValue> {
    match value {
        serenity::all::CommandDataOptionValue::String(value) => {
            Some(saf_discord::CommandOptionValue::String(value.clone()))
        }
        serenity::all::CommandDataOptionValue::Integer(value) => {
            Some(saf_discord::CommandOptionValue::Integer(*value))
        }
        serenity::all::CommandDataOptionValue::Boolean(value) => {
            Some(saf_discord::CommandOptionValue::Boolean(*value))
        }
        serenity::all::CommandDataOptionValue::Number(value) => {
            Some(saf_discord::CommandOptionValue::String(value.to_string()))
        }
        _ => None,
    }
}

#[cfg(feature = "live-discord")]
pub(super) async fn execute_discord_plan(
    session: &RuntimeSession,
    plan: DiscordCommandPlan,
    user_id: Option<u64>,
    head_refresher: Option<&PlayerHeadRefresher>,
) -> DiscordInteractionReply {
    match plan {
        DiscordCommandPlan::LocalCommand { command } => {
            execute_discord_command(session, command).await
        }
        DiscordCommandPlan::ControllerAction { action } => match action {
            DiscordControllerAction::Dashboard => dashboard_reply(session, "Dashboard").await,
            DiscordControllerAction::Status => dashboard_reply(session, "Status").await,
            DiscordControllerAction::Help => help_reply(),
            DiscordControllerAction::AccountPanel { username } => {
                account_panel_reply(session, username.as_str()).await
            }
            DiscordControllerAction::DiscordId => user_id
                .map(|id| format!("Discord ID: {id}"))
                .map(DiscordInteractionReply::content)
                .unwrap_or_else(|| DiscordInteractionReply::content("Discord ID unavailable.")),
            DiscordControllerAction::SellInventory {
                username,
                include_hotbar,
            } => confirm_sell_inventory_reply(session, username.as_deref(), include_hotbar).await,
            DiscordControllerAction::DelistEverything { username } => {
                confirm_delist_everything_reply(session, username.as_deref())
            }
            DiscordControllerAction::RefreshHeads { username } => {
                refresh_heads_reply(session, head_refresher, username.as_deref()).await
            }
            DiscordControllerAction::StatusForConfirmation { username, action } => {
                confirm_status_action_reply(session, username.as_str(), action.as_str())
            }
        },
    }
}

#[cfg(feature = "live-discord")]
#[allow(dead_code)]
pub(super) async fn execute_discord_terminal(
    session: &RuntimeSession,
    line: &str,
) -> DiscordInteractionReply {
    execute_discord_command(
        session,
        saf_core::LocalCommand::Terminal {
            line: line.to_string(),
            created_at: None,
        },
    )
    .await
}

#[cfg(feature = "live-discord")]
async fn execute_discord_command(
    session: &RuntimeSession,
    command: saf_core::LocalCommand,
) -> DiscordInteractionReply {
    match session.process_local_command(command).await {
        Ok(outcome) => discord_reply_for_outcome(&outcome).await,
        Err(error) => DiscordInteractionReply::content(format_runtime_error(&error)),
    }
}

#[cfg(feature = "live-discord")]
fn resolve_discord_account(session: &RuntimeSession, username: Option<&str>) -> Option<String> {
    session.selector().resolve_running(username)
}

#[cfg(feature = "live-discord")]
fn account_selection_error(session: &RuntimeSession, username: Option<&str>) -> String {
    if let Some(username) = username.filter(|value| !value.trim().is_empty()) {
        return format!("No running account matched `{}`.", username.trim());
    }
    let running = session.running_accounts();
    if running.is_empty() {
        "No accounts are currently running.".to_string()
    } else {
        format!(
            "Choose an account first. Running accounts: {}.",
            running.join(", ")
        )
    }
}
