use saf_core::ports::{InventoryItem, InventorySnapshot};

const INVENTORY_LOG_ITEM_LIMIT: usize = 80;

pub(super) fn log_inventory_snapshot(snapshot: &InventorySnapshot, source: &'static str) {
    let items = snapshot
        .items
        .iter()
        .take(INVENTORY_LOG_ITEM_LIMIT)
        .map(format_inventory_item)
        .collect::<Vec<_>>();

    tracing::info!(
        account = %snapshot.account,
        source,
        item_count = snapshot.items.len(),
        logged_items = items.len(),
        items = ?items,
        "inventory snapshot items"
    );
}

fn format_inventory_item(item: &InventoryItem) -> String {
    let slot = item
        .slot
        .map(|slot| slot.to_string())
        .unwrap_or_else(|| "-".to_string());
    let uuid = item.uuid.as_deref().unwrap_or("-");
    let tag = item.tag.as_deref().unwrap_or("-");
    let price = item
        .price
        .filter(|price| price.is_finite())
        .map(|price| format!("{price:.0}"))
        .unwrap_or_else(|| "-".to_string());
    let location = if item.in_hotbar {
        "hotbar"
    } else {
        "inventory"
    };

    format!(
        "slot={slot} tag={tag} uuid={uuid} price={price} location={location} name={}",
        item.item_name
    )
}
