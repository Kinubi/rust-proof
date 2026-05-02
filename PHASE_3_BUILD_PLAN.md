# rust-proof Phase 3 Build Plan

Date: 2026-05-01  
Status: planning for the host-runtime build; `erp-runtime/` is now far enough along that the missing counterpart is `rp-runtime/`

## 1. Purpose

This document is the Phase 3 execution plan for `rp-runtime/`.

In roadmap terms this corresponds to M4.

Phase 2 delivered the first real embedded runtime shell around `rp-node` in `erp-runtime/`. That work is now far enough along that the next missing milestone is no longer inside the ESP runtime. The missing milestone is the first real host or desktop peer that can speak the same wire profile, host the same node engine, and complete end-to-end peer smoke against the device runtime.

The Phase 3 goal is therefore straightforward:

- turn `rp-runtime/` from a skeleton into the first host runtime for `rp-node`
- make it the first real counterpart to `erp-runtime/`
- keep the implementation buildable throughout the rewrite

## 2. Current Baseline

These facts are true at the start of Phase 3:

- `rp-core/` and `rp-node/` already exist as the shared engine path.
- `erp-runtime/` now hosts the first honest runtime shell around `rp-node`.
- `erp-runtime/` already implements the current minimal compatible transport stack:
  - TCP
  - multistream-select `/multistream/1.0.0`
  - Noise `/noise` using `Noise_XX_25519_ChaChaPoly_SHA256`
  - Yamux `/yamux/1.0.0`
  - Identify `/ipfs/id/1.0.0`
  - `NodeHello`
  - `Sync`
  - `Announce`
- `rp-runtime/` is still a stale skeleton rather than a real runtime host.
- `rp-runtime/src/main.rs` still uses an older `Node::new` and `NodeCommand` path rather than the current `NodeEngine` contract.
- `rp-runtime/src/network/manager.rs` still contains an older full-libp2p experiment built around Gossipsub, Kademlia, and JSON request-response.
- `rp-runtime/src/storage.rs` still implements an older storage trait shape that no longer matches the current `rp-node::contract::Storage` boundary.
- the repository root Cargo config still defaults to the ESP target, so host validation must keep using explicit host targets such as `--target x86_64-unknown-linux-gnu`.

That means the Phase 3 objective is not to polish the existing `rp-runtime/` code. The objective is to replace the transitional host skeleton with a runtime that actually matches the current `rp-node` boundary and the current `erp-runtime` wire profile.

## 3. Phase 3 Objective

Build `rp-runtime` as the first desktop or server runtime host for `rp-node`.

In practical terms, that means:

- `rp-runtime` owns host sockets, storage, timers, identity persistence, logging, and process lifecycle
- `rp-node` remains the canonical behavior engine
- `rp-runtime` drives `NodeEngine::step` with real runtime events and executes the resulting `NodeAction`s
- `rp-runtime` speaks the same minimal compatible session profile that `erp-runtime` already implements
- configuration is loaded at runtime rather than compiled into the binary

## 4. Non-Goals For The First Phase 3 Slice

The first Phase 3 slice should stay narrow.

Out of scope for the first `rp-runtime` delivery:

- reviving the older Gossipsub or Kademlia experiment
- switching the ESP runtime back to a full libp2p stack
- extracting a new shared transport crate before the host runtime works end to end
- archival-node storage or full history indexing
- local control APIs, RPC, wallet-web hosting, or operator UX beyond what is required to boot and peer
- production metrics and fleet-management work
- ambitious peer discovery beyond static or configured bootstrap peers

## 5. Compatibility Profile To Match

The Phase 3 runtime must match the profile already implemented in `erp-runtime/`.

### Transport and session stack

- TCP only for v1
- multistream-select v1
- Noise XX with X25519 static DH keys and libp2p-compatible identity binding
- Yamux after Noise
- Identify over `/ipfs/id/1.0.0`
- custom runtime protocols:
  - `/rust-proof/node-hello/1`
  - `/rust-proof/sync/1`
  - `/rust-proof/announce/1`

### Identity model

- canonical node identity remains `rp_node::contract::PeerId = [u8; 32]`
- transport identity remains separate from node identity
- the host runtime should persist a libp2p-compatible transport identity on disk
- the first host implementation should default to a file-backed ECDSA P-256 transport key so it matches the embedded runtime's current transport-identity choice and wire expectations

### Bootstrap and addressing

- use the same bootstrap peer string format already documented for `erp-runtime/`:
  - `/ip4/<addr>/tcp/<port>`
  - `/dns4/<host>/tcp/<port>`
  - optionally `@<libp2p-peer-id>` to pin the expected remote transport identity
- unlike `erp-runtime/`, load bootstrap peers at process start from runtime config rather than from build-time environment variables

## 6. Design Constraints

1. `rp-node` stays device-agnostic.
2. `rp-runtime` must be wire-compatible with `erp-runtime`, not merely feature-similar.
3. Runtime adapters stay thin and explicit.
4. The first host slice should prefer direct adaptation of the proven `erp-runtime` session/profile code over broad abstraction work.
5. Host validation commands must remain explicit about the host target because the repository root defaults to the ESP target.
6. Runtime configuration should be runtime-loaded and inspectable.
7. Per-session Tokio tasks are acceptable for v1. If a crate requires `futures::io::AsyncRead` and `AsyncWrite`, adapt Tokio streams with `tokio-util` compatibility layers rather than blocking the runtime.

## 7. Definition Of Done For The First Phase 3 Delivery

Phase 3 for `rp-runtime` is complete when all of the following are true:

- `rp-runtime` depends on `rp-node` and constructs a `NodeEngine`
- startup can initialize runtime services and create the engine cleanly
- the runtime can feed at least these inputs into the node engine:
  - `Tick`
  - `PeerConnected`
  - `PeerDisconnected`
  - `FrameReceived`
  - `StorageLoaded`
  - `PersistCompleted`
- the runtime can execute at least these actions correctly:
  - `SendFrame`
  - `BroadcastFrame`
  - `PersistBlock`
  - `PersistSnapshot`
  - `RequestBlocks`
  - `ScheduleWake`
  - `LoadSnapshot`
  - `DisconnectPeer`
  - `ReportEvent`
- startup recovery can either:
  - boot from genesis, or
  - restore the latest persisted snapshot bundle and continue
- `rp-runtime` can listen for TCP peers and dial configured bootstrap peers
- `rp-runtime` can complete Identify and `NodeHello` against `erp-runtime`
- `rp-runtime` can complete at least one `Sync` request or response exchange with `erp-runtime`
- `rp-runtime` can complete at least one `Announce` flow with `erp-runtime`
- `cargo check -p rp-runtime --target x86_64-unknown-linux-gnu` passes
- `cargo test -p rp-runtime --target x86_64-unknown-linux-gnu` passes for the covered host slices
- a manual end-to-end smoke run demonstrates that `rp-runtime` can act as the peer counterpart for `erp-runtime`

## 8. Delivery Order

The order matters.

Do not start by extracting a shared transport crate or by reworking peer discovery. Start by replacing the stale host shell with a real runtime structure, then port the already-proven compatibility profile into a host-friendly form.

### M4.0 Freeze The Host Runtime Boundary

Objective:

Freeze what `rp-runtime` owns and what remains in `rp-node`.

Tasks:

- confirm that `rp-runtime` is the first host peer for the already-implemented embedded runtime
- freeze the v1 compatibility profile listed above
- confirm that the older Gossipsub, Kademlia, and JSON request-response experiment is retired rather than extended
- define the smallest boot path that can drive `NodeEngine`
- define the runtime-loaded config surface for listen address, bootstrap peers, data directory, and logging

Exit criteria:

- this Phase 3 file is accepted
- there is no ambiguity about runtime versus engine ownership
- there is no ambiguity about the wire profile the host runtime must implement

### M4.1 Rewrite Bootstrap And Runtime Shell

Objective:

Replace the stale `main.rs` skeleton with a real host runtime shell.

Tasks:

- remove the older `Node::new` and `NodeCommand` startup path
- create a runtime bootstrap that owns:
  - `NodeEngine`
  - runtime config
  - runtime event channels
  - adapter instances for network, storage, clock, wake, and identity
- define a runtime manager that translates between adapter events and `NodeInput`
- keep the first startup path small: boot, restore, listen, schedule ticks, and run the main event pump

Recommended files:

- `rp-runtime/src/main.rs`
- `rp-runtime/src/lib.rs`
- `rp-runtime/src/runtime/mod.rs`
- `rp-runtime/src/runtime/config.rs`
- `rp-runtime/src/runtime/errors.rs`
- `rp-runtime/src/runtime/manager.rs`

Exit criteria:

- `rp-runtime` compiles with a constructed `NodeEngine`
- runtime ownership is moved out of `main.rs`

### M4.2 Replace The Storage Adapter

Objective:

Make host persistence match the current `rp-node` storage boundary and startup-recovery needs.

Tasks:

- replace the older `Storage` implementation with one that matches the current trait:
  - `save_block`
  - `load_block`
  - `save_snapshot`
  - `load_snapshot`
- define a durable Sled layout, for example:
  - `meta`
  - `blocks`
  - `snapshots`
- add a helper that can recover the latest snapshot bundle at startup
- persist enough metadata to restore the latest head block and associated state bytes without scanning the entire database
- document cleanup and overwrite behavior for repeated snapshots at the same height

Recommended files:

- `rp-runtime/src/storage/mod.rs`
- `rp-runtime/src/storage/sled_storage.rs`

Exit criteria:

- the storage adapter matches the current `rp-node::contract::Storage` trait
- startup can load a latest snapshot bundle or cleanly fall back to genesis

### M4.3 Implement Host Identity And Config Surfaces

Objective:

Give the host runtime stable file-backed identities and a stable process config surface.

Tasks:

- add a node identity manager for the canonical `[u8; 32]` node peer identity
- add a file-backed transport identity manager for the libp2p-compatible transport key
- define on-disk locations for identity material under the runtime data directory
- implement runtime config loading for:
  - listen address or port
  - bootstrap peers
  - data directory
  - frame limits and session limits
  - log level through normal host logging configuration
- keep the first config surface env-based or simple file-based if that keeps the rewrite smaller

Recommended files:

- `rp-runtime/src/identity/mod.rs`
- `rp-runtime/src/identity/manager.rs`
- `rp-runtime/src/network/transport_identity.rs`
- `rp-runtime/src/runtime/config.rs`

Exit criteria:

- `rp-runtime` has stable persisted identities across restarts
- a host operator can point the runtime at a data directory and bootstrap peers without recompiling

### M4.4 Build The Host Session Stack

Objective:

Port the already-proven embedded profile into a host-friendly networking implementation.

Tasks:

- replace the older libp2p `Swarm` experiment in `rp-runtime/src/network/manager.rs`
- implement the same session-control design already used in `erp-runtime`:
  - network manager as control plane
  - session worker per active connection
  - peer registry for session ownership and node-peer mapping
- add host socket adapters for Tokio TCP streams
- port or adapt the following protocol and transport pieces:
  - multistream
  - Noise
  - Yamux
  - Identify
  - `NodeHello`
  - `Sync`
  - `Announce`
- use `tokio-util` compatibility wrappers where third-party crates require `futures::io` traits
- keep the first host slice limited to static or configured bootstrap peers

Recommended files:

- `rp-runtime/src/network/mod.rs`
- `rp-runtime/src/network/config.rs`
- `rp-runtime/src/network/bootstrap.rs`
- `rp-runtime/src/network/manager.rs`
- `rp-runtime/src/network/session.rs`
- `rp-runtime/src/network/peer_registry.rs`
- `rp-runtime/src/network/socket/mod.rs`
- `rp-runtime/src/network/socket/tokio.rs`
- `rp-runtime/src/network/socket/traits.rs`
- `rp-runtime/src/network/protocol/`
- `rp-runtime/src/network/transport/`

Exit criteria:

- `rp-runtime` can accept and dial TCP sessions
- `rp-runtime` can complete Noise, Yamux, Identify, and `NodeHello` against `erp-runtime`

### M4.5 Wire The Runtime Event Loop

Objective:

Make `rp-runtime` a real host that drives `NodeEngine::step`.

Tasks:

- create the runtime event pump
- translate runtime events into `NodeInput`
- execute `NodeAction`s by calling host adapters
- connect Tokio timers to `Tick` and `ScheduleWake`
- connect storage results to `StorageLoaded` and `PersistCompleted`
- connect session events to `PeerConnected`, `PeerDisconnected`, and `FrameReceived`
- log `ReportEvent` messages in a host-appropriate way

Important rule:

`rp-node` decides behavior. `rp-runtime` only translates between OS events and the node contract.

Exit criteria:

- the runtime can drive a real `step` loop
- network, storage, and timers are all connected through the same runtime manager

### M4.6 Validate Against `erp-runtime`

Objective:

Use the new host runtime as the real embedded peer counterpart.

Tasks:

- run host validation commands with explicit host targets
- run a local host instance listening on a known TCP address
- boot `erp-runtime` with `BOOTSTRAP_PEERS` pointing at the host instance
- verify end-to-end milestones in order:
  - TCP connect
  - Noise + Yamux session establishment
  - Identify success
  - `NodeHello` verification
  - one `Sync` exchange
  - one `Announce` exchange
- document the smoke-test commands and expected log signatures

Exit criteria:

- `rp-runtime` is usable as the first real peer counterpart for `erp-runtime`
- the repository has a repeatable host-plus-device smoke path

## 9. Recommended Module Layout

The first Phase 3 implementation should bias toward familiarity with the embedded runtime layout so code review stays local and mechanical.

Recommended target layout:

- `rp-runtime/src/main.rs`
- `rp-runtime/src/lib.rs`
- `rp-runtime/src/runtime/`
  - `mod.rs`
  - `config.rs`
  - `errors.rs`
  - `manager.rs`
- `rp-runtime/src/storage/`
  - `mod.rs`
  - `sled_storage.rs`
- `rp-runtime/src/identity/`
  - `mod.rs`
  - `manager.rs`
- `rp-runtime/src/network/`
  - `mod.rs`
  - `bootstrap.rs`
  - `config.rs`
  - `manager.rs`
  - `peer_registry.rs`
  - `session.rs`
  - `transport_identity.rs`
  - `socket/`
  - `protocol/`
  - `transport/`

Important note:

Do not block the first host runtime on extracting a new shared crate for session logic. The first priority is a working peer. If duplication between `erp-runtime` and `rp-runtime` becomes painful after both sides are validated, extraction can happen in the hardening phase.

## 10. Cargo And Dependency Direction

The current `rp-runtime/Cargo.toml` is pointed at the older libp2p experiment. The first Phase 3 slice should move it toward the same lower-level session stack already proven on the embedded side.

Expected dependency direction:

- keep:
  - `rp-core`
  - `rp-node`
  - `tokio`
  - `sled`
- remove from the first host slice:
  - the broad `libp2p` dependency used for Gossipsub, Kademlia, and JSON request-response
  - `serde_json` as the primary wire format
- add or align with:
  - `futures`
  - `postcard`
  - `quick-protobuf`
  - `unsigned-varint`
  - `multiaddr`
  - `snow`
  - `yamux`
  - `libp2p-identity`
  - `tokio-util` with compatibility support

The goal is not ideological purity. The goal is to stop paying for a second, incompatible networking design inside `rp-runtime`.

## 11. Recommended Validation Commands

Because the repository root defaults to the ESP target, host validation commands should stay explicit.

Primary host checks:

- `cargo check -p rp-runtime --target x86_64-unknown-linux-gnu`
- `cargo test -p rp-runtime --target x86_64-unknown-linux-gnu`

First host runtime smoke target:

- `cargo run -p rp-runtime --target x86_64-unknown-linux-gnu`

First cross-runtime smoke target:

- host runtime listens locally
- embedded runtime boots with `BOOTSTRAP_PEERS` pointing at the host runtime

## 12. Sequencing Guidance

The easiest way to get lost in Phase 3 is to try to clean up everything at once.

Recommended implementation order:

1. make `rp-runtime` compile around `NodeEngine` and the new runtime manager
2. replace storage and startup recovery
3. add file-backed identities and runtime config
4. replace the stale network manager with the minimal compatible session stack
5. wire runtime events to `NodeEngine::step`
6. prove peer smoke against `erp-runtime`

If a tradeoff appears between architectural cleanliness and producing the first honest host peer, prefer the path that gets the host peer working while keeping the boundary with `rp-node` clean.