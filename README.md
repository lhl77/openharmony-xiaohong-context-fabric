# Xiaohong M4 Context Fabric

A domain-agnostic Rust library for versioned context snapshots and synchronization primitives. It models arbitrary namespaced JSON context, not a board-note workflow.

## Capabilities

- Versioned entries with writer identity, timestamps, and deletion tombstones.
- Snapshot envelopes with schema version and SHA-256 integrity checksum.
- Delta validation and deterministic, idempotent merge by revision, timestamp, and writer ID.
- `rlib`, `cdylib`, and `staticlib` crate outputs for host adapters.

## Scope

This crate is the portable state-model/core, not a complete distributed service. Device discovery, OpenHarmony soft-bus/KV transport, persistence, encryption, identity and authorization are host adapters and are not implemented here. The checksum is not a signature. Default last-writer-wins is unsuitable for every data type; applications must provide conflict policy for approvals, counters, and concurrent edits. See [API.md](API.md).

## Verify

`cargo test --locked` and `cargo check --locked`.


License: MIT.
