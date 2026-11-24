# M4 Context Fabric API

M4 provides domain-agnostic versioned context entries, snapshots, deltas, integrity checks, and deterministic merge primitives. It has no dependency on tasks, notes, chat, or any specific business schema: callers namespace keys and store JSON values.

## Rust API

- `ContextEntry::new(key, value, revision, updated_at_ms, writer_id)` creates a live value.
- `ContextEntry::tombstone(...)` records deletion while preserving a revision marker.
- `ContextSnapshot::new(context_id, source_device_id, snapshot_revision, entries)` validates the schema and creates a SHA-256 checksum.
- `ContextSnapshot::verify_checksum()` detects accidental or untrusted payload changes. The checksum is not a signature or authentication mechanism.
- `ContextDelta::validate()` checks the range and unique keys.
- `merge_entries(target, incoming)` applies entries by `(revision, updatedAtMs, writerId)` order and is idempotent for duplicate delivery.

## Contract and limits

- Snapshot checksum canonicalizes the typed structure through serde JSON and is intended for integrity detection, not authorization.
- The default merge is deterministic last-writer ordering. Applications with semantic conflicts, counters, approvals, or security-sensitive fields must supply a field-specific policy and may need explicit user resolution.
- Tombstones must be retained until every replica is known to have observed them; premature removal can resurrect deleted values.
- Device discovery, transport, durable storage, encryption, identity, access control, and background synchronization are adapters/host responsibilities. This crate does not claim to implement OpenHarmony soft-bus or distributed KV integration.
- Snapshot serialization is caller-controlled; do not include data a receiving device is not allowed to access.

## Verification

Run `cargo test --locked` and `cargo check --locked`. OpenHarmony cross-build requires the target SDK/NDK and is not implied by host tests.
