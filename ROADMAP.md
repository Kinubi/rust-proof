# rust-proof Roadmap

Date: 2026-04-22  
Status: active target roadmap for the five-crate architecture

## 1. Purpose

This roadmap moves the repository from its current mixed state to the following target architecture:

- `rp-core`
- `rp-node`
- `rp-runtime`
- `erp-runtime`
- `rp-client`

The key architectural commitments are:

1. the blockchain engine is shared once in `rp-core`
2. the node engine is shared once in `rp-node`
3. desktop and embedded peers are distinguished by runtime adapters, not by duplicated node logic
4. the wallet remains a separate application layer in `rp-client`

## 2. Target Architecture

## `rp-core`

The blockchain engine.

- `no_std + alloc`
- canonical models, validation, state transition, hashing, encoding, fork-choice inputs and outputs

## `rp-node`

The node engine.

- `no_std + alloc`
- peer state, sync logic, mempool policy, message handling, import pipeline, capability negotiation
- defines traits or event boundaries for transport, storage, clock, wake, and identity

## `rp-runtime`

The desktop or server runtime.

- `std`
- hosts `rp-node`
- integrates transport, storage, timers, observability, process lifecycle, and optional local APIs

## `erp-runtime`

The embedded runtime.

- target-specific
- hosts `rp-node`
- integrates constrained transport, storage, timers, identity, bounded memory policy, and optional local web hosting

## `rp-client`

The wallet and operator application.

- `std`
- CLI, wallet UX, transaction construction, transaction submission, diagnostics
- may also produce static wallet web assets that a runtime can host

## 3. Current Repository Reality

The repository is not fully migrated yet at the code-boundary level.

Current mapping:

- `rp-core/` is still a transitional mixed crate and needs to be split internally into `rp-core` and `rp-node` boundaries
- `rp-node/` exists as the shared node-engine crate skeleton
- `rp-runtime/` exists as the host runtime shell crate skeleton
- `rp-client/` is the wallet application scaffold
- `erp-runtime/` is the embedded runtime scaffold

This roadmap describes the target model, not the current implementation state.

## 4. Design Rules

1. Consensus logic belongs only in `rp-core`.
2. Node behavior belongs only in `rp-node`.
3. Runtime adapters stay thin.
4. Desktop and embedded peers share the same node engine.
5. `rp-client` is not consensus-critical.
6. The embedded runtime is a first-class peer runtime.
7. `no_std` is a property of the shared engines, not necessarily of every runtime shell.
8. Wallet web hosting, if added, is a runtime capability, not a node-engine concern.

## 5. Delivery Strategy

The roadmap is organized as seven milestones across two execution phases.

- Phase 1: M0 through M2, plus the separation and `no_std + alloc` conversion work needed to make `rp-core` and `rp-node` honest shared engines
- Phase 2: M3 through M6, starting with the `erp-runtime/` and `rp-runtime/` rewrites and continuing through `rp-client` and compatibility work

| Window | Milestone | Main outcome |
| --- | --- | --- |
| Late April 2026 | M0 | Architecture freeze and naming freeze |
| May 2026 | M1 | Extract `rp-core` and define `rp-node` boundaries |
| May to June 2026 | M2 | Implement the shared `rp-node` engine boundary |
| June to July 2026 | M3 | Build `erp-runtime` around the node engine |
| July to August 2026 | M4 | Build `rp-runtime` around the same node engine |
| August to September 2026 | M5 | Build `rp-client` wallet and operator flows |
| September to October 2026 | M6 | Compatibility, hardening, and end-to-end testnet |

## 6. Detailed Milestones

## M0. Architecture Freeze And Naming Freeze

### Objective

Freeze the final crate model and stop further design drift.

### Deliverables

- accepted architecture docs
- clear mapping from current directories to target crates
- explicit agreement that `rp-client` is a wallet app and both desktop and embedded runtimes can host nodes

### Exit criteria

- all major docs describe the same five-crate model
- there is no longer any architectural ambiguity about whether the ESP is a client or a peer runtime

## M1. Extract `rp-core` And Define `rp-node`

### Objective

Separate blockchain rules from node behavior inside the current mixed codebase.

### Tasks

- isolate blockchain engine concerns inside the current `rp-core/` tree
- define what belongs in the future `rp-node`
- identify all runtime-coupled code paths in the current mixed crate
- identify and remove `std`-bound assumptions from the future shared-engine surfaces
- move validation and state transition logic behind a smaller engine boundary
- remove direct storage ownership from engine logic

### Current files that must be split or reduced

- `rp-core/src/blockchain.rs`
- `rp-core/src/state.rs`
- `rp-core/src/mempool.rs`
- `rp-core/src/storage.rs`
- `rp-core/src/node.rs`
- `rp-core/src/network/`
- `rp-core/src/main.rs`

### Exit criteria

- the future `rp-core` surface is identifiable and mostly detached from runtime concerns
- the future `rp-node` boundary is documented and testable

## M2. Implement The Shared `rp-node` Boundary

### Objective

Create the device-agnostic node engine boundary.

### Tasks

- define the node event model
- define the node output and action model
- define runtime-facing boundaries for transport, storage, clock, wake, and identity
- move peer state, sync state, import orchestration, and mempool admission into the node engine layer
- convert shared-engine paths to `no_std + alloc` friendly abstractions where needed
- keep concrete runtime integration out of the shared node engine

### Recommended design style

The node engine should behave like a pure state machine.

Inputs:

- peer events
- local transaction submission
- timer events
- storage load results
- inbound protocol frames

Outputs:

- send or broadcast actions
- persist actions
- wake or scheduling actions
- disconnect or peer-management actions
- structured engine events for observability

### Exit criteria

- `rp-node` can be reasoned about without reference to OS sockets or ESP drivers
- the node engine can drive both a host runtime and an embedded runtime in principle

The remaining milestones are Phase 2 work. Runtime rewrites begin here.

## M3. Build `erp-runtime`

### Objective

Create the embedded runtime that hosts the same `rp-node`.

### Tasks

- evolve `erp-runtime/` into the real embedded runtime
- implement the transport boundary for the embedded network stack
- implement bounded storage behavior suitable for the device
- implement timer and wake integration
- define restart recovery rules
- define capability profiles for embedded peers

### Important design principle

The embedded runtime is a first-class peer runtime, but it is not required to be archival.

### Exit criteria

- the embedded runtime can host a real peer using the same node engine as the desktop runtime

## M4. Build `rp-runtime`

### Objective

Create the desktop or server runtime that hosts `rp-node`.

### Tasks

- evolve `rp-runtime/` from a crate skeleton into the real host runtime
- wire host transport integration into the node engine boundary
- wire host storage integration into the node engine boundary
- provide process lifecycle, logging, and metrics
- optionally add local control APIs and local wallet web hosting later

### Runtime policy

- runtime-specific code must stay out of `rp-node`
- runtime should be replaceable without changing node logic

### Exit criteria

- a desktop or server process can host a peer node through the shared node engine

## M5. Build `rp-client`

### Objective

Turn the current desktop client scaffold into the wallet and operator application.

### Tasks

- evolve `rp-client/` into the full wallet and operator application
- support key management
- support transaction construction and signing
- support transaction submission and node inspection
- support operator diagnostics
- optionally support a web-wallet build target whose assets can be hosted by a runtime

### Important scope rule

`rp-client` is not the canonical node. It talks to or cooperates with runtimes but does not replace the shared node engine.

### Exit criteria

- the wallet app can operate against either desktop or embedded runtimes

## M6. Compatibility, Hardening, And Testnet

### Objective

Prove the architecture works end to end.

### Tasks

- cross-runtime compatibility tests between `rp-runtime` and `erp-runtime`
- protocol version and capability tests
- restart and recovery tests
- malformed frame and invalid block tests
- storage resilience tests
- wallet-to-runtime integration tests

### Exit criteria

- one desktop runtime and one embedded runtime can participate in the same network using the same node engine
- the wallet app can interact with either runtime surface as designed

## 7. Cross-Cutting Workstreams

## A. Protocol Design

- handshake and capability exchange
- transaction and block framing
- sync request and response shapes
- max frame sizing
- version negotiation

## B. Runtime Boundary Design

- transport boundary
- storage boundary
- clock and wake boundary
- identity or signer boundary
- observability boundary

## C. Storage Policy

- archival or full support in `rp-runtime`
- bounded or pruned support in `erp-runtime`
- safe restart and replay behavior in both runtimes

## D. Wallet Surface

- CLI wallet behavior
- optional web-wallet packaging
- local hosting by runtime when required
- secure signing boundaries

## 8. Version 0.1 Definition Of Done

Version `0.1` should include:

- a real `rp-core`
- a real `rp-node`
- a desktop runtime hosting a peer
- an embedded runtime hosting a peer
- a wallet application
- proof that the same node engine works across both runtimes

Version `0.1` should not require:

- identical storage footprints across runtimes
- archival storage on the embedded device
- a separate wallet-web crate
- runtime-specific forks of the node engine
