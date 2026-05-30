#[cfg(feature = "live-cofl")]
use super::support::strip_minecraft_color_codes;
use super::{formatting::format_coins, stats::PurchaseStatsUpdate};
use async_trait::async_trait;
#[cfg(feature = "live-cofl")]
use saf_core::ports::NotificationKind;
use saf_core::ports::{Notification, Notifier, PortError};
use saf_core::{AccountId, FlipEvent, RuntimeSession, SafConfig};
use std::sync::Arc;

#[derive(Clone)]
struct CompositeNotifier {
    notifiers: Vec<Arc<dyn Notifier>>,
}

#[async_trait]
impl Notifier for CompositeNotifier {
    async fn notify(&self, notification: Notification) -> Result<(), PortError> {
        for notifier in &self.notifiers {
            notifier.notify(notification.clone()).await?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
struct StdoutNotifier;

#[async_trait]
impl Notifier for StdoutNotifier {
    async fn notify(&self, notification: Notification) -> Result<(), PortError> {
        println!(
            "{}",
            serde_json::to_string(&notification).unwrap_or_default()
        );
        Ok(())
    }
}

pub(super) async fn notify_operator_best_effort(
    session: &RuntimeSession,
    notification: Notification,
) {
    let title = notification.title.clone();
    let account = notification.account.clone();
    if let Err(error) = session.notify(notification).await {
        match account {
            Some(account) => {
                tracing::warn!(account = %account, title = %title, error = %error, "operator notification failed");
            }
            None => {
                tracing::warn!(title = %title, error = %error, "operator notification failed");
            }
        }
    }
}

pub(super) fn default_notifier(config: &SafConfig) -> Arc<dyn Notifier> {
    let mut notifiers: Vec<Arc<dyn Notifier>> = vec![Arc::new(StdoutNotifier)];
    add_webhook_notifier(&mut notifiers, config);
    Arc::new(CompositeNotifier { notifiers })
}

#[cfg(feature = "live-cofl")]
pub(super) fn all_flip_notification(account: &AccountId, flip: &FlipEvent) -> Notification {
    let auction_id = flip
        .auction_id
        .as_ref()
        .map(ToString::to_string)
        .unwrap_or_else(|| "unknown".to_string());
    let item_name = strip_minecraft_color_codes(&flip.item_name);
    let buy_kind = flip_buy_kind(flip);
    Notification::new(
        NotificationKind::FlipFound,
        "Flip Found",
        format!(
            "{}: [`{}`](https://sky.coflnet.com/a/{}) `{}` -> `{}` (`{}` profit) [{}] `{}` volume",
            nicer_finder(&flip.finder),
            item_name,
            auction_id,
            format_flip_number(flip.starting_bid),
            format_flip_number(flip.target),
            format_flip_number(flip.profit),
            buy_kind,
            format_flip_volume(flip.volume)
        ),
        Some(account.clone()),
    )
    .with_fields(vec![
        ("Item".to_string(), item_name),
        ("Cost".to_string(), format_flip_number(flip.starting_bid)),
        ("Target".to_string(), format_flip_number(flip.target)),
        ("Profit".to_string(), format_flip_number(flip.profit)),
        ("Finder".to_string(), nicer_finder(&flip.finder)),
    ])
    .with_thumbnail(crate::player_head::account_head_thumbnail_url(
        account.as_str(),
    ))
}

pub(super) fn flip_buy_kind(flip: &FlipEvent) -> &'static str {
    if flip.is_timed_bed() { "BED" } else { "NUGGET" }
}

fn nicer_finder(finder: &str) -> String {
    match finder {
        "USER" => "User".to_string(),
        "SNIPER_MEDIAN" => "Median Sniper".to_string(),
        "TFM" => "TFM".to_string(),
        "AI" => "AI".to_string(),
        "CraftCost" => "Craft Cost".to_string(),
        "SNIPER" => "Sniper".to_string(),
        "STONKS" => "Stonks".to_string(),
        "FLIPPER" => "Flipper".to_string(),
        other => other.to_string(),
    }
}

fn format_flip_volume(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    if value.fract().abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
            .trim_end_matches('0')
            .trim_end_matches('.')
            .to_string()
    }
}

fn format_flip_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_string();
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let abs = value.abs();
    if abs >= 1_000_000_000.0 {
        format!("{sign}{:.1}B", abs / 1_000_000_000.0)
    } else if abs >= 1_000_000.0 {
        format!("{sign}{:.1}M", abs / 1_000_000.0)
    } else if abs >= 1_000.0 {
        format!("{sign}{:.1}K", abs / 1_000.0)
    } else {
        format!("{sign}{abs}")
    }
}

#[cfg(all(feature = "live-cofl", feature = "discord-webhook"))]
pub(super) fn all_flip_notifier(config: &SafConfig) -> Option<Arc<dyn Notifier>> {
    if config.send_all_flips.is_empty() {
        return None;
    }
    Some(Arc::new(
        saf_discord::webhook_notifier::DiscordWebhookNotifier::new(
            config.send_all_flips.clone(),
            saf_discord::DiscordWebhookIdentity {
                username: non_empty(config.branding.name.clone()),
                avatar_url: non_empty(config.branding.icon_url.clone()),
            },
        ),
    ))
}

#[cfg(all(feature = "live-cofl", not(feature = "discord-webhook")))]
pub(super) fn all_flip_notifier(_config: &SafConfig) -> Option<Arc<dyn Notifier>> {
    None
}

#[cfg(feature = "discord-webhook")]
fn add_webhook_notifier(notifiers: &mut Vec<Arc<dyn Notifier>>, config: &SafConfig) {
    if !config.webhook.is_empty() {
        notifiers.push(Arc::new(
            saf_discord::webhook_notifier::DiscordWebhookNotifier::new(
                config.webhook.clone(),
                saf_discord::DiscordWebhookIdentity {
                    username: non_empty(config.branding.name.clone()),
                    avatar_url: non_empty(config.branding.icon_url.clone()),
                },
            ),
        ));
    }
}

#[cfg(not(feature = "discord-webhook"))]
fn add_webhook_notifier(_notifiers: &mut Vec<Arc<dyn Notifier>>, _config: &SafConfig) {}

#[cfg(feature = "discord-webhook")]
fn non_empty(value: String) -> Option<String> {
    (!value.trim().is_empty()).then_some(value)
}

pub(super) fn purchase_notification_body(
    config: &SafConfig,
    account: &AccountId,
    purchase: &PurchaseStatsUpdate,
) -> String {
    let template = config.webhook_format.trim();
    if template.is_empty() {
        return format!(
            "{} bought for {} with {} expected profit.",
            purchase.item_name,
            format_coins(purchase.price as f64),
            format_coins(purchase.profit)
        );
    }

    let price = saf_core::numbers::add_commas_to_number(purchase.price as f64);
    let profit_percentage = purchase.profit_percentage.unwrap_or_else(|| {
        if purchase.price > 0 {
            purchase.profit / purchase.price as f64 * 100.0
        } else {
            0.0
        }
    });
    let buy_kind = if purchase.buy_kind.eq_ignore_ascii_case("BED") {
        "BED"
    } else {
        "NUGGET"
    };
    let args = [
        purchase.item_name.clone(),
        format_flip_number(purchase.profit),
        price,
        format_flip_number(purchase.target_price),
        purchase
            .buy_speed_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "unknown".to_string()),
        buy_kind.to_string(),
        nicer_finder(&purchase.finder),
        purchase.auction_id.clone(),
        format_flip_number(purchase.price as f64),
        account.to_string(),
        format_flip_volume(purchase.volume.unwrap_or(0.0)),
        format_flip_number(profit_percentage),
    ];
    format_webhook_template(template, &args)
}

fn format_webhook_template(template: &str, args: &[String]) -> String {
    let mut formatted = template.to_string();
    for (index, arg) in args.iter().enumerate() {
        formatted = formatted.replace(&format!("{{{index}}}"), arg);
    }
    formatted
}
