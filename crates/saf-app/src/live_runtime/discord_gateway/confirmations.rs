use super::super::formatting::format_coins;
use super::controls::discord_button;
use super::{DiscordInteractionReply, account_selection_error, resolve_discord_account};
use saf_core::ports::{InventoryItem, InventorySnapshot};
use saf_core::{AccountId, RuntimeDirective, RuntimeOutcome, RuntimeSession};

pub(super) fn confirm_status_action_reply(
    session: &RuntimeSession,
    username: &str,
    action: &str,
) -> DiscordInteractionReply {
    match action {
        "clear_data" => {
            confirm_clear_data_reply(session, (!username.is_empty()).then_some(username))
        }
        _ => DiscordInteractionReply::content(format!(
            "Confirmation action `{action}` is not supported by this runtime."
        )),
    }
}

fn confirm_clear_data_reply(
    session: &RuntimeSession,
    username: Option<&str>,
) -> DiscordInteractionReply {
    let Some(account) = resolve_discord_account(session, username) else {
        return DiscordInteractionReply::content(account_selection_error(session, username));
    };
    DiscordInteractionReply::with_components(
        format!(
            "Confirm clearing saved queue and bid recovery data for `{account}`. Only do this when recovery data is stale or blocking normal operation."
        ),
        vec![serenity::builder::CreateActionRow::Buttons(vec![
            discord_button(
                format!("saf:confirmClearData:{account}"),
                "Clear Saved Data",
                serenity::all::ButtonStyle::Danger,
            ),
            discord_button(
                "saf:cancel",
                "Cancel",
                serenity::all::ButtonStyle::Secondary,
            ),
        ])],
    )
}

pub(super) fn confirm_delist_everything_reply(
    session: &RuntimeSession,
    username: Option<&str>,
) -> DiscordInteractionReply {
    let Some(account) = resolve_discord_account(session, username) else {
        return DiscordInteractionReply::content(account_selection_error(session, username));
    };
    DiscordInteractionReply::with_components(
        format!(
            "Confirm scanning active auctions for `{account}` and queueing every found auction for delisting."
        ),
        vec![serenity::builder::CreateActionRow::Buttons(vec![
            discord_button(
                format!("saf:confirmDelistAll:{account}"),
                "Scan + Queue Delists",
                serenity::all::ButtonStyle::Danger,
            ),
            discord_button(
                format!("saf:account:{account}"),
                "Account Panel",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                "saf:cancel",
                "Cancel",
                serenity::all::ButtonStyle::Secondary,
            ),
        ])],
    )
}

pub(super) async fn confirm_sell_inventory_reply(
    session: &RuntimeSession,
    username: Option<&str>,
    include_hotbar: bool,
) -> DiscordInteractionReply {
    let Some(account) = resolve_discord_account(session, username) else {
        return DiscordInteractionReply::content(account_selection_error(session, username));
    };
    let Some(account_id) = AccountId::new(account.clone()) else {
        return DiscordInteractionReply::content(account_selection_error(session, username));
    };
    let include_hotbar_flag = if include_hotbar { 1 } else { 0 };
    let mut content = format!(
        "Confirm queueing inventory listings for `{account}`. Hotbar items are {}.",
        if include_hotbar {
            "included"
        } else {
            "excluded"
        }
    );
    content.push('\n');
    content.push_str(
        &inventory_listing_confirmation_preview(session, &account_id, include_hotbar).await,
    );
    DiscordInteractionReply::with_components(
        content,
        vec![serenity::builder::CreateActionRow::Buttons(vec![
            discord_button(
                format!("saf:confirmSellInventory:{account}:{include_hotbar_flag}"),
                "Queue Listings",
                serenity::all::ButtonStyle::Danger,
            ),
            discord_button(
                format!("saf:inventory:{account}"),
                "Back to Inventory",
                serenity::all::ButtonStyle::Secondary,
            ),
            discord_button(
                "saf:cancel",
                "Cancel",
                serenity::all::ButtonStyle::Secondary,
            ),
        ])],
    )
}

async fn inventory_listing_confirmation_preview(
    session: &RuntimeSession,
    account: &AccountId,
    include_hotbar: bool,
) -> String {
    match session
        .execute_directive(RuntimeDirective::ShowInventory {
            account: account.clone(),
        })
        .await
    {
        Ok(RuntimeOutcome::InventorySnapshot { snapshot, .. }) => {
            format_inventory_listing_preview(&snapshot, include_hotbar)
        }
        Ok(RuntimeOutcome::Planned { .. }) => {
            "Preview unavailable: no live inventory provider is registered.".to_string()
        }
        Ok(_) => "Preview unavailable: inventory provider returned no snapshot.".to_string(),
        Err(error) => format!("Preview unavailable: {error}"),
    }
}

const INVENTORY_LISTING_PREVIEW_LIMIT: usize = 12;

pub(in crate::live_runtime) fn format_inventory_listing_preview(
    snapshot: &InventorySnapshot,
    include_hotbar: bool,
) -> String {
    let candidates = snapshot
        .items
        .iter()
        .filter(|item| inventory_preview_item_is_listable(item, include_hotbar))
        .collect::<Vec<_>>();
    let mut lines = vec![format!(
        "Preview: {} listable inventory item(s).",
        candidates.len()
    )];
    if candidates.is_empty() {
        lines.push("No listable inventory items found.".to_string());
        return lines.join("\n");
    }

    lines.extend(
        candidates
            .iter()
            .take(INVENTORY_LISTING_PREVIEW_LIMIT)
            .map(|item| {
                format!(
                    "- {} `{}` {}{}",
                    item.item_name,
                    item.uuid.as_deref().unwrap_or("unknown"),
                    item.price
                        .map(format_coins)
                        .unwrap_or_else(|| "unknown".to_string()),
                    item.slot
                        .map(|slot| format!(" slot {slot}"))
                        .unwrap_or_default()
                )
            }),
    );
    if candidates.len() > INVENTORY_LISTING_PREVIEW_LIMIT {
        lines.push(format!(
            "...and {} more listable item(s).",
            candidates.len() - INVENTORY_LISTING_PREVIEW_LIMIT
        ));
    }
    lines.join("\n")
}

fn inventory_preview_item_is_listable(item: &InventoryItem, include_hotbar: bool) -> bool {
    item.uuid
        .as_ref()
        .is_some_and(|uuid| !uuid.trim().is_empty())
        && item
            .price
            .is_some_and(|price| price.is_finite() && price >= 500.0)
        && (include_hotbar || !item.in_hotbar)
}
