use super::auction_flow::transfer_followup;
use super::bank::BankCooldownDecision;
use super::stats::SoldStatsUpdate;
use super::*;
use saf_core::ports::PortError;
use saf_core::{
    BotState, LocalCommand, MarketInstruction, QueueEntry, RuntimeDirective, RuntimeOutcome,
};
use serde_json::json;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

#[derive(Debug)]
struct FailingNotifier;

#[async_trait::async_trait]
impl Notifier for FailingNotifier {
    async fn notify(&self, _notification: Notification) -> Result<(), PortError> {
        Err(PortError::Failed("webhook unavailable".to_string()))
    }
}

#[cfg(feature = "live-cofl")]
#[derive(Debug, Default)]
struct RecordingNotifier {
    notifications: Mutex<Vec<Notification>>,
}

#[cfg(feature = "live-cofl")]
#[async_trait::async_trait]
impl Notifier for RecordingNotifier {
    async fn notify(&self, notification: Notification) -> Result<(), PortError> {
        self.notifications.lock().unwrap().push(notification);
        Ok(())
    }
}

#[derive(Debug)]
struct FixedCookiePriceProvider(Option<f64>);

#[async_trait::async_trait]
impl CookiePriceProvider for FixedCookiePriceProvider {
    async fn booster_cookie_buy_price(&self) -> std::result::Result<Option<f64>, PortError> {
        Ok(self.0)
    }
}

#[derive(Debug)]
struct FailingMinecraftEventClient {
    account: AccountId,
}

#[async_trait::async_trait]
impl MinecraftClient for FailingMinecraftEventClient {
    async fn account(&self) -> AccountId {
        self.account.clone()
    }

    async fn perform(&self, _action: MinecraftAction) -> Result<(), PortError> {
        Ok(())
    }

    async fn next_event(&self) -> Result<Option<MinecraftEvent>, PortError> {
        Err(PortError::Failed("event stream unavailable".to_string()))
    }
}

#[derive(Debug)]
struct FailingMarketMinecraftClient {
    account: AccountId,
    attempts: Arc<Mutex<usize>>,
    events: Arc<Mutex<VecDeque<MinecraftEvent>>>,
}

#[async_trait::async_trait]
impl MinecraftClient for FailingMarketMinecraftClient {
    async fn account(&self) -> AccountId {
        self.account.clone()
    }

    async fn perform(&self, _action: MinecraftAction) -> Result<(), PortError> {
        *self.attempts.lock().unwrap() += 1;
        Err(PortError::Failed("market action unavailable".to_string()))
    }

    async fn next_event(&self) -> Result<Option<MinecraftEvent>, PortError> {
        Ok(self.events.lock().unwrap().pop_front())
    }
}

#[derive(Debug)]
struct CorruptingClickMinecraftClient {
    account: AccountId,
    path: PathBuf,
    corrupted: Arc<Mutex<bool>>,
    actions: Arc<Mutex<Vec<MinecraftAction>>>,
}

#[async_trait::async_trait]
impl MinecraftClient for CorruptingClickMinecraftClient {
    async fn account(&self) -> AccountId {
        self.account.clone()
    }

    async fn perform(&self, action: MinecraftAction) -> Result<(), PortError> {
        self.actions.lock().unwrap().push(action.clone());
        if matches!(action, MinecraftAction::ClickSlot { .. }) {
            let mut corrupted = self.corrupted.lock().unwrap();
            if !*corrupted {
                std::fs::write(&self.path, b"{not valid json")
                    .map_err(|error| PortError::Failed(error.to_string()))?;
                *corrupted = true;
            }
        }
        Ok(())
    }

    async fn next_event(&self) -> Result<Option<MinecraftEvent>, PortError> {
        Ok(None)
    }
}

async fn install_failing_minecraft_client(
    runtime: &mut LiveRuntime,
    account: &AccountId,
) -> Arc<Mutex<usize>> {
    install_failing_minecraft_client_with_events(runtime, account, Vec::new()).await
}

async fn install_failing_minecraft_client_with_events(
    runtime: &mut LiveRuntime,
    account: &AccountId,
    events: Vec<MinecraftEvent>,
) -> Arc<Mutex<usize>> {
    let attempts = Arc::new(Mutex::new(0));
    let client = Arc::new(FailingMarketMinecraftClient {
        account: account.clone(),
        attempts: attempts.clone(),
        events: Arc::new(Mutex::new(events.into_iter().collect())),
    });
    let managed = runtime.managed_minecraft.get(account).unwrap().clone();
    *managed.inner.lock().await = Some(LiveMinecraftClientBundle {
        minecraft: client,
        inventory_provider: None,
    });
    attempts
}

async fn install_recorded_minecraft_client(
    runtime: &mut LiveRuntime,
    account: &AccountId,
) -> Arc<RecordedMinecraftClient> {
    let client = Arc::new(RecordedMinecraftClient::new(account.clone()));
    let managed = runtime.managed_minecraft.get(account).unwrap().clone();
    *managed.inner.lock().await = Some(LiveMinecraftClientBundle {
        minecraft: client.clone(),
        inventory_provider: None,
    });
    runtime
        .minecraft_clients
        .insert(account.clone(), client.clone());
    client
}

#[derive(Default)]
struct FixedInventoryProvider;

#[async_trait]
impl InventoryProvider for FixedInventoryProvider {
    async fn snapshot(&self, account: &AccountId) -> Result<InventorySnapshot, PortError> {
        Ok(InventorySnapshot {
            account: account.clone(),
            items: vec![InventoryItem {
                uuid: Some("priced-item".to_string()),
                item_name: "Aspect of the Dragons".to_string(),
                lore: Vec::new(),
                price: None,
                tag: Some("ASPECT_OF_THE_DRAGON".to_string()),
                slot: Some(10),
                in_hotbar: false,
            }],
        })
    }
}

#[cfg(feature = "live-discord")]
#[derive(Default)]
struct ListingPreviewInventoryProvider;

#[cfg(feature = "live-discord")]
#[async_trait]
impl InventoryProvider for ListingPreviewInventoryProvider {
    async fn snapshot(&self, account: &AccountId) -> Result<InventorySnapshot, PortError> {
        Ok(InventorySnapshot {
            account: account.clone(),
            items: vec![
                InventoryItem {
                    uuid: Some("main-priced".to_string()),
                    item_name: "Aspect of the Dragons".to_string(),
                    lore: Vec::new(),
                    price: Some(1_250_000.0),
                    tag: Some("ASPECT_OF_THE_DRAGON".to_string()),
                    slot: Some(10),
                    in_hotbar: false,
                },
                InventoryItem {
                    uuid: Some("hotbar-priced".to_string()),
                    item_name: "Wither Impact Wand".to_string(),
                    lore: Vec::new(),
                    price: Some(2_500_000.0),
                    tag: Some("WITHER_IMPACT_WAND".to_string()),
                    slot: Some(40),
                    in_hotbar: true,
                },
                InventoryItem {
                    uuid: Some("too-cheap".to_string()),
                    item_name: "Cheap Stone".to_string(),
                    lore: Vec::new(),
                    price: Some(100.0),
                    tag: Some("STONE".to_string()),
                    slot: Some(11),
                    in_hotbar: false,
                },
                InventoryItem {
                    uuid: None,
                    item_name: "Missing UUID".to_string(),
                    lore: Vec::new(),
                    price: Some(1_000_000.0),
                    tag: None,
                    slot: Some(12),
                    in_hotbar: false,
                },
            ],
        })
    }
}

#[cfg(feature = "live-discord")]
#[derive(Default)]
struct FixedActiveAuctionProvider;

#[cfg(feature = "live-discord")]
#[async_trait]
impl ActiveAuctionProvider for FixedActiveAuctionProvider {
    async fn active_auctions(&self, _account: &AccountId) -> Result<Vec<ActiveAuction>, PortError> {
        Ok(vec![
            ActiveAuction {
                auction_id: "auction-1".to_string(),
                item_uuid: "listed-item-1".to_string(),
                name: Some("Aspect of the Dragons".to_string()),
            },
            ActiveAuction {
                auction_id: String::new(),
                item_uuid: "skipped-missing-auction".to_string(),
                name: Some("Incomplete auction row".to_string()),
            },
        ])
    }
}

#[derive(Default)]
struct PurchasedInventoryProvider;

#[async_trait]
impl InventoryProvider for PurchasedInventoryProvider {
    async fn snapshot(&self, account: &AccountId) -> Result<InventorySnapshot, PortError> {
        Ok(InventorySnapshot {
            account: account.clone(),
            items: vec![InventoryItem {
                uuid: Some("inventory-uuid".to_string()),
                item_name: "Ancient Necron's Leggings".to_string(),
                lore: Vec::new(),
                price: Some(56_300_000.0),
                tag: Some("NECRON_LEGGINGS".to_string()),
                slot: Some(12),
                in_hotbar: false,
            }],
        })
    }
}

#[derive(Default)]
struct MutableInventoryProvider {
    items: Arc<Mutex<Vec<InventoryItem>>>,
}

#[async_trait]
impl InventoryProvider for MutableInventoryProvider {
    async fn snapshot(&self, account: &AccountId) -> Result<InventorySnapshot, PortError> {
        Ok(InventorySnapshot {
            account: account.clone(),
            items: self.items.lock().unwrap().clone(),
        })
    }
}

#[derive(Default)]
struct FailingInventoryProvider;

#[async_trait]
impl InventoryProvider for FailingInventoryProvider {
    async fn snapshot(&self, _account: &AccountId) -> Result<InventorySnapshot, PortError> {
        Err(PortError::Failed(
            "inventory snapshot unavailable".to_string(),
        ))
    }
}

#[derive(Default)]
struct FixedInventoryPriceLookup {
    requests: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl InventoryPriceLookup for FixedInventoryPriceLookup {
    async fn lookup(&self, item: &InventoryItem) -> Result<Option<f64>, PortError> {
        if let Some(tag) = &item.tag {
            self.requests.lock().unwrap().push(tag.clone());
        }
        Ok(Some(5_000_000.0))
    }
}

struct FailingInventoryPriceLookup;

#[async_trait]
impl InventoryPriceLookup for FailingInventoryPriceLookup {
    async fn lookup(&self, _item: &InventoryItem) -> Result<Option<f64>, PortError> {
        Err(PortError::Failed("price API down".to_string()))
    }
}

mod auction_reconcile;
mod cofl;
mod command_runtime;
#[cfg(feature = "live-discord")]
mod discord;
mod island_startup;
mod queue_inventory;
