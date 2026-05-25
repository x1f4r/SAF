use crate::blacklist::{BlacklistPolicy, BlacklistPolicyHandle, ItemContext};
use crate::buy::{BuyDecision, BuyThresholds, SkipDecision, SkipPolicy, buy_speed_profile};
use crate::flip::FlipEvent;
use crate::ids::AuctionId;
use crate::ports::{MinecraftAction, MinecraftClient, PortError};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug)]
pub struct FlipProcessor {
    blacklist: BlacklistPolicyHandle,
    skip_policy: SkipPolicy,
    thresholds: BuyThresholds,
}

impl FlipProcessor {
    pub fn new(
        blacklist: BlacklistPolicy,
        skip_policy: SkipPolicy,
        thresholds: BuyThresholds,
    ) -> Self {
        Self {
            blacklist: BlacklistPolicyHandle::new(blacklist),
            skip_policy,
            thresholds,
        }
    }

    pub fn blacklist_handle(&self) -> BlacklistPolicyHandle {
        self.blacklist.clone()
    }

    pub async fn process<C: MinecraftClient + ?Sized>(
        &self,
        client: &C,
        flip: FlipEvent,
    ) -> Result<FlipOutcome, PortError> {
        if let Some(reason) = flip.invalid_reason.clone() {
            return Ok(FlipOutcome::IgnoredInvalid {
                item_name: flip.item_name,
                reason,
            });
        }

        let item = ItemContext {
            tag: flip.tag.clone(),
            item_name: Some(flip.item_name.clone()),
            weird_item_name: Some(flip.weird_item_name.clone()),
            ..Default::default()
        };
        if let Some(reason) = self.blacklist.current().buy_block_reason(&item) {
            return Ok(FlipOutcome::IgnoredBlocked {
                item_name: flip.item_name,
                reason: reason.to_string(),
            });
        }

        let Some(auction_id) = flip.auction_id.clone() else {
            return Ok(FlipOutcome::IgnoredInvalid {
                item_name: flip.item_name,
                reason: "missing auction id".to_string(),
            });
        };

        let skip = self.skip_policy.decide(&BuyDecision {
            item_name: flip.item_name.clone(),
            finder: flip.finder.clone(),
            profit: flip.profit,
            profit_percent: flip.profit_percentage,
            price: flip.starting_bid,
        });
        let profile = buy_speed_profile(flip.starting_bid, self.thresholds);

        client
            .perform(MinecraftAction::OpenAuction(auction_id.clone()))
            .await?;

        Ok(FlipOutcome::OpenedAuction {
            auction_id,
            item_name: flip.item_name,
            starting_bid: flip.starting_bid,
            purchase_at_ms: flip.purchase_at_ms,
            profile: profile.name.to_string(),
            skip,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FlipOutcome {
    IgnoredInvalid {
        item_name: String,
        reason: String,
    },
    IgnoredBlocked {
        item_name: String,
        reason: String,
    },
    IgnoredSkipped {
        auction_id: AuctionId,
        item_name: String,
        starting_bid: f64,
        purchase_at_ms: Option<u64>,
        skip: SkipDecision,
    },
    OpenedAuction {
        auction_id: AuctionId,
        item_name: String,
        starting_bid: f64,
        purchase_at_ms: Option<u64>,
        profile: String,
        skip: SkipDecision,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BlockListConfig, RelistBlockConfig, SkipConfig};
    use crate::ids::AccountId;
    use crate::ports::{MinecraftAction, PortError};
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct FakeClient {
        actions: Arc<Mutex<Vec<MinecraftAction>>>,
    }

    #[async_trait]
    impl MinecraftClient for FakeClient {
        async fn account(&self) -> AccountId {
            AccountId::new("Main").unwrap()
        }

        async fn perform(&self, action: MinecraftAction) -> Result<(), PortError> {
            self.actions.lock().unwrap().push(action);
            Ok(())
        }
    }

    fn processor() -> FlipProcessor {
        FlipProcessor::new(
            BlacklistPolicy::from_config(
                &BlockListConfig::default(),
                &RelistBlockConfig::default(),
            ),
            SkipPolicy::from_config(&SkipConfig::default()),
            BuyThresholds::default(),
        )
    }

    #[tokio::test]
    async fn valid_flip_opens_auction_through_client_port() {
        let client = FakeClient::default();
        let outcome = processor()
            .process(
                &client,
                FlipEvent::from_payload(&json!({
                    "id": "auction-1",
                    "itemName": "Hyperion",
                    "startingBid": "30m",
                    "target": "50m",
                    "finder": "SNIPER_MEDIAN"
                })),
            )
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            FlipOutcome::OpenedAuction { profile, .. } if profile == "turbo"
        ));
        assert_eq!(
            client.actions.lock().unwrap().as_slice(),
            &[MinecraftAction::OpenAuction(
                AuctionId::new("auction-1").unwrap()
            )]
        );
    }

    #[tokio::test]
    async fn invalid_or_blocked_flips_do_not_open_auction() {
        let client = FakeClient::default();
        let invalid = processor()
            .process(
                &client,
                FlipEvent::from_payload(&json!({
                    "id": "auction-1",
                    "itemName": "Hyperion",
                    "startingBid": "30m",
                    "target": -1
                })),
            )
            .await
            .unwrap();
        assert!(matches!(invalid, FlipOutcome::IgnoredInvalid { .. }));
        assert!(client.actions.lock().unwrap().is_empty());

        let processor = FlipProcessor::new(
            BlacklistPolicy::from_config(
                &BlockListConfig {
                    tags: vec![json!("GIANTS_SWORD")],
                    ..Default::default()
                },
                &RelistBlockConfig::default(),
            ),
            SkipPolicy::from_config(&SkipConfig::default()),
            BuyThresholds::default(),
        );
        let blocked = processor
            .process(
                &client,
                FlipEvent::from_payload(&json!({
                    "id": "auction-2",
                    "itemName": "Giant's Sword",
                    "tag": "GIANTS_SWORD",
                    "startingBid": "30m",
                    "target": "60m"
                })),
            )
            .await
            .unwrap();
        assert!(matches!(blocked, FlipOutcome::IgnoredBlocked { .. }));
        assert!(client.actions.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn skip_policy_marks_opened_auction_without_suppressing_buy() {
        let client = FakeClient::default();
        let processor = FlipProcessor::new(
            BlacklistPolicy::from_config(
                &BlockListConfig::default(),
                &RelistBlockConfig::default(),
            ),
            SkipPolicy::from_config(&SkipConfig {
                min_profit: json!("25m"),
                ..SkipConfig::default()
            }),
            BuyThresholds::default(),
        );

        let outcome = processor
            .process(
                &client,
                FlipEvent::from_payload(&json!({
                    "id": "auction-1",
                    "itemName": "Ancient Skeleton Master Chestplate",
                    "startingBid": "70",
                    "target": "76m",
                    "profit": "73m"
                })),
            )
            .await
            .unwrap();

        assert!(matches!(
            outcome,
            FlipOutcome::OpenedAuction {
                skip: SkipDecision { reasons },
                ..
            } if reasons == vec![crate::buy::SkipReason::MinProfit, crate::buy::SkipReason::MinPercent]
        ));
        assert_eq!(
            client.actions.lock().unwrap().as_slice(),
            &[MinecraftAction::OpenAuction(
                AuctionId::new("auction-1").unwrap()
            )]
        );
    }
}
