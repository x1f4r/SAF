use crate::palette::{kind_color, kind_icon};
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
    pub author: Option<DiscordEmbedAuthor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<DiscordEmbedThumbnail>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<DiscordEmbedField>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footer: Option<DiscordEmbedFooter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiscordEmbedAuthor {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiscordEmbedThumbnail {
    pub url: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiscordEmbedField {
    pub name: String,
    pub value: String,
    pub inline: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DiscordEmbedFooter {
    pub text: String,
}

pub fn notification_payload(
    notification: &Notification,
    identity: &DiscordWebhookIdentity,
) -> DiscordWebhookPayload {
    let author = identity.username.as_ref().map(|name| DiscordEmbedAuthor {
        name: truncate_chars(name, 256),
        icon_url: identity.avatar_url.clone(),
    });
    let thumbnail = notification
        .thumbnail_url
        .as_ref()
        .filter(|url| !url.trim().is_empty())
        .map(|url| DiscordEmbedThumbnail { url: url.clone() });
    let fields = notification
        .fields
        .iter()
        .take(25)
        .map(|(name, value)| DiscordEmbedField {
            name: truncate_chars(name, 256),
            value: truncate_chars(value, 1024),
            inline: true,
        })
        .collect();

    DiscordWebhookPayload {
        username: identity.username.clone(),
        avatar_url: identity.avatar_url.clone(),
        embeds: vec![DiscordEmbed {
            title: truncate_chars(&titled(notification), 256),
            description: truncate_chars(&notification.body, 4096),
            color: kind_color(notification.kind),
            author,
            thumbnail,
            fields,
            footer: notification
                .account
                .as_ref()
                .map(|account| DiscordEmbedFooter {
                    text: account.to_string(),
                }),
            timestamp: None,
        }],
    }
}

fn titled(notification: &Notification) -> String {
    format!("{} {}", kind_icon(notification.kind), notification.title)
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
