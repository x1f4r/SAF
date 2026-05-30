use super::helpers::truncate_discord;
use saf_core::ports::NotificationKind;
use serenity::builder::CreateEmbed;

/// Discord caps an embed field value at 1024 characters and a description at
/// 4096. We keep titles short and reuse [`truncate_discord`] for the body so
/// the cards never exceed the API limits.
const FIELD_VALUE_LIMIT: usize = 1024;
const FIELD_NAME_LIMIT: usize = 256;
const TITLE_LIMIT: usize = 256;

/// Shared builder for the redesigned operator cards. Every gateway reply that
/// renders structured output funnels through here so colour, icon, and field
/// handling stay consistent with the webhook path.
pub(in crate::live_runtime::discord_gateway) struct EmbedCard {
    color: u32,
    title: String,
    description: String,
    fields: Vec<(String, String, bool)>,
    thumbnail: Option<String>,
}

impl EmbedCard {
    pub(in crate::live_runtime::discord_gateway) fn new(
        color: u32,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            color,
            title: title.into(),
            description: description.into(),
            fields: Vec::new(),
            thumbnail: None,
        }
    }

    /// Build a card whose colour and icon come from the shared notification
    /// palette. The icon is prefixed to the title so the event type reads at a
    /// glance, matching the webhook embeds.
    pub(in crate::live_runtime::discord_gateway) fn for_kind(
        kind: NotificationKind,
        title: impl Into<String>,
        description: impl Into<String>,
    ) -> Self {
        let title = format!("{} {}", saf_discord::kind_icon(kind), title.into());
        Self::new(saf_discord::kind_color(kind), title, description)
    }

    pub(in crate::live_runtime::discord_gateway) fn field(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
        inline: bool,
    ) -> Self {
        self.fields.push((name.into(), value.into(), inline));
        self
    }

    pub(in crate::live_runtime::discord_gateway) fn thumbnail(
        mut self,
        url: Option<String>,
    ) -> Self {
        self.thumbnail = url.filter(|value| !value.trim().is_empty());
        self
    }

    pub(in crate::live_runtime::discord_gateway) fn build(self) -> CreateEmbed {
        let mut embed = CreateEmbed::new()
            .color(self.color)
            .title(truncate_chars(&self.title, TITLE_LIMIT));
        let description = truncate_discord(&self.description);
        if !description.trim().is_empty() {
            embed = embed.description(description);
        }
        if let Some(url) = self.thumbnail {
            embed = embed.thumbnail(url);
        }
        for (name, value, inline) in self.fields.into_iter().take(25) {
            embed = embed.field(
                truncate_chars(&name, FIELD_NAME_LIMIT),
                truncate_chars(&value, FIELD_VALUE_LIMIT),
                inline,
            );
        }
        embed
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
