use async_trait::async_trait;
use saf_core::gui::{WindowSlot, WindowSnapshot};
use saf_core::ids::AccountId;
use saf_core::ports::{
    InventoryItem, InventoryProvider, InventorySnapshot, MinecraftAction, MinecraftClient,
    MinecraftEvent, PortError,
};
use std::sync::Mutex;
use tokio::sync::mpsc;

pub use azalea as azalea_crate;

mod metadata;
mod tracker;

use metadata::azalea_item_metadata;
#[cfg(test)]
use metadata::minecraft_item_name;
use tracker::AzaleaWindowTracker;
#[cfg(test)]
use tracker::{map_azalea_event, map_disconnect_reason};

const INVENTORY_COMPONENT_WAIT: std::time::Duration = std::time::Duration::from_secs(5);
const INVENTORY_COMPONENT_POLL: std::time::Duration = std::time::Duration::from_millis(250);
const SIGN_INPUT_SETTLE: std::time::Duration = std::time::Duration::from_millis(250);
const MAX_UNMAPPED_EVENTS_PER_POLL: usize = 256;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AzaleaAuthMode {
    Offline,
    Microsoft { cache_key: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AzaleaClientConfig {
    pub account: AccountId,
    pub server: String,
    pub auth: AzaleaAuthMode,
}

impl AzaleaClientConfig {
    pub fn offline(account: AccountId, server: impl Into<String>) -> Self {
        Self {
            account,
            server: server.into(),
            auth: AzaleaAuthMode::Offline,
        }
    }

    pub fn microsoft(
        account: AccountId,
        server: impl Into<String>,
        cache_key: impl Into<String>,
    ) -> Self {
        Self {
            account,
            server: server.into(),
            auth: AzaleaAuthMode::Microsoft {
                cache_key: cache_key.into(),
            },
        }
    }
}

pub struct AzaleaMinecraftClient {
    account: AccountId,
    client: azalea_crate::Client,
    events: Mutex<mpsc::UnboundedReceiver<azalea_crate::Event>>,
    window_tracker: Mutex<AzaleaWindowTracker>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MicrosoftAuthReport {
    pub username: String,
    pub uuid: String,
}

pub async fn authenticate_microsoft_cache(
    cache_key: &str,
) -> Result<MicrosoftAuthReport, azalea_crate::auth::AuthError> {
    let account = azalea_crate::account::Account::microsoft(cache_key).await?;
    Ok(MicrosoftAuthReport {
        username: account.username().to_string(),
        uuid: account.uuid().to_string(),
    })
}

impl AzaleaMinecraftClient {
    pub async fn connect(config: AzaleaClientConfig) -> Result<Self, AzaleaConnectError> {
        let azalea_account = match &config.auth {
            AzaleaAuthMode::Offline => {
                azalea_crate::account::Account::offline(config.account.as_str())
            }
            AzaleaAuthMode::Microsoft { cache_key } => {
                azalea_crate::account::Account::microsoft(cache_key).await?
            }
        };
        let (client, events) =
            azalea_crate::Client::join(azalea_account, config.server.as_str()).await?;
        Ok(Self::from_joined(config.account, client, events))
    }

    pub fn from_joined(
        account: AccountId,
        client: azalea_crate::Client,
        events: mpsc::UnboundedReceiver<azalea_crate::Event>,
    ) -> Self {
        disable_azalea_auto_reconnect(&client);
        Self {
            account,
            client,
            events: Mutex::new(events),
            window_tracker: Mutex::new(AzaleaWindowTracker::default()),
        }
    }

    pub fn raw_client(&self) -> &azalea_crate::Client {
        &self.client
    }

    pub fn current_window_snapshot(&self) -> Result<Option<WindowSnapshot>, PortError> {
        let inventory = self
            .client
            .get_inventory()
            .map_err(|error| azalea_inventory_get_error(&self.account, error.to_string()))?;
        let Some(slots) = inventory.slots() else {
            return Ok(None);
        };
        let title = inventory
            .title()
            .map(|title| title.to_string())
            .unwrap_or_default();
        Ok(Some(WindowSnapshot {
            title,
            slots: slots
                .iter()
                .enumerate()
                .filter(|(_, item)| item.is_present())
                .map(|(slot, item)| {
                    let metadata = azalea_item_metadata(item);
                    WindowSlot {
                        slot,
                        name: metadata.item_name,
                        display_name: metadata.display_name,
                        lore: metadata.lore,
                        item_uuid: metadata.item_uuid,
                    }
                })
                .collect(),
        }))
    }

    fn current_window_snapshot_for_event_poll(&self) -> Result<Option<WindowSnapshot>, PortError> {
        match self.current_window_snapshot() {
            Ok(snapshot) => Ok(snapshot),
            Err(PortError::Unavailable(_)) => Ok(None),
            Err(error) => Err(error),
        }
    }

    pub fn action_chat_command(action: &MinecraftAction) -> Option<String> {
        match action {
            MinecraftAction::Chat(message) => Some(message.clone()),
            MinecraftAction::OpenAuction(auction_id) => {
                Some(format!("/viewauction {}", auction_id.as_str()))
            }
            MinecraftAction::ClickSlot(_)
            | MinecraftAction::SwapSlotToHotbar { .. }
            | MinecraftAction::SetHeldHotbarSlot(_)
            | MinecraftAction::ActivateHeldItem
            | MinecraftAction::TypeText(_)
            | MinecraftAction::CloseWindow
            | MinecraftAction::Disconnect => None,
        }
    }

    async fn inventory_with_component_wait(
        &self,
    ) -> Result<azalea_crate::container::ContainerHandleRef, PortError> {
        let deadline = std::time::Instant::now() + INVENTORY_COMPONENT_WAIT;
        loop {
            match self.client.get_inventory() {
                Ok(inventory) => return Ok(inventory),
                Err(error) => {
                    let message = error.to_string();
                    if !azalea_inventory_component_is_pending(&message) {
                        return Err(PortError::Failed(message));
                    }
                    if std::time::Instant::now() >= deadline {
                        return Err(azalea_inventory_get_error(&self.account, message));
                    }
                    tokio::time::sleep(INVENTORY_COMPONENT_POLL).await;
                }
            }
        }
    }
}

impl Drop for AzaleaMinecraftClient {
    fn drop(&mut self) {
        shutdown_azalea_client(&self.client);
    }
}

#[async_trait]
impl MinecraftClient for AzaleaMinecraftClient {
    async fn account(&self) -> AccountId {
        self.account.clone()
    }

    async fn perform(&self, action: MinecraftAction) -> Result<(), PortError> {
        match action {
            MinecraftAction::Chat(message) => {
                self.client.chat(message);
                Ok(())
            }
            MinecraftAction::OpenAuction(auction_id) => {
                self.client
                    .chat(format!("/viewauction {}", auction_id.as_str()));
                Ok(())
            }
            MinecraftAction::ClickSlot(slot) => {
                let inventory = self.inventory_with_component_wait().await?;
                inventory.left_click(slot);
                Ok(())
            }
            MinecraftAction::SwapSlotToHotbar { slot, hotbar_slot } => {
                let inventory = self.inventory_with_component_wait().await?;
                inventory.click(azalea_inventory::operations::SwapClick {
                    source_slot: slot as u16,
                    target_slot: hotbar_slot,
                });
                Ok(())
            }
            MinecraftAction::SetHeldHotbarSlot(slot) => {
                self.client.set_selected_hotbar_slot(slot);
                Ok(())
            }
            MinecraftAction::ActivateHeldItem => {
                self.client.start_use_item();
                Ok(())
            }
            MinecraftAction::TypeText(text) => {
                tokio::time::sleep(SIGN_INPUT_SETTLE).await;
                let pos = self
                    .client
                    .position()
                    .map(sign_update_position)
                    .map_err(|error| PortError::Failed(error.to_string()))?;
                self.client.write_packet(
                    azalea_crate::protocol::packets::game::ServerboundSignUpdate {
                        pos,
                        is_front_text: true,
                        lines: sign_update_lines(&text),
                    },
                );
                Ok(())
            }
            MinecraftAction::CloseWindow => {
                let inventory = self.inventory_with_component_wait().await?;
                inventory.close();
                Ok(())
            }
            MinecraftAction::Disconnect => {
                shutdown_azalea_client(&self.client);
                Ok(())
            }
        }
    }

    async fn next_event(&self) -> Result<Option<MinecraftEvent>, PortError> {
        let mut events = self
            .events
            .lock()
            .map_err(|_| PortError::Failed("azalea events lock poisoned".to_string()))?;
        let mut unmapped = 0;
        loop {
            match events.try_recv() {
                Ok(event) => {
                    let mut tracker = self.window_tracker.lock().map_err(|_| {
                        PortError::Failed("azalea window tracker lock poisoned".to_string())
                    })?;
                    if let Some(event) = tracker.observe_event(event) {
                        if matches!(
                            event,
                            MinecraftEvent::Kicked { .. } | MinecraftEvent::Disconnected { .. }
                        ) {
                            shutdown_azalea_client(&self.client);
                        }
                        return Ok(Some(event));
                    }
                    unmapped += 1;
                    if unmapped >= MAX_UNMAPPED_EVENTS_PER_POLL {
                        break;
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    shutdown_azalea_client(&self.client);
                    return Ok(Some(MinecraftEvent::Disconnected {
                        reason: "azalea event stream closed".to_string(),
                    }));
                }
            }
        }
        drop(events);

        let snapshot = self.current_window_snapshot_for_event_poll()?;
        let mut tracker = self
            .window_tracker
            .lock()
            .map_err(|_| PortError::Failed("azalea window tracker lock poisoned".to_string()))?;
        Ok(tracker.observe_current_window_snapshot(snapshot))
    }
}

#[async_trait]
impl InventoryProvider for AzaleaMinecraftClient {
    async fn snapshot(&self, account: &AccountId) -> Result<InventorySnapshot, PortError> {
        if account != &self.account {
            return Err(PortError::Unavailable(format!(
                "azalea client is connected for {}, not {account}",
                self.account
            )));
        }
        let inventory = self.inventory_with_component_wait().await?;
        let items = inventory_items_from_slots(inventory.slots().unwrap_or_default());
        Ok(InventorySnapshot {
            account: self.account.clone(),
            items,
        })
    }
}

fn inventory_items_from_slots(slots: Vec<azalea_inventory::ItemStack>) -> Vec<InventoryItem> {
    let total_slots = slots.len();
    slots
        .into_iter()
        .enumerate()
        .filter(|(_, item)| item.is_present())
        .filter_map(|(slot, item)| {
            let slot = canonical_player_inventory_slot(slot, total_slots)?;
            let metadata = azalea_item_metadata(&item);
            Some(InventoryItem {
                uuid: metadata.item_uuid,
                item_name: metadata.display_name,
                lore: metadata.lore,
                price: None,
                tag: metadata.skyblock_tag,
                slot: Some(slot),
                in_hotbar: (36..=44).contains(&slot),
            })
        })
        .collect()
}

fn canonical_player_inventory_slot(raw_slot: usize, total_slots: usize) -> Option<u8> {
    if total_slots > 46 {
        let player_inventory_start = total_slots.checked_sub(36)?;
        let player_inventory_end = player_inventory_start + 36;
        if !(player_inventory_start..player_inventory_end).contains(&raw_slot) {
            return None;
        }
        return u8::try_from(raw_slot - player_inventory_start + 9).ok();
    }
    if raw_slot <= 44 {
        u8::try_from(raw_slot).ok()
    } else {
        None
    }
}

fn azalea_inventory_component_is_pending(message: &str) -> bool {
    message.contains("missing a required component")
        && message.contains("azalea_entity::inventory::Inventory")
}

fn azalea_inventory_get_error(account: &AccountId, message: String) -> PortError {
    if azalea_inventory_component_is_pending(&message) {
        PortError::Unavailable(format!("inventory is not ready for {account}: {message}"))
    } else {
        PortError::Failed(message)
    }
}

fn disable_azalea_auto_reconnect(client: &azalea_crate::Client) {
    let mut ecs = client.ecs.write();
    ecs.remove_resource::<azalea_crate::auto_reconnect::AutoReconnectDelay>();
    let mut entity = ecs.entity_mut(client.entity);
    entity.insert(azalea_crate::auto_reconnect::AutoReconnectDelay::new(
        std::time::Duration::MAX,
    ));
    entity.remove::<azalea_crate::auto_reconnect::InternalReconnectAfter>();
}

fn shutdown_azalea_client(client: &azalea_crate::Client) {
    disable_azalea_auto_reconnect(client);
    client.disconnect();
    client.exit();
}

#[derive(Debug, thiserror::Error)]
pub enum AzaleaConnectError {
    #[error("azalea auth failed: {0}")]
    Auth(#[from] azalea_crate::auth::AuthError),
    #[error("azalea connect failed: {0}")]
    Join(#[from] azalea_crate::protocol::resolve::ResolveError),
}

fn sign_update_position(position: azalea_crate::Vec3) -> azalea_crate::BlockPos {
    azalea_crate::BlockPos::from(position.west(1.0))
}

fn sign_update_lines(text: &str) -> [String; 4] {
    [
        text.to_string(),
        "^^^^^^^^^^^^^^^".to_string(),
        "    Auction    ".to_string(),
        "     hours     ".to_string(),
    ]
}

#[cfg(test)]
mod tests;
