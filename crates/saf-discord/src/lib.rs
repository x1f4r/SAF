use saf_core::ids::AccountId;
use serde::{Deserialize, Serialize};

mod commands;
mod planning;
mod webhook;

pub use commands::{
    CommandDefinition, CommandOption, CommandOptionChoice, CommandOptionKind, command_definitions,
    validate_command_definitions,
};
pub use planning::{
    CommandInvocation, CommandOptionValue, DiscordCommandPlan, DiscordCommandPlanError,
    DiscordControllerAction, plan_button, plan_invocation,
};
pub use webhook::{
    DiscordEmbed, DiscordEmbedFooter, DiscordWebhookIdentity, DiscordWebhookPayload,
    notification_payload,
};

pub enum DiscordCommand {
    Dashboard,
    StartBot {
        username: Option<String>,
    },
    StopBot {
        username: Option<String>,
    },
    SendCommand {
        username: Option<String>,
        command: String,
    },
    Blacklist,
    Queue {
        username: Option<String>,
    },
    Reconcile {
        username: Option<String>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AccountPanel {
    pub account: AccountId,
    pub state: String,
    pub queue_size: usize,
    pub purse: Option<u64>,
    pub current_auctions: Option<u32>,
    pub max_auctions: Option<u32>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DashboardSummary {
    pub accounts: Vec<AccountPanel>,
    pub external_backend_enabled: bool,
    pub paused: bool,
}

impl DashboardSummary {
    pub fn button_rows_needed(&self) -> usize {
        let global_rows = 2;
        let account_rows = self.accounts.len().min(3);
        global_rows + account_rows
    }
}

#[cfg(feature = "webhook-notifier")]
pub mod webhook_notifier {
    use super::{DiscordWebhookIdentity, notification_payload};
    use async_trait::async_trait;
    use saf_core::ports::{Notification, Notifier, PortError};

    pub struct DiscordWebhookNotifier {
        client: reqwest::Client,
        urls: Vec<String>,
        identity: DiscordWebhookIdentity,
    }

    impl DiscordWebhookNotifier {
        pub fn new(urls: Vec<String>, identity: DiscordWebhookIdentity) -> Self {
            Self {
                client: reqwest::Client::new(),
                urls,
                identity,
            }
        }

        pub fn urls(&self) -> &[String] {
            &self.urls
        }
    }

    #[async_trait]
    impl Notifier for DiscordWebhookNotifier {
        async fn notify(&self, notification: Notification) -> Result<(), PortError> {
            if self.urls.is_empty() {
                return Ok(());
            }

            let payload = notification_payload(&notification, &self.identity);
            for url in &self.urls {
                self.client
                    .post(url)
                    .json(&payload)
                    .send()
                    .await
                    .map_err(|error| PortError::Failed(error.to_string()))?
                    .error_for_status()
                    .map_err(|error| PortError::Failed(error.to_string()))?;
            }
            Ok(())
        }
    }
}

#[cfg(feature = "serenity-controller")]
pub mod serenity_controller {
    use super::{CommandDefinition, CommandOption, CommandOptionKind};
    use serenity::all::CommandOptionType;
    use serenity::builder::{CreateCommand, CreateCommandOption};

    pub fn to_serenity_commands(commands: &[CommandDefinition]) -> Vec<CreateCommand> {
        commands
            .iter()
            .map(|command| {
                command.options.iter().fold(
                    CreateCommand::new(command.name).description(command.description),
                    |builder, option| {
                        let command_option = CreateCommandOption::new(
                            option_kind(option.kind),
                            option.name,
                            option.description,
                        )
                        .required(option.required);
                        builder.add_option(add_option_choices(command_option, option))
                    },
                )
            })
            .collect()
    }

    fn add_option_choices(
        builder: CreateCommandOption,
        option: &CommandOption,
    ) -> CreateCommandOption {
        match option.kind {
            CommandOptionKind::String => option.choices.iter().fold(builder, |builder, choice| {
                builder.add_string_choice(choice.name, choice.value)
            }),
            CommandOptionKind::Integer | CommandOptionKind::Boolean => builder,
        }
    }

    fn option_kind(kind: CommandOptionKind) -> CommandOptionType {
        match kind {
            CommandOptionKind::String => CommandOptionType::String,
            CommandOptionKind::Integer => CommandOptionType::Integer,
            CommandOptionKind::Boolean => CommandOptionType::Boolean,
        }
    }
}

#[cfg(test)]
mod tests;
