use saf_core::ports::Notification;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscordWebhookIdentity {
    pub username: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiscordWebhookPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_url: Option<String>,
    pub embeds: Vec<DiscordEmbed>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiscordEmbed {
    pub title: String,
    pub description: String,
    pub color: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<DiscordEmbedFooter>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiscordEmbedFooter {
    pub text: String,
}

pub fn notification_payload(
    notification: &Notification,
    identity: &DiscordWebhookIdentity,
) -> DiscordWebhookPayload {
    DiscordWebhookPayload {
        username: identity.username.clone(),
        avatar_url: identity.avatar_url.clone(),
        embeds: vec![DiscordEmbed {
            title: truncate_chars(&notification.title, 256),
            description: truncate_chars(&notification.body, 4096),
            color: 0x3498db,
            footer: notification
                .account
                .as_ref()
                .map(|account| DiscordEmbedFooter {
                    text: account.to_string(),
                }),
        }],
    }
}

fn truncate_chars(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        return value.to_string();
    }
    let suffix = "...";
    value
        .chars()
        .take(max.saturating_sub(suffix.len()))
        .collect::<String>()
        + suffix
}
