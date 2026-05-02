# rust-proof Phase 4 Build Plan

Date: 2026-05-02  
Status: proposed execution plan for Milestone M5 (`rp-client`)

## 1. Purpose

Phase 4 turns `rp-client/` from a small CLI scaffold into the wallet and operator application layer for the repository.

The working architectural assumption for this phase is:

- the reusable part of `rp-client` should be a shared client or wallet manager that either `rp-runtime` or `erp-runtime` can host
- the full CLI shell and any future browser UX remain thin adapters around that shared manager
- runtime hosting is an application capability, not a reason to move wallet logic into the runtimes or back into `rp-node`
- `rp-client` should own the client-facing HTTP API semantics and workflow handlers, while each runtime owns the actual HTTP server, socket binding, auth, and lifecycle
- local wallet or operator requests must not be routed through the P2P network managers

This keeps the repository on the five-crate model while still allowing both runtimes to expose wallet or operator surfaces later.

## 2. Scope And Non-Goals

### In scope

- restructure `rp-client` into a real library plus thin binary
- define a shared `ClientManager` or equivalent service boundary inside `rp-client`
- add wallet flows: key management, transaction construction, signing, submission, and node diagnostics
- define the client-facing API contract that runtimes may expose or host
- support both standalone client mode and runtime-hosted client mode

### Out of scope

- peer networking or consensus logic in `rp-client`
- duplicating sync or mempool behavior already owned by `rp-node`
- forcing the browser to become the default source of truth for keys on embedded devices
- creating a sixth long-term architecture crate just to hold wallet logic unless Phase 4 proves that split is necessary

## 3. Target Architecture

## `rp-client` crate shape

By the end of this phase, `rp-client` should be both:

- a reusable library for wallet and operator workflows
- a small CLI binary that exercises that library

Recommended internal layout:

- `rp-client/src/lib.rs`
- `rp-client/src/main.rs`
- `rp-client/src/manager.rs`
- `rp-client/src/api.rs`
- `rp-client/src/runtime_api.rs`
- `rp-client/src/wallet/`
- `rp-client/src/tx/`
- `rp-client/src/diagnostics/`
- `rp-client/src/hosted/`

The exact module names can change, but the split should make the reusable manager logic obvious.

## Shared manager role

`ClientManager` should own the application-layer workflows that are currently missing from the scaffold.

Expected responsibilities:

- wallet lifecycle and key selection
- address derivation and account presentation
- transaction assembly and signing
- transaction submission through a runtime-facing client boundary
- operator diagnostics such as node status, head height, sync health, and peer visibility
- packaging or describing any optional hosted assets or hosted wallet routes

It should not become another peer runtime manager. It is a wallet and operator service layer.

## Runtime hosting model

The shared manager should be hostable by either runtime, but the runtime adapters should stay thin.

Recommended split:

- `rp-client` owns wallet and operator logic
- `rp-runtime` may host a desktop or local HTTP surface backed by `rp-client`
- `erp-runtime` may host an embedded HTTP surface or asset bundle backed by `rp-client`
- each runtime owns request serving, socket or HTTP integration, auth hooks, and lifecycle
- the shared client manager owns workflow logic, request validation, and response shaping where possible

This means the full desktop CLI app is not embedded directly into the runtimes. The reusable library surface is.

## HTTP ownership and request flow

The intended request path for hosted mode is:

1. a runtime-owned HTTP listener accepts the request
2. the runtime host adapter parses transport details and converts the request into shared `rp-client` API types
3. the runtime host adapter calls `ClientManager`
4. `ClientManager` executes the workflow and calls back into a narrow runtime-facing trait for node inspection, transaction submission, or runtime-backed signing
5. the runtime host adapter turns the shared response back into HTTP

Important constraint:

- the P2P `NetworkManager` in either runtime is not the HTTP dispatch path and should not become a generic local API router

## Runtime manager topology

The current runtime shape already uses separate managers for node, network, storage, and wake responsibilities.

Phase 4 should extend that pattern instead of mixing local API work into existing P2P managers.

Recommended runtime-side topology:

- `NodeManager` remains the owner of `NodeEngine` progression
- `NetworkManager` remains responsible for peer transport and protocol sessions only
- `StorageManager` remains responsible for persistence only
- `Wake` handling remains responsible for timer scheduling only
- a new `ClientHostManager` or `LocalApiManager` owns the local API surface and runtime-hosted wallet entrypoints

Recommended `ClientHostManager` responsibilities:

- bind and serve the local HTTP surface
- enforce auth, request-size limits, and connection policy
- decode HTTP into shared `rp-client::api` request types
- call shared `ClientManager` handlers
- translate shared responses into HTTP responses
- optionally serve static wallet assets or a minimal SPA shell

Explicit non-responsibilities:

- peer handshake handling
- custom protocol framing
- sync protocol state
- forwarding local HTTP traffic through `NetworkManager`

## Runtime-side integration boundary

The shared `ClientManager` should not reach directly into runtime internals.

Instead, each runtime should provide a narrow adapter that implements an `rp-client` runtime-facing trait.

Recommended trait shape:

- `get_status()`
- `get_head()`
- `get_sync_status()`
- `get_peers()` or `get_peer_count()`
- `submit_transaction(tx)`
- `request_runtime_signature(request)` for device-owned key paths when enabled

The trait stays transport-agnostic. A standalone CLI can use a remote adapter, while a hosted runtime can use an in-process adapter.

## Runtime command path recommendation

For hosted mode, the runtime-side adapter should communicate with runtime internals using typed local commands rather than by calling P2P managers directly.

Recommended first-cut pattern:

- introduce a dedicated local API command path with typed requests and responses
- use channels plus reply handles or another explicit request-response boundary inside each runtime
- let the local API layer query node and runtime state through that boundary
- let transaction submission flow into the same node-side path used by other local submissions

This keeps the HTTP surface isolated from peer networking and avoids turning `NetworkManager` into a catch-all coordinator.

## Shared API types

`rp-client` should own the client-facing request and response model.

Recommended first-cut API families:

- wallet status and runtime status
- key generation and key listing
- address display
- transfer build requests
- transfer sign requests
- signed transaction submission
- chain head inspection
- sync status inspection
- peer summary inspection

These types should live in shared Rust modules first. HTTP path naming and serialization can sit on top later.

## 4. Core Design Rules

1. `rp-client` remains outside the consensus-critical path.
2. Any logic that can be shared between CLI mode and runtime-hosted mode should live in `rp-client`, not in duplicated runtime glue.
3. Any logic that depends on peer transport, storage replay, sync state, or chain validation must stay in `rp-node` or the runtimes.
4. Runtime-hosted wallet pages are a runtime capability, but the workflow implementation they call should come from `rp-client`.
5. Embedded hosted mode should default to device-owned keys unless a later design explicitly chooses another trust model.
6. Feature flags are preferable to crate proliferation if the only difference is CLI, hosted assets, or runtime-host integration.
7. `NetworkManager` remains a P2P component; it must not become the entrypoint for local wallet or operator HTTP traffic.
8. Runtime-owned HTTP serving and `rp-client`-owned API semantics should be separated cleanly enough that CLI, host-runtime, and embedded-runtime modes all reuse the same workflow code.

## 5. Recommended Delivery Stages

## Stage 0. Freeze The Client Boundary

### Goal

Make the client and runtime responsibilities explicit before large code changes begin.

### Deliverables

- documented client responsibilities versus runtime responsibilities
- agreed list of first-class client workflows
- agreed signer model for standalone mode versus hosted embedded mode
- agreed runtime-facing request and response types for client operations
- agreed runtime topology for hosted mode, including where `ClientHostManager` sits relative to `NodeManager`, `NetworkManager`, and storage
- explicit decision that local API traffic does not transit the P2P network managers

### Required workflow set for the first cut

- status
- keygen
- show address
- build transfer transaction
- sign transfer transaction
- submit signed transaction
- inspect chain head and sync status
- inspect connected peers or peer count

### Exit criteria

- no ambiguity remains about whether `rp-client` talks to runtimes, runs inside runtimes, or both
- the answer is: both, through one shared manager boundary

## Stage 1. Restructure `rp-client` Around A Library Surface

### Goal

Move the current scaffold away from a binary-only shape.

### Tasks

- add `rp-client/src/lib.rs`
- move CLI behavior out of `main.rs` into reusable library code
- introduce `ClientManager` or a similarly named shared application service
- add shared API request and response types in `rp-client/src/api.rs`
- add handler entrypoints that can be called from CLI mode or hosted runtime mode
- keep `main.rs` as argument parsing and output formatting only
- add unit tests around the shared manager using a mock runtime client

### Exit criteria

- the existing `status` and `keygen` flows run through shared library code rather than inline CLI printing

## Stage 2. Wallet And Signer Work

### Goal

Provide real wallet behavior without entangling it with runtime networking internals.

### Tasks

- define a keystore abstraction
- implement an initial local keystore for desktop or CLI mode
- define address derivation and key display flows
- define transaction builder helpers using the canonical `rp-core` transaction types
- define a signer abstraction that can support both local signing and runtime-backed signing later
- define the request and response shapes for key and signing operations so the same workflow can be reused in hosted mode

### Recommended signer modes

- local signer for standalone `rp-client`
- runtime-backed signer for runtime-hosted embedded wallet mode when keys remain on the device

### Exit criteria

- `rp-client` can create and sign a valid transaction in a test harness without direct knowledge of runtime socket or peer code

## Stage 3. Runtime API Contract And Diagnostics

### Goal

Define the client-facing contract that either runtime can expose.

### Tasks

- add shared request and response types for client workflows
- define a `RuntimeClient` trait or equivalent abstraction inside `rp-client`
- implement a first adapter against `rp-runtime`
- define the compatibility target that `erp-runtime` must satisfy
- include diagnostics calls for status, head, sync, and peers
- define the first concrete runtime adapter methods and their ownership boundaries
- define the initial local command path inside the runtime for request-response style status and submission operations

### Recommended first-cut `RuntimeClient` responsibilities

- return runtime or wallet status
- return current head summary
- return sync summary
- return peer summary or count
- accept submitted transactions
- optionally perform runtime-backed signing when the key is intentionally runtime-owned

### Recommended first-cut shared request types

- `GetStatus`
- `GenerateKey`
- `ShowAddress`
- `BuildTransfer`
- `SignTransfer`
- `SubmitTransaction`
- `GetHead`
- `GetSyncStatus`
- `GetPeers`

### Recommended policy

- keep the shared Rust request and response types runtime-agnostic
- let runtime adapters choose HTTP, local IPC, or another transport later
- do not hard-code HTTP or a browser into the core client logic
- do not expose `NetworkManager` methods directly as the client API surface

### Exit criteria

- the same `ClientManager` can drive a mock runtime client and at least one real runtime adapter

## Stage 4. Hosted Mode In Both Runtimes

### Goal

Make the client logic hostable by runtimes rather than only callable from a standalone CLI.

### Tasks

- add optional hosted assets or hosted-route support inside `rp-client`
- add a thin `ClientHostManager` in `rp-runtime` for local hosted mode
- add a thin `ClientHostManager` in `erp-runtime` for embedded hosted mode
- ensure both adapters use the same shared workflow layer from `rp-client`
- define auth and request-size limits appropriate to each runtime
- define the runtime-local command and reply path used by the host managers
- keep HTTP request parsing and response writing inside the runtime host managers, not inside `NetworkManager`

### Recommended implementation split

- `rp-client` provides manager logic, shared DTOs, handler entrypoints, and optional asset manifesting
- `rp-runtime` provides desktop or server HTTP hosting glue, runtime adapters, and local API command plumbing
- `erp-runtime` provides embedded HTTP hosting glue, runtime adapters, and local API command plumbing with tighter resource budgets

### Recommended first-cut module split

- `rp-client::api` defines request and response DTOs
- `rp-client::manager` defines the workflow coordinator
- `rp-client::hosted` defines reusable handler functions or route descriptions, not the server socket itself
- `rp-runtime::client` hosts the desktop HTTP server and implements the runtime adapter
- `erp-runtime::client` hosts the embedded HTTP server and implements the runtime adapter

### Recommended first-cut runtime wiring

For `rp-runtime`:

- spawn `ClientHostManager` alongside the existing node, network, storage, and wake tasks
- add a local API channel pair for typed client requests and replies
- implement the runtime adapter on top of those channels and any read-model state the runtime already owns

For `erp-runtime`:

- add a hosted client task inside the runtime host orchestration, separate from transport session handling
- keep the embedded HTTP surface smaller and bounded
- prefer device-owned signing for sensitive key paths

### Exit criteria

- both runtimes can host the same logical client workflows without duplicating wallet logic

## Stage 5. Hardening, Packaging, And Compatibility

### Goal

Make the client layer fit for the first cross-runtime testnet.

### Tasks

- add integration tests for client-to-runtime workflows
- add version and capability checks on the client-facing runtime API
- define migration rules for local keystore or wallet metadata
- add diagnostics around runtime unavailability and degraded sync state
- document which hosted features are available on host versus embedded targets
- add tests that prove the local HTTP surface remains independent from the P2P `NetworkManager`

### Exit criteria

- `rp-client` can run standalone against a runtime
- `rp-runtime` can host the shared client layer
- `erp-runtime` can host the same client layer in a constrained form

## 6. Concrete File Targets

The first implementation pass should expect to touch at least:

- `rp-client/Cargo.toml`
- `rp-client/src/lib.rs`
- `rp-client/src/main.rs`
- `rp-client/src/api.rs`
- `rp-client/src/manager.rs`
- `rp-client/src/runtime_api.rs`
- `rp-client/src/wallet/`
- `rp-client/src/tx/`
- `rp-client/src/hosted/mod.rs`
- `rp-runtime/src/client/`
- `rp-runtime/src/main.rs`
- `rp-runtime/src/runtime/manager.rs`
- `erp-runtime/src/client/`
- `erp-runtime/src/runtime/host.rs`

The runtime adapter directories may start as a single module each, but the split should remain explicit.

## 7. Sequencing Recommendations

Recommended order:

1. restructure `rp-client` as library plus thin binary
2. land `ClientManager` and the mockable runtime-facing trait
3. land shared API DTOs and handler entrypoints
4. implement local key and transaction flows
5. implement the first real runtime adapter against `rp-runtime`
6. add a dedicated `ClientHostManager` and hosted-mode support in `rp-runtime`
7. add embedded hosted-mode support in `erp-runtime`
8. harden compatibility and versioning

This order keeps the first real consumer on the host side while preserving the embedded hosting goal.

## 8. Phase 4 Exit Criteria

Phase 4 is complete when all of the following are true:

- `rp-client` is a real library plus binary, not just a small CLI scaffold
- the shared wallet and operator workflows live behind a reusable manager boundary
- a standalone client can talk to at least one runtime through that boundary
- the same logical client layer can be hosted by `rp-runtime`
- the architecture and APIs leave a clear path for `erp-runtime` to host the same layer without forking the wallet logic
- neither runtime needs to route local wallet or operator HTTP traffic through its P2P `NetworkManager`

The stronger finish for this phase is:

- both `rp-runtime` and `erp-runtime` host the same client layer with runtime-specific adapters and without duplicating wallet workflow logic