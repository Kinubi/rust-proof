# rust-proof System Design

Date: 2026-04-22  
Status: target architecture for the next major refactor

## 1. Summary

`rust-proof` is being designed as an embedded-first blockchain stack with one canonical blockchain engine, one canonical node engine, multiple runtimes, and one wallet application.

The target crate model is:

- `rp-core`
- `rp-node`
- `rp-runtime`
- `erp-runtime`
- `rp-client`

Only the wallet application is intentionally `std`-bound by default. The blockchain engine and node engine are intended to be `no_std + alloc` so they can run in both server-class and embedded runtimes.

This document describes the target system design. It is intentionally more detailed than the ADR and more stable than implementation notes.

## 2. Design Goals

The architecture is designed to satisfy all of the following at the same time:

1. The ESP must be a first-class peer in the P2P network.
2. The PC side must also be able to host a node.
3. Blockchain rules must live in one canonical engine, not in duplicated runtime code.
4. Node behavior must also live in one canonical engine, not in duplicated host and embedded logic.
5. Runtime-specific concerns such as sockets, Wi-Fi, flash, timers, and process management must stay outside the shared engines.
6. The wallet must remain usable on desktop and optionally be hostable as a webpage by a runtime, including the ESP runtime.
7. The design must remain honest about current repository state while clearly describing the target architecture.

## 3. Target Crate Model

## `rp-core`

Role: blockchain engine.

Constraints:

- `no_std + alloc`
- deterministic
- no transport awareness
- no filesystem or database awareness
- no async executor awareness

Responsibilities:

- transaction model
- block model
- slash-proof model
- canonical binary encoding
- hashing
- signature verification
- state transition logic
- fork choice inputs and outputs
- validator selection logic
- typed validation and consensus errors

Non-responsibilities:

- peer management
- sync state machines
- networking
- storage adapters
- process lifecycle
- wallet UI

## `rp-node`

Role: device-agnostic node engine.

Constraints:

- `no_std + alloc`
- event-driven
- transport-agnostic
- storage-agnostic
- clock-agnostic

Responsibilities:

- peer state tracking
- sync logic
- block import orchestration
- mempool admission policy
- message validation and routing
- capability negotiation
- node-role handling
- coordination between peers and `rp-core`

Non-responsibilities:

- socket APIs
- Wi-Fi stack integration
- libp2p runtime coupling
- file or flash drivers
- OS timers
- web serving
- wallet UX

## `rp-runtime`

Role: desktop or server runtime for `rp-node`.

Constraints:

- `std`
- can depend on async runtimes, host networking, and host storage

Responsibilities:

- drive the `rp-node` event loop
- implement transport adapters for host networking
- implement persistent storage adapters
- implement timers and wakeups
- host observability and process lifecycle
- optionally expose local control APIs and host the wallet web assets

This crate is a runtime shell, not the canonical node logic.

## `erp-runtime`

Role: embedded runtime for `rp-node`.

Constraints:

- target-specific
- memory-bounded
- transport and storage must be explicit and bounded

Responsibilities:

- drive the same `rp-node` engine on embedded hardware
- provide transport, storage, clock, wake, and identity adapters
- integrate with the ESP network stack and timers
- persist bounded chain state and peer metadata
- optionally host the wallet web UI from device storage

This crate is the embedded node host. It is not just a lightweight client.

## `rp-client`

Role: wallet and operator application.

Constraints:

- `std`
- not consensus-critical
- should consume stable APIs from runtimes

Responsibilities:

- wallet UX
- operator CLI and diagnostics
- transaction construction and signing flows
- account and node inspection
- optional build target for wallet web assets served by `rp-runtime` or `erp-runtime`

`rp-client` is not the canonical node. It is the user-facing application layer.

## 4. Current Repository To Target Mapping

The repository now uses the target crate names at the directory level, but the internal code has not finished migrating to the target boundaries.

Current state:

- current `rp-core/`
  - still contains mixed code that must be split into true `rp-core` and `rp-node` boundaries
- current `rp-node/`
  - exists as the destination crate for the shared node engine but is not implemented yet
- current `rp-runtime/`
  - exists as the destination host runtime shell but is not implemented yet
- current `rp-client/`
  - exists as the wallet application scaffold
- current `erp-runtime/`
  - exists as the embedded runtime scaffold

This matters because the current tree still contains mixed code in `rp-core/`, including host runtime concerns that do not belong in the final `rp-core` or `rp-node` boundary.

## 5. Node Engine Design

`rp-node` is the singular node engine shared by desktop and embedded runtimes.

It should be designed as a pure state machine that consumes inputs and produces outputs.

### Input classes

- `Tick`
- `PeerConnected`
- `PeerDisconnected`
- `FrameReceived`
- `LocalTransactionSubmitted`
- `StorageLoaded`
- `PersistCompleted`
- `ImportRequested`

### Output classes

- `SendFrame`
- `BroadcastFrame`
- `PersistBlock`
- `PersistSnapshot`
- `RequestBlocks`
- `ScheduleWake`
- `DisconnectPeer`
- `ReportEvent`

This keeps the node engine platform-independent while still allowing it to drive real networking and storage through runtime adapters.

## 6. Runtime Trait Boundary

`rp-node` should define the environment boundary. The runtimes should implement it.

The exact API can change, but the architectural seam should look like this.

### Transport boundary

The node engine must not know whether bytes are moving over libp2p, raw TCP, ESP transport glue, or something else.

Conceptually the runtime must provide:

- peer addressing
- send to peer
- broadcast to peers
- receive inbound frames
- connection lifecycle events

### Storage boundary

The node engine must not know whether data is persisted to files, a KV store, or flash sectors.

Conceptually the runtime must provide:

- load head and essential chain metadata
- load blocks or snapshots by identifier
- persist blocks
- persist state snapshots
- persist peer or sync metadata if required

### Clock and wake boundary

The node engine must not depend on OS timers directly.

Conceptually the runtime must provide:

- current tick or time source where needed
- scheduling of future wakes
- delivery of wake events back into the node engine

### Identity boundary

The node engine may need a node identity and sometimes a signing interface for node-level actions.

Conceptually the runtime must provide:

- node identifier
- validator identity if enabled
- access to signing operations or signing services

### Observability boundary

The node engine should emit structured events rather than writing directly to logs.

The runtime decides how those events are logged, exported, or ignored.

## 7. Peer Roles And Capabilities

All runtimes should speak the same node protocol and advertise capabilities explicitly.

Recommended capability model:

- protocol version
- chain identifier
- max frame size
- can serve headers
- can serve recent blocks
- can serve state proofs
- validator enabled or disabled
- archival, full, pruned, or relay profile

This allows embedded peers to be first-class citizens without pretending every peer must be archival.

## 8. Protocol Surfaces

The design has two distinct protocol surfaces.

### Peer protocol

Shared by `rp-node` regardless of runtime.

Recommended message families:

- handshake and capability exchange
- peer status and head exchange
- transaction announcements
- transaction requests and responses
- block announcements
- block requests and responses
- header sync requests and responses
- state-proof requests and responses

### Control API

Exposed by a runtime for human or application interaction.

Recommended control API uses:

- node status
- peer list or peer summary
- chain head
- account balance and nonce
- transaction submission
- validator/operator actions
- wallet hosting and local device control

The control API is runtime-facing. The peer protocol is node-facing. They should not be conflated.

## 9. Wallet Placement

The wallet belongs in `rp-client`, not in `rp-node`.

However, `rp-runtime` and `erp-runtime` may optionally host a wallet webpage.

That means the wallet UI can be:

- run locally as a desktop CLI or app through `rp-client`
- built as static web assets from `rp-client`
- served by `rp-runtime`
- served by `erp-runtime`

### Security model for ESP-hosted wallet UI

If the wallet webpage is served by the ESP runtime, the private key should stay on the device by default.

Recommended model:

- browser renders the wallet UI
- browser requests operations from the runtime
- runtime signs on-device or through device-controlled key material
- browser does not become the default source of truth for keys

## 10. Storage Model

The storage policy can differ by runtime, but the node engine must assume bounded and explicit behavior.

### `rp-runtime` storage expectations

- can support full or archival histories
- can support richer indexes
- can host explorer or wallet APIs if desired

### `erp-runtime` storage expectations

- should default to pruned or bounded storage
- must minimize write amplification
- must recover safely after power loss
- should persist only what is required for correct participation and restart recovery

The engine should not require archival storage to remain protocol-compatible.

## 11. Networking Strategy

`rp-node` should not hard-wire libp2p.

That does not mean libp2p is banned. It means libp2p becomes an implementation option for a runtime transport, not the definition of the node engine.

This gives the project freedom to:

- use libp2p in `rp-runtime`
- use a constrained or custom transport in `erp-runtime`
- preserve one canonical protocol and node behavior above the transport layer

## 12. Migration Strategy

The migration from the current repository should happen in this order:

1. finish the architecture rewrite and freeze the target crate model
2. reduce `rp-core/` until the blockchain engine is isolated
3. extract the device-agnostic node engine out of the current mixed runtime code
4. fill in `rp-runtime/` as the host runtime shell
5. evolve `erp-runtime/` into the embedded runtime shell
6. evolve `rp-client/` into the wallet application
7. move protocol and runtime interfaces to the correct crates only after the seams are stable

## 13. Design Rules

1. Consensus rules live only in `rp-core`.
2. Node behavior lives only in `rp-node`.
3. Runtimes implement environment boundaries and stay thin.
4. Wallet UX stays out of the node engine.
5. The embedded runtime is a first-class peer runtime, not a second-class client.
6. Desktop and embedded nodes must share one node engine.
7. `no_std` is a property of the shared engines, not of every runtime.
8. The current repository names are transitional and must not be mistaken for the final architecture.

## 14. Version 0.1 Target

Version `0.1` should demonstrate:

- one canonical blockchain engine
- one canonical node engine
- one desktop runtime hosting a peer node
- one embedded runtime hosting a peer node
- one wallet application
- protocol compatibility across desktop and embedded peers

That is the smallest release that proves the architecture is real.