use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BotState {
    Moving,
    Idle,
    GettingReady,
    Buying,
    Claiming,
    Listing,
    ListingNoName,
    Delisting,
    Expired,
    Diagnostics,
    Stopping,
    Custom(String),
}

impl BotState {
    pub fn persisted_queue_state(&self) -> bool {
        matches!(self, Self::Listing | Self::ListingNoName)
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Moving => "moving",
            Self::Idle => "idle",
            Self::GettingReady => "getting ready",
            Self::Buying => "buying",
            Self::Claiming => "claiming",
            Self::Listing => "listing",
            Self::ListingNoName => "listingNoName",
            Self::Delisting => "delisting",
            Self::Expired => "expired",
            Self::Diagnostics => "diagnostics",
            Self::Stopping => "stopping",
            Self::Custom(value) => value,
        }
    }
}

impl From<&str> for BotState {
    fn from(value: &str) -> Self {
        match value {
            "moving" => Self::Moving,
            "idle" => Self::Idle,
            "getting ready" | "gettingReady" => Self::GettingReady,
            "buying" => Self::Buying,
            "claiming" => Self::Claiming,
            "listing" => Self::Listing,
            "listingNoName" => Self::ListingNoName,
            "delisting" => Self::Delisting,
            "expired" => Self::Expired,
            "diagnostics" => Self::Diagnostics,
            "stopping" => Self::Stopping,
            other => Self::Custom(other.to_string()),
        }
    }
}

impl Serialize for BotState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for BotState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(Self::from(value.as_str()))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct QueueEntry {
    pub action: Value,
    pub state: BotState,
    pub priority: u8,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedDataClear {
    pub queue_removed: usize,
    pub bid_data_cleared: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedQueueState {
    #[serde(default = "empty_bid_data")]
    pub bid_data: Value,
    #[serde(default)]
    pub queue: Vec<QueueEntry>,
}

impl Default for SavedQueueState {
    fn default() -> Self {
        Self {
            bid_data: empty_bid_data(),
            queue: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct StateStore {
    path: PathBuf,
    queue: Vec<QueueEntry>,
    bid_data: Value,
}

impl StateStore {
    pub fn open(base_dir: impl AsRef<Path>, account_uuid: &str) -> Result<Self, StateStoreError> {
        let path = base_dir
            .as_ref()
            .join("SavedData")
            .join(format!("{account_uuid}.json"));
        let saved = read_saved_state(&path)?;
        Ok(Self {
            path,
            queue: saved.queue,
            bid_data: saved.bid_data,
        })
    }

    pub fn queue(&self) -> &[QueueEntry] {
        &self.queue
    }

    pub fn bid_data(&self) -> &Value {
        &self.bid_data
    }

    pub fn bid_data_entry(&self, item_uuid: &str) -> Option<&Value> {
        self.bid_data.as_object()?.get(item_uuid)
    }

    pub fn remove_bid_data_entry(&mut self, item_uuid: &str) -> Option<Value> {
        self.bid_data.as_object_mut()?.remove(item_uuid)
    }

    pub fn add(&mut self, action: Value, state: BotState, priority: u8) -> bool {
        if matches!(state, BotState::Custom(ref value) if value == "claimSold" || value == "reconcileAuctions")
            && self.queue.iter().any(|entry| {
                matches!(&entry.state, BotState::Custom(value) if value == "claimSold" || value == "reconcileAuctions")
            })
        {
            return false;
        }

        self.queue.push(QueueEntry {
            action,
            state,
            priority,
        });
        self.queue.sort_by_key(|entry| entry.priority);
        true
    }

    pub fn remove_next(&mut self) -> Option<QueueEntry> {
        (!self.queue.is_empty()).then(|| self.queue.remove(0))
    }

    pub fn remove_first_matching(
        &mut self,
        predicate: impl Fn(&QueueEntry) -> bool,
    ) -> Option<QueueEntry> {
        let index = self.queue.iter().position(predicate)?;
        Some(self.queue.remove(index))
    }

    pub fn clear_matching(&mut self, predicate: impl Fn(&QueueEntry) -> bool) {
        self.queue.retain(|entry| !predicate(entry));
    }

    pub fn remove_at(&mut self, index: usize) -> Option<QueueEntry> {
        (index < self.queue.len()).then(|| self.queue.remove(index))
    }

    pub fn clear_queue(&mut self) -> usize {
        let removed = self.queue.len();
        self.queue.clear();
        removed
    }

    pub fn clear_saved_data(&mut self) -> SavedDataClear {
        let queue_removed = self.clear_queue();
        let bid_data_cleared = !is_empty_bid_data(&self.bid_data);
        self.bid_data = empty_bid_data();
        SavedDataClear {
            queue_removed,
            bid_data_cleared,
        }
    }

    pub fn save(&self) -> Result<(), StateStoreError> {
        let persisted = SavedQueueState {
            bid_data: self.bid_data.clone(),
            queue: self
                .queue
                .iter()
                .filter(|entry| entry.state.persisted_queue_state())
                .cloned()
                .collect(),
        };

        self.write_saved_state(persisted)
    }

    pub fn save_all(&self) -> Result<(), StateStoreError> {
        self.write_saved_state(SavedQueueState {
            bid_data: self.bid_data.clone(),
            queue: self.queue.clone(),
        })
    }

    fn write_saved_state(&self, persisted: SavedQueueState) -> Result<(), StateStoreError> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let temp_path = self.path.with_extension("json.tmp");
        fs::write(&temp_path, serde_json::to_vec_pretty(&persisted)?)?;
        fs::rename(temp_path, &self.path)?;
        Ok(())
    }
}

fn read_saved_state(path: &Path) -> Result<SavedQueueState, StateStoreError> {
    if !path.exists() {
        return Ok(SavedQueueState {
            bid_data: empty_bid_data(),
            queue: Vec::new(),
        });
    }
    let raw = fs::read(path)?;
    if raw.iter().all(u8::is_ascii_whitespace) {
        return Ok(SavedQueueState::default());
    }
    Ok(serde_json::from_slice(&raw)?)
}

fn empty_bid_data() -> Value {
    Value::Object(Default::default())
}

fn is_empty_bid_data(value: &Value) -> bool {
    matches!(value, Value::Object(map) if map.is_empty())
}

#[derive(Debug, thiserror::Error)]
pub enum StateStoreError {
    #[error("state io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("state json error: {0}")]
    Json(#[from] serde_json::Error),
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn persists_only_resumable_entries() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = StateStore::open(temp.path(), "account").unwrap();
        store.add(json!({"auctionID": "a"}), BotState::Buying, 1);
        store.add(json!({"auctionID": "b"}), BotState::Listing, 4);
        store.save().unwrap();

        let restored = StateStore::open(temp.path(), "account").unwrap();
        assert_eq!(restored.queue().len(), 1);
        assert_eq!(restored.queue()[0].state, BotState::Listing);
    }

    #[test]
    fn node_queue_states_round_trip_as_strings() {
        let custom = serde_json::from_value::<BotState>(json!("claimSold")).unwrap();
        assert_eq!(custom, BotState::Custom("claimSold".to_string()));
        assert_eq!(serde_json::to_value(custom).unwrap(), json!("claimSold"));
        assert_eq!(
            serde_json::to_value(BotState::GettingReady).unwrap(),
            json!("getting ready")
        );
        assert_eq!(
            serde_json::from_value::<BotState>(json!("gettingReady")).unwrap(),
            BotState::GettingReady
        );
    }

    #[test]
    fn loads_node_saved_data_with_custom_queue_states() {
        let temp = tempfile::tempdir().unwrap();
        let saved_dir = temp.path().join("SavedData");
        fs::create_dir_all(&saved_dir).unwrap();
        fs::write(
            saved_dir.join("account.json"),
            serde_json::to_vec(&json!({
                "bidData": {"auction": "a"},
                "queue": [
                    {"action": {"reason": "startup"}, "state": "claimSold", "priority": 2},
                    {"action": {"auctionID": "b"}, "state": "listingNoName", "priority": 4}
                ]
            }))
            .unwrap(),
        )
        .unwrap();

        let mut store = StateStore::open(temp.path(), "account").unwrap();
        assert_eq!(store.bid_data(), &json!({"auction": "a"}));
        assert_eq!(
            store.queue()[0].state,
            BotState::Custom("claimSold".to_string())
        );
        assert!(!store.add(
            json!({"reason": "duplicate"}),
            BotState::Custom("reconcileAuctions".to_string()),
            2
        ));
        store.save().unwrap();

        let restored = StateStore::open(temp.path(), "account").unwrap();
        assert_eq!(restored.queue().len(), 1);
        assert_eq!(restored.queue()[0].state, BotState::ListingNoName);
    }

    #[test]
    fn save_all_keeps_intentional_transient_queue_entries() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = StateStore::open(temp.path(), "account").unwrap();
        store.add(
            json!({"amount": 50_000_000}),
            BotState::Custom("bank".to_string()),
            5,
        );
        store.save_all().unwrap();

        let restored = StateStore::open(temp.path(), "account").unwrap();
        assert_eq!(restored.queue().len(), 1);
        assert_eq!(
            restored.queue()[0].state,
            BotState::Custom("bank".to_string())
        );
    }

    #[test]
    fn clear_saved_data_removes_queue_and_bid_data() {
        let temp = tempfile::tempdir().unwrap();
        let mut store = StateStore::open(temp.path(), "account").unwrap();
        store.bid_data = json!({"auction-1": {"target": 10_000_000}});
        store.add(json!({"auctionID": "a"}), BotState::Listing, 4);
        store.add(
            json!({"amount": 50_000_000}),
            BotState::Custom("bank".to_string()),
            5,
        );

        let cleared = store.clear_saved_data();
        assert_eq!(
            cleared,
            SavedDataClear {
                queue_removed: 2,
                bid_data_cleared: true,
            }
        );
        assert!(store.queue().is_empty());
        assert_eq!(store.bid_data(), &json!({}));
    }

    #[test]
    fn missing_saved_data_fields_default_to_safe_empty_values() {
        let temp = tempfile::tempdir().unwrap();
        let saved_dir = temp.path().join("SavedData");
        fs::create_dir_all(&saved_dir).unwrap();
        fs::write(saved_dir.join("account.json"), br#"{"queue":[]}"#).unwrap();

        let store = StateStore::open(temp.path(), "account").unwrap();
        assert_eq!(store.bid_data(), &json!({}));
        assert!(store.queue().is_empty());
    }
}
