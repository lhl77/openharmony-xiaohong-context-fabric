//! Domain-agnostic context snapshot, delta, and conflict-resolution primitives.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;

pub const SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Error)]
pub enum FabricError {
    #[error("invalid context: {0}")]
    Invalid(String),
    #[error("snapshot schema {0} is unsupported")]
    UnsupportedSchema(u16),
    #[error("invalid delta: {0}")]
    InvalidDelta(String),
}

pub type Result<T> = std::result::Result<T, FabricError>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextEntry {
    pub key: String,
    pub value: serde_json::Value,
    pub revision: u64,
    pub updated_at_ms: u64,
    pub writer_id: String,
    pub tombstone: bool,
}

impl ContextEntry {
    pub fn new(key: impl Into<String>, value: serde_json::Value, revision: u64,
        updated_at_ms: u64, writer_id: impl Into<String>) -> Self {
        Self { key: key.into(), value, revision, updated_at_ms,
            writer_id: writer_id.into(), tombstone: false }
    }

    pub fn tombstone(key: impl Into<String>, revision: u64,
        updated_at_ms: u64, writer_id: impl Into<String>) -> Self {
        Self { key: key.into(), value: serde_json::Value::Null, revision,
            updated_at_ms, writer_id: writer_id.into(), tombstone: true }
    }

    fn order_key(&self) -> (u64, u64, &str) {
        (self.revision, self.updated_at_ms, self.writer_id.as_str())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextSnapshot {
    pub schema_version: u16,
    pub context_id: String,
    pub source_device_id: String,
    pub snapshot_revision: u64,
    pub entries: BTreeMap<String, ContextEntry>,
    pub checksum_sha256: String,
}

impl ContextSnapshot {
    pub fn new(context_id: impl Into<String>, source_device_id: impl Into<String>,
        snapshot_revision: u64, entries: BTreeMap<String, ContextEntry>) -> Result<Self> {
        let mut snapshot = Self { schema_version: SCHEMA_VERSION,
            context_id: context_id.into(), source_device_id: source_device_id.into(),
            snapshot_revision, entries, checksum_sha256: String::new() };
        snapshot.validate()?;
        snapshot.checksum_sha256 = snapshot.compute_checksum()?;
        Ok(snapshot)
    }

    pub fn validate(&self) -> Result<()> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(FabricError::UnsupportedSchema(self.schema_version));
        }
        if self.context_id.trim().is_empty() || self.source_device_id.trim().is_empty() {
            return Err(FabricError::Invalid("contextId and sourceDeviceId are required".into()));
        }
        if self.entries.iter().any(|(key, value)| key != &value.key || key.trim().is_empty()) {
            return Err(FabricError::Invalid("entry keys must be non-empty and match map keys".into()));
        }
        if self.entries.values().any(|entry| entry.revision > self.snapshot_revision) {
            return Err(FabricError::Invalid("entry revision exceeds snapshot revision".into()));
        }
        Ok(())
    }

    pub fn verify_checksum(&self) -> Result<bool> {
        self.validate()?;
        Ok(self.checksum_sha256 == self.compute_checksum()?)
    }

    fn compute_checksum(&self) -> Result<String> {
        #[derive(Serialize)]
        struct Canonical<'a> {
            schema_version: u16,
            context_id: &'a str,
            source_device_id: &'a str,
            snapshot_revision: u64,
            entries: &'a BTreeMap<String, ContextEntry>,
        }
        let bytes = serde_json::to_vec(&Canonical { schema_version: self.schema_version,
            context_id: &self.context_id, source_device_id: &self.source_device_id,
            snapshot_revision: self.snapshot_revision, entries: &self.entries })
            .map_err(|e| FabricError::Invalid(e.to_string()))?;
        Ok(hex::encode(Sha256::digest(bytes)))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ContextDelta {
    pub context_id: String,
    pub from_revision: u64,
    pub to_revision: u64,
    pub entries: Vec<ContextEntry>,
}

impl ContextDelta {
    pub fn validate(&self) -> Result<()> {
        if self.context_id.trim().is_empty() || self.to_revision <= self.from_revision {
            return Err(FabricError::InvalidDelta("contextId required and toRevision must exceed fromRevision".into()));
        }
        let mut keys = std::collections::BTreeSet::new();
        for entry in &self.entries {
            if entry.key.trim().is_empty() || entry.revision > self.to_revision
                || !keys.insert(entry.key.as_str()) {
                return Err(FabricError::InvalidDelta("entries need unique keys and revisions within delta".into()));
            }
        }
        Ok(())
    }

    pub fn apply_to(&self, snapshot: &mut ContextSnapshot) -> Result<MergeReport> {
        self.validate()?;
        snapshot.validate()?;
        if snapshot.context_id != self.context_id {
            return Err(FabricError::InvalidDelta("delta contextId does not match snapshot".into()));
        }
        if snapshot.snapshot_revision != self.from_revision {
            return Err(FabricError::InvalidDelta("delta base revision does not match snapshot".into()));
        }
        let report = merge_entries(&mut snapshot.entries, &self.entries);
        snapshot.snapshot_revision = self.to_revision;
        snapshot.checksum_sha256 = snapshot.compute_checksum()?;
        Ok(report)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MergeReport {
    pub applied: Vec<String>,
    pub ignored: Vec<String>,
}

/// Deterministic last-writer ordering: revision, timestamp, then writer ID.
/// Applications with non-total or security-sensitive conflict semantics should supply a policy above this layer.
pub fn merge_entries(target: &mut BTreeMap<String, ContextEntry>, incoming: &[ContextEntry]) -> MergeReport {
    let mut report = MergeReport { applied: Vec::new(), ignored: Vec::new() };
    for entry in incoming {
        let apply = target.get(&entry.key).is_none_or(|current| entry.order_key() > current.order_key());
        if apply {
            report.applied.push(entry.key.clone());
            target.insert(entry.key.clone(), entry.clone());
        } else {
            report.ignored.push(entry.key.clone());
        }
    }
    report.applied.sort();
    report.ignored.sort();
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delta_merge_is_idempotent_and_rejects_stale_updates() {
        let mut state = BTreeMap::new();
        let newer = ContextEntry::new("task/state", serde_json::json!({"step":"review"}), 3, 30, "phone");
        let report = merge_entries(&mut state, std::slice::from_ref(&newer));
        assert_eq!(report.applied, vec!["task/state"]);
        assert!(merge_entries(&mut state, std::slice::from_ref(&newer)).applied.is_empty());
        let stale = ContextEntry::new("task/state", serde_json::json!({"step":"draft"}), 2, 40, "tablet");
        assert_eq!(merge_entries(&mut state, &[stale]).ignored, vec!["task/state"]);
    }

    #[test]
    fn tombstone_wins_and_snapshot_detects_tampering() {
        let mut state = BTreeMap::new();
        state.insert("session/title".into(), ContextEntry::new("session/title", serde_json::json!("x"), 1, 1, "a"));
        let deleted = ContextEntry::tombstone("session/title", 2, 2, "a");
        merge_entries(&mut state, &[deleted]);
        assert!(state["session/title"].tombstone);
        let mut snapshot = ContextSnapshot::new("c1", "phone", 3, state).unwrap();
        assert!(snapshot.verify_checksum().unwrap());
        snapshot.entries.get_mut("session/title").unwrap().revision = 3;
        assert!(!snapshot.verify_checksum().unwrap());
    }

    #[test]
    fn delta_rejects_duplicates_and_bad_range() {
        let entry = ContextEntry::new("k", serde_json::Value::Null, 4, 1, "d");
        let delta = ContextDelta { context_id: "c".into(), from_revision: 1, to_revision: 3,
            entries: vec![entry.clone(), entry] };
        assert!(delta.validate().is_err());
    }

    #[test]
    fn delta_applies_only_to_matching_context_revision() {
        let mut snapshot = ContextSnapshot::new("c", "phone", 2, BTreeMap::new()).unwrap();
        let delta = ContextDelta { context_id: "c".into(), from_revision: 2, to_revision: 3,
            entries: vec![ContextEntry::new("workflow/status", serde_json::json!("ready"), 3, 8, "tablet")] };
        assert_eq!(delta.apply_to(&mut snapshot).unwrap().applied, vec!["workflow/status"]);
        assert!(snapshot.verify_checksum().unwrap());
        assert!(delta.apply_to(&mut snapshot).is_err());
    }
}
