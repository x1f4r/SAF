use super::LiveRuntime;
use super::formatting::format_coins;
use super::notifier::{notify_operator_best_effort, purchase_notification_body};
use super::stats::ChatStatsUpdate;
use super::tracked::SoldListingMetadata;
use anyhow::Result;
use saf_core::AccountId;
use saf_core::ports::{Notification, NotificationKind};

impl LiveRuntime {
    pub(super) async fn handle_chat_stats_update(
        &mut self,
        account: &AccountId,
        update: ChatStatsUpdate,
    ) -> Result<()> {
        if let Some(purchase) = update.purchase {
            self.remove_pending_live_buy(account)?;
            let body = purchase_notification_body(&self.config, account, &purchase);
            notify_operator_best_effort(
                self.session.as_ref(),
                Notification::new(
                    NotificationKind::Bought,
                    "Item purchased",
                    body,
                    Some(account.clone()),
                )
                .with_fields(vec![
                    ("Item".to_string(), purchase.item_name.clone()),
                    ("Cost".to_string(), format_coins(purchase.price as f64)),
                    ("Expected profit".to_string(), format_coins(purchase.profit)),
                ])
                .with_thumbnail(crate::player_head::account_head_thumbnail_url(
                    account.as_str(),
                )),
            )
            .await;
            self.queue_purchase_relist(account, &purchase).await?;
        }

        if let Some(sold) = update.sold {
            self.queue_sold_reconcile_or_defer(account, &sold).await?;
        }

        if let Some(claim) = update.claim {
            let metadata = self
                .sold_tracker
                .take_claim(account, &claim.item_name, claim.coins)
                .map_err(anyhow::Error::from)?;
            let collected = claim_notification_collected_coins(claim.coins, metadata.as_ref());
            if collected != claim.coins {
                tracing::warn!(
                    account = %account,
                    item = %claim.item_name,
                    raw_claim_coins = claim.coins,
                    tracked_collected_coins = collected,
                    "using tracked listing amount for sold notification because claim chat amount was implausible"
                );
            }
            let mut body = format!(
                "Collected {} for selling {} to {}.",
                format_coins(collected as f64),
                claim.item_name,
                claim.buyer
            );
            let mut fields = vec![
                ("Item".to_string(), claim.item_name.clone()),
                ("Collected".to_string(), format_coins(collected as f64)),
                ("Buyer".to_string(), claim.buyer.clone()),
            ];
            if let Some(metadata) = metadata {
                if metadata.profit.is_finite() && metadata.profit != 0.0 {
                    body.push_str(&format!(" Profit: {}.", format_coins(metadata.profit)));
                    fields.push(("Profit".to_string(), format_coins(metadata.profit)));
                }
                if !metadata.auction_id.trim().is_empty() {
                    body.push_str(&format!(" Auction: {}.", metadata.auction_id));
                    fields.push(("Auction".to_string(), metadata.auction_id.clone()));
                }
            }
            notify_operator_best_effort(
                self.session.as_ref(),
                Notification::new(
                    NotificationKind::Sold,
                    "Item sold",
                    body,
                    Some(account.clone()),
                )
                .with_fields(fields)
                .with_thumbnail(crate::player_head::account_head_thumbnail_url(
                    account.as_str(),
                )),
            )
            .await;
        }

        Ok(())
    }
}

pub(super) fn claim_notification_collected_coins(
    raw_coins: u64,
    metadata: Option<&SoldListingMetadata>,
) -> u64 {
    let Some(metadata) = metadata else {
        return raw_coins;
    };
    if tracked_collection_amount_should_replace_claim(raw_coins, metadata.expected_collected) {
        metadata.expected_collected
    } else {
        raw_coins
    }
}

fn tracked_collection_amount_should_replace_claim(raw_coins: u64, expected_collected: u64) -> bool {
    if raw_coins == 0 || expected_collected == 0 {
        return false;
    }
    if raw_coins >= expected_collected {
        return false;
    }
    let tolerance = (expected_collected / 50).max(1_000);
    expected_collected.saturating_sub(raw_coins) > tolerance && raw_coins < expected_collected / 10
}
