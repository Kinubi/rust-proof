# rust-proof Roadmap

Date: 2026-04-21  
Status: proposed delivery roadmap for the current workspace

## 1. Purpose

This roadmap turns the current three-part workspace into a coherent product architecture:

- an embedded client in `erp-client/`
- a PC client in `rust-proof-client/`
- a core blockchain engine that must become `no_std` capable

The most important architectural decision in this roadmap is this:

**The blockchain engine should migrate to `no_std`; the host node runtime should not.**

That means networking, storage, JSON-RPC, OS process management, and async orchestration remain in `std` crates. The deterministic state transition engine, consensus rules, transaction and block validation, serialization, and Merkle/state-root logic move into a `no_std`-friendly core.

Trying to force `tokio`, `sled`, `libp2p`, and host process concerns into `no_std` is the wrong target. The right target is a strict separation between:

- deterministic protocol logic
- host integration logic
- client implementations

## 2. Current Baseline In This Repository

As of 2026-04-21, the workspace already has the right high-level product split, but the internal crate boundaries do not yet support a `no_std` core.

### Current workspace shape

- `rust-proof-core/` contains models, state, blockchain logic, mempool, storage, a node actor, libp2p networking, and the main binary
- `rust-proof-client/` is a desktop CLI scaffold with `status` and `keygen` placeholders
- `erp-client/` is an embedded Wi-Fi bring-up and async runtime prototype for ESP32-P4 + ESP32-C6

### Current blockers to a `no_std` core

The current `rust-proof-core/` library mixes deterministic logic with host-only concerns.

- `rust-proof-core/src/state.rs` uses `std::collections::HashMap`
- `rust-proof-core/src/blockchain.rs` couples validation, fork choice, state mutation, mempool, and persistent storage
- `rust-proof-core/src/storage.rs` hard-wires `sled` and `std`
- `rust-proof-core/src/node.rs` depends on `tokio`
- `rust-proof-core/src/network/manager.rs` depends on `libp2p`, `serde_json`, `std::time`, and process-level async handling
- `rust-proof-core/src/main.rs` boots a host runtime, which is inherently `std`

### Practical implication

The current directory named `rust-proof-core/` is really a mixed node crate, not a pure blockchain core crate yet. The roadmap below fixes that first.

## 3. North-Star Architecture

The target architecture should be organized around strict boundaries.

### Product roles

#### Core blockchain engine

Responsibilities:

- canonical transaction and block types
- canonical binary encoding
- signature verification and hashing
- mempool ordering rules
- ledger state transition rules
- validator selection rules
- slashing validation rules
- block validation and state-root computation
- deterministic fork-choice inputs and outputs

Hard constraints:

- `no_std` compatible
- no filesystem access
- no sockets
- no threads
- no async runtime dependency
- no wall-clock reads from the environment
- no JSON as consensus-critical encoding

#### Host node runtime

Responsibilities:

- storage backend
- libp2p networking
- JSON-RPC or other external API
- process lifecycle
- task orchestration
- peer management
- chain sync
- observability and operator tooling

Hard constraints:

- may use `std`
- may use `tokio`, `libp2p`, `sled` or a replacement backend
- must treat the core engine as the source of truth for validation

#### PC client

Responsibilities:

- wallet and key management for desktop workflows
- transaction construction and signing
- RPC interaction with the node
- operator commands and diagnostics
- developer and testnet tooling

Hard constraints:

- should not embed consensus rules that can drift from core
- should consume shared protocol types or generated DTOs

#### Embedded client

Responsibilities:

- device identity
- secure signing or transaction origination
- compact protocol interaction with a node
- light verification where feasible
- device telemetry, connectivity, and retry logic

Hard constraints:

- must respect memory and power budgets
- must not attempt to become a full archival node in the first release
- should begin as a signer and light client before attempting richer peer participation

## 4. Architectural Rules

These rules should govern every milestone.

1. The canonical blockchain engine lives behind a stable library boundary and compiles without `std`.
2. Consensus-critical encoding is binary and deterministic. JSON is only for host-facing APIs.
3. All host runtimes must call into the same validation and state-transition code.
4. The embedded client is a constrained client first, not a miniature server.
5. Every network and storage format must be versioned.
6. The PC client and embedded client must share protocol definitions rather than duplicate them.
7. `serde` support is optional and feature-gated where it touches core types.
8. Every phase must end with explicit acceptance criteria and runnable checks.

## 5. Recommended Target Workspace Shape

The current three product areas stay, but the internal workspace should be expanded so the boundaries are clean.

### Recommended crates

- `rust-proof-core`
  - `no_std` deterministic blockchain engine
  - models, state transition, mempool rules, consensus, hashing, canonical encoding
- `rust-proof-node`
  - `std` host runtime
  - storage, networking, RPC, process orchestration, chain sync
- `rust-proof-protocol`
  - shared external protocol and wire DTOs if needed
  - versioned message envelopes, RPC request/response types, client-safe shared schemas
- `rust-proof-client`
  - desktop CLI and wallet
- `erp-client`
  - embedded client firmware
- `rust-proof-testkit`
  - simulation helpers, fixtures, golden vectors, adversarial tests

### Mapping from the current codebase

The current code should be redistributed roughly like this:

- Move `rust-proof-core/src/models/` into the future `rust-proof-core`
- Move `rust-proof-core/src/traits.rs` into the future `rust-proof-core`
- Move `rust-proof-core/src/state.rs` into the future `rust-proof-core`
- Move `rust-proof-core/src/mempool.rs` into the future `rust-proof-core`
- Split `rust-proof-core/src/blockchain.rs` into a pure engine layer and a host orchestration layer
- Move `rust-proof-core/src/storage.rs` into the future `rust-proof-node` or a dedicated storage crate
- Move `rust-proof-core/src/node.rs` into the future `rust-proof-node`
- Move `rust-proof-core/src/network/` into the future `rust-proof-node`
- Move `rust-proof-core/src/main.rs` into the future `rust-proof-node`

### Naming cleanup

The package name inside `rust-proof-core/Cargo.toml` is currently `rust-proof`, which is ambiguous given the directory name and future split. Early in the roadmap, package names should be normalized so the package graph is self-explanatory.

## 6. Delivery Strategy

This roadmap is sequenced in eight milestones. The milestones are intentionally structured so that protocol stabilization happens before client expansion, and so that the `no_std` migration happens before the embedded client depends on the core engine.

### Summary timeline

| Window | Milestone | Main outcome |
| --- | --- | --- |
| May 2026 | M0 | Architecture freeze and workspace split plan |
| May to June 2026 | M1 | Shared protocol foundation and package cleanup |
| June to July 2026 | M2 | Deterministic core extraction |
| July to August 2026 | M3 | `no_std` migration of the blockchain engine |
| August to September 2026 | M4 | Host node runtime rebuilt on top of the core |
| September to October 2026 | M5 | PC client becomes a real wallet/operator tool |
| October to November 2026 | M6 | Embedded client becomes a real light client and signer |
| November to December 2026 | M7 | End-to-end hardening, testnet, and release prep |

## 7. Detailed Milestones

## M0. Architecture Freeze And Scope Reset

### Objective

Freeze the intended product boundaries before more code is added in the wrong places.

### Why this milestone exists

Right now the repository has the correct top-level idea but the wrong internal separations for a `no_std` core. If this is not fixed first, every new feature will increase the cost of the migration.

### Tasks

- Decide that the blockchain engine, not the full host node, is the `no_std` target.
- Write a short architecture decision record that defines the responsibilities of `core`, `node`, `pc client`, and `embedded client`.
- Decide whether `rust-proof-protocol` is required immediately or can be introduced in M1.
- Decide whether the embedded client is an RPC client, a light client, a signer, or a future validator participant for the first release.
- Define the first release scope explicitly.
- Define what will not be attempted in the first release.

### First-release recommendation

The first release should target this behavior:

- one or more host nodes run the blockchain runtime
- the PC client can create keys, construct transactions, sign them, submit them, and query status
- the embedded client can authenticate, sign payloads or transactions, submit them to a host node, and verify a compact chain view such as headers or checkpoint proofs

### Explicit non-goals for the first release

- full archival storage on the embedded device
- full libp2p swarm participation on the embedded device
- smart contracts
- advanced finality gadgets
- privacy features
- generalized on-device state sync beyond light verification

### Deliverables

- architecture decision note
- crate split plan
- release scope definition
- success metrics for each product component

### Exit criteria

- the team can describe exactly which parts must compile in `no_std`
- the team can list which current source files must move out of the future core crate
- the first release scope fits in a single page without contradictions

## M1. Shared Protocol Foundation And Package Cleanup

### Objective

Create the package boundaries and shared protocol definitions that all three product areas will rely on.

### Tasks for package structure

- create a true `rust-proof-core` library package
- create a `rust-proof-node` host package or equivalent host binary crate
- normalize package names so directory names and package names line up
- keep `erp-client` out of default host workflows, as the workspace already does
- add a `rust-proof-testkit` package if integration and adversarial tests start polluting production crates

### Tasks for shared types

- define stable transaction and block envelope versions
- define canonical public key, signature, hash, and address formats
- define node-to-client request and response schemas
- define host-to-embedded request and response schemas
- define error code conventions that do not depend on free-form strings
- define explicit protocol version negotiation or compatibility rules

### Tasks for encoding

- keep or replace the current `ToBytes` and `FromBytes` framework, but make the decision once
- if keeping the current custom encoding, document field order, discriminants, and canonical endianness
- if replacing it, choose a format that works in `no_std` and does not compromise determinism
- keep JSON serialization behind a host-facing boundary only
- add golden-vector tests for transaction and block encoding

### Tasks for core data model cleanup

- remove implicit assumptions from model structs
- ensure every enum discriminant is stable and documented
- make transaction hashing and block hashing independent from presentation formats
- define whether fees, stakes, slash proofs, and future extensions all live in one envelope version or use tagged extensions

### Deliverables

- clean package graph
- versioned protocol document
- golden test vectors for hashes and encodings
- initial compatibility test between PC client and host node DTOs

### Exit criteria

- protocol structs are not duplicated across clients
- at least one golden-vector test suite exists for serialized transactions and blocks
- the intended `core` crate has no direct dependency on `tokio`, `libp2p`, `sled`, or JSON transport code

## M2. Deterministic Core Extraction

### Objective

Extract the blockchain engine from the mixed runtime code so it becomes a pure deterministic library.

### Core design target

The `core` crate should expose pure or nearly pure operations like:

- validate transaction
- apply transaction
- validate block header
- apply block
- compute state root
- select validator for slot or height
- evaluate slash proof
- return deterministic effects and state deltas

### Concrete refactors from the current codebase

- split `blockchain.rs` into a pure chain engine and host integration layer
- remove direct storage ownership from the core blockchain state machine
- remove direct async or channel concepts from the core API
- remove direct networking concerns from the core API
- replace `String` and `&'static str` errors in consensus-critical paths with compact typed error enums
- isolate host-only glue that currently lives next to state-transition logic

### Specific changes required by current files

- `state.rs`: make data structures independent from `std`
- `blockchain.rs`: stop storing a boxed storage trait inside the engine
- `node.rs`: move actor handling out of the future core
- `storage.rs`: host-only, not part of the future core
- `network/manager.rs`: host-only, not part of the future core

### Data structure strategy

For the future `no_std` core, prefer deterministic and `alloc`-friendly collections:

- `alloc::vec::Vec`
- `alloc::collections::BTreeMap`
- `alloc::collections::BTreeSet`

This is a good fit because it reduces hasher concerns and gives deterministic iteration order, which is useful for state-root computation and stable testing.

### State machine cleanup tasks

- make transaction validation return typed errors with enough context for tests
- make state transitions return explicit effects rather than relying on ambient host state
- define a canonical slot and epoch input model instead of reading time from the environment
- ensure validator selection takes explicit deterministic inputs
- ensure slashing validation is deterministic and side-effect free
- ensure mempool ordering logic is deterministic

### Deliverables

- pure engine API
- typed core error enums
- deterministic state-transition test suite
- no host runtime code inside the engine crate

### Exit criteria

- the engine can be used by a host node and by tests without standing up `tokio`
- the engine can validate and apply blocks using only explicit inputs
- storage is an adapter around the engine, not embedded inside it

## M3. `no_std` Migration Of The Blockchain Engine

### Objective

Make the extracted blockchain engine compile and run in `no_std` environments with `alloc`.

### Migration approach

Use this structure at the crate root:

- `#![cfg_attr(not(feature = "std"), no_std)]`
- `extern crate alloc` where needed
- `std` as an opt-in feature, not a requirement

### Core migration tasks

- replace `std::collections::HashMap` with `BTreeMap` or another `alloc`-friendly map
- replace `std::cmp::Ordering` imports with `core::cmp::Ordering`
- remove `std::fmt::Debug` requirements where they leak into core trait boundaries unnecessarily
- remove any use of filesystem paths, threads, system time, process signals, or sockets from the core crate
- replace string-heavy errors in consensus-critical code with enums and compact payloads
- gate `serde` derives behind a feature if they are still needed for host interoperability

### Serialization tasks

- verify that canonical encoding does not require `std`
- ensure all byte conversion utilities use `alloc` only
- add cross-target golden tests to ensure host and non-host builds produce identical hashes

### Cryptography tasks

- verify that the chosen signing and verification crates work under the intended `no_std` profile
- confirm feature flags for `ed25519-dalek`, hashing crates, and RNG use are correct for host signing, test signing, and embedded verification
- separate deterministic verification from host-only key generation paths

### Testing tasks

- add `cargo check` for `--no-default-features`
- add at least one bare-metal target smoke build in CI for the engine crate
- add property tests for state transitions on the host build
- add golden-vector tests that are shared across host and `no_std` builds

### Acceptance target for this milestone

By the end of M3, a block should be constructible, hashable, validatable, and applicable by the `core` crate in a `no_std + alloc` build, with the same outputs as the host build.

### Exit criteria

- `core` compiles with `--no-default-features`
- `core` has no direct `std` imports in its library code path
- canonical hashes are identical across host and `no_std` test vectors

## M4. Host Node Runtime Rebuilt On Top Of Core

### Objective

Turn the current mixed host logic into a clean runtime that depends on the new engine instead of owning consensus logic itself.

### Runtime responsibilities

- load and persist chain data
- feed transactions and blocks into the engine
- maintain mempool persistence policy if needed
- run libp2p networking
- expose RPC
- coordinate chain sync and peer management
- manage process lifecycle and observability

### Tasks for storage

- move `sled` integration fully out of the core crate
- define storage adapters around engine snapshots, blocks, and indexes
- decide whether `sled` remains the backend or is replaced later
- add a migration strategy for on-disk schema versions
- distinguish canonical chain storage from auxiliary indexes

### Tasks for node orchestration

- rebuild the current actor model so it wraps the engine cleanly
- keep `tokio` and channel handling in the runtime layer only
- ensure the runtime does not bypass the engine for validation
- define internal commands for transaction submission, block import, sync, and queries

### Tasks for networking

- keep `libp2p` in the host node only
- stop using JSON as the internal canonical wire representation for consensus objects if that is still happening in hot paths
- define explicit network message versions
- implement block and transaction gossip against the new engine boundary
- implement chain sync around engine validation and storage persistence

### Tasks for RPC

- define a stable RPC surface for the PC client and embedded client
- provide endpoints for status, head, block lookup, balance, nonce, submit transaction, and submit device-signed payloads if applicable
- make RPC error codes structured and stable
- keep RPC DTOs separate from consensus-internal structs when needed

### Observability tasks

- structured logs for sync, mempool, block import, and peer events
- metrics for block height, mempool size, connected peers, import latency, and failed validations
- trace IDs or request correlation for RPC and sync flows

### Deliverables

- host node binary using the new `core`
- storage adapter layer
- RPC surface usable by the PC client
- network manager using engine validation for imports

### Exit criteria

- the host node can boot, accept a transaction, include it in a block, persist state, and serve the result through RPC
- no host-only dependency is needed to compile the engine crate

## M5. PC Client From Scaffold To Real Operator Tool

### Objective

Turn `rust-proof-client/` from a placeholder CLI into a real desktop client.

### Primary product role

The PC client should become the operator and wallet interface for the system. It is also the easiest place to deliver developer ergonomics quickly.

### Feature set for the first meaningful release

- generate keys
- import and export keys securely
- show account status
- construct transfer, stake, unstake, and slash-proof submission requests if supported
- sign transactions locally
- submit transactions to a node
- query head, balances, nonce, mempool visibility, and recent blocks
- inspect protocol version and node health

### Recommended command groups

- `status`
- `wallet create`
- `wallet import`
- `wallet address`
- `tx build`
- `tx sign`
- `tx submit`
- `chain head`
- `chain block`
- `validator stake`
- `validator unstake`

### Security tasks

- define how desktop keys are stored
- support environment-variable or file-based key injection for development only
- avoid printing secrets in logs
- add confirmation flows for destructive wallet actions

### Integration tasks

- use the shared protocol types or DTOs rather than duplicating structs
- ensure RPC compatibility tests run in CI
- add fixtures so the client can be tested against a local host node automatically

### Deliverables

- usable CLI
- wallet module
- RPC client module
- integration tests against a local node

### Exit criteria

- a developer can create a wallet, fund it in a test environment, submit a signed transaction, and observe final state through the CLI alone

## M6. Embedded Client From Bring-Up To Light Client And Signer

### Objective

Turn `erp-client/` from a networking bring-up project into a constrained, production-shaped blockchain client.

### Important scope rule

The embedded client should not try to implement the full host node feature set in its first meaningful release. The first meaningful release should focus on being a robust device identity, signer, and compact protocol client.

### Stage 1 responsibilities for the embedded client

- establish connectivity reliably
- maintain device identity
- store or access signing material safely
- build or receive compact transaction payloads
- sign transactions or attestations
- submit them to a host node
- query compact chain state such as latest header, checkpoint, or account nonce
- verify compact proofs or signed checkpoints where feasible

### Stage 2 responsibilities after the basics work

- validate block headers or checkpoint chains
- cache minimal account or chain state
- support resumable synchronization after disconnects
- support compact proof verification for balance or inclusion checks

### Explicit anti-goals for early embedded milestones

- full mempool gossip
- full archival chain sync
- running a full libp2p stack if memory and operational cost are not justified
- unrestricted dynamic allocation patterns

### Embedded protocol strategy

Start with the simplest transport that is reliable on the target hardware and software stack.

Recommended order:

- compact request-response protocol over a straightforward transport
- stable binary payloads or compact DTOs
- optional promotion to richer peer interaction later

Do not begin with protocol ambition that outpaces device reliability.

### Firmware tasks

- separate connectivity, protocol, signing, and application logic into explicit tasks or modules
- define bounded buffers for all inbound and outbound message classes
- formalize retry, timeout, and reconnect behavior
- define persistent device configuration and version reporting
- define a firmware-safe error model that can be surfaced over logs or telemetry

### Cryptographic tasks

- decide whether keys live in flash, secure element, or protected partitioning
- keep signing and verification logic compatible with the `core` crate where possible
- use the same canonical transaction hash as the host and PC client

### Validation tasks

- if the embedded device performs light verification, define exactly what is verified
- choose between trusted checkpoint mode, signed-header mode, or proof-based state verification
- make verification bounded and measurable in memory and timing

### Resource-management tasks

- set RAM budgets for buffers, protocol state, and temporary cryptographic working memory
- set flash budgets for firmware, configuration, and optional persistent checkpoints
- define acceptable reconnect latency and steady-state power expectations

### Deliverables

- embedded device can authenticate to a node
- embedded device can submit a signed payload or transaction
- embedded device can query compact chain state
- embedded device can survive disconnects without corrupting its local protocol state

### Exit criteria

- an end-to-end demo exists where the embedded device signs and submits to a host node and the result can be observed from the PC client
- memory and timing measurements are captured and documented for the critical path

## M7. End-To-End Hardening, Testnet, And Release Preparation

### Objective

Move from “it works in demos” to “it behaves predictably under failure and version churn.”

### Testnet goals

- multi-node host network
- PC client against a real node, not mocks only
- embedded device against a real node over unstable connectivity
- deterministic replay of protocol test vectors

### Failure-mode testing

- malformed transactions
- orphan blocks
- duplicate blocks
- double-sign slash proofs
- incompatible protocol versions
- disconnected embedded client recovery
- storage restart and recovery on host node restart

### Performance and resource testing

- mempool growth behavior
- block import latency
- sync latency
- storage growth and compaction behavior
- embedded memory high-water marks
- embedded reconnect timing

### Security and correctness tasks

- fuzz canonical decoders
- fuzz transaction and block validation entry points
- add property tests for state transitions
- verify signature and hash test vectors from independent fixtures
- run compatibility tests across crate versions before each release cut

### Release engineering tasks

- define semantic versioning policy
- define protocol version compatibility policy
- define storage schema migration policy
- build CI matrices for host and embedded targets
- document the local dev flow and the minimal testnet flow

### Deliverables

- repeatable testnet setup
- compatibility matrix
- CI coverage for core, node, PC client, and embedded firmware builds
- release checklist

### Exit criteria

- the system survives restart, disconnect, malformed-input, and version-mismatch tests without undefined behavior
- the system has a documented upgrade and compatibility story

## 8. Workstreams Across All Milestones

These workstreams cut across the milestone schedule and should be tracked continuously.

## A. Consensus And State Correctness

- formalize validator selection inputs and invariants
- formalize epoch and slot semantics
- formalize slash-proof formats and validation rules
- document fork-choice behavior precisely
- make all consensus-critical functions deterministic and replayable

## B. Canonical Encoding And Versioning

- freeze field ordering and discriminants
- version every external message envelope
- add test vectors for hash stability
- define deprecation rules for old protocol versions

## C. Error Model Cleanup

- replace free-form strings in core paths with enums
- map internal errors to stable RPC and embedded-facing error codes
- preserve enough structured detail for debugging without coupling clients to internal strings

## D. Storage Strategy

- separate canonical state from caches and indexes
- define snapshot policy
- define replay and recovery flows
- define schema versioning and migrations

## E. Security And Key Management

- align transaction hashing across all clients
- document signing domain separation if introduced
- define secure handling for desktop and embedded keys
- prevent test-only key flows from leaking into production defaults

## F. Test Infrastructure

- shared fixtures for transactions, blocks, and slash proofs
- deterministic chain replay tests
- host integration tests
- embedded protocol loopback tests
- compatibility tests across protocol versions

## G. Documentation

- architecture docs for crate boundaries
- protocol docs for wire formats
- operator docs for node setup
- developer docs for local end-to-end workflows
- embedded deployment docs for firmware flashing, configuration, and diagnostics

## 9. Concrete Technical Decisions To Make Early

The following decisions should be made early because they affect all later work.

### Decision 1: collection strategy in `no_std`

Recommendation: use `BTreeMap` and `BTreeSet` in the engine unless performance data proves they are insufficient.

Reason:

- deterministic ordering
- fewer hashing concerns
- straightforward `alloc` support

### Decision 2: canonical encoding strategy

Recommendation: keep a dedicated canonical binary encoding path for consensus objects. Do not allow JSON to become consensus-critical.

### Decision 3: embedded transport strategy

Recommendation: start with a compact request-response protocol over the simplest reliable transport. Do not start with full embedded libp2p unless measurements justify it.

### Decision 4: package split timing

Recommendation: do the package split before significant new feature work. The longer the current mixed crate persists, the more expensive the migration becomes.

### Decision 5: host storage backend

Recommendation: keep `sled` only if it remains sufficient after schema, recovery, and operational requirements are defined. Avoid making the core engine aware of the backend either way.

## 10. CI And Verification Plan

The roadmap is only credible if every milestone lands with checks.

### Required host checks

- workspace `cargo check`
- workspace tests for host crates
- clippy for host crates
- golden-vector tests for protocol data
- integration tests for node plus PC client

### Required core checks

- `cargo check` with `--no-default-features`
- at least one `no_std` target smoke build
- property tests on host for state transitions
- serialization compatibility tests

### Required embedded checks

- firmware builds in CI
- protocol codec tests that run on host
- bounded-memory tests where practical
- reconnect and timeout behavior tests in harnesses or simulation where possible

### Suggested milestone-gated checks

- M1 gate: protocol vectors stable
- M3 gate: `core` compiles without `std`
- M4 gate: host node imports and persists blocks through the new engine
- M5 gate: PC client can submit and query end-to-end
- M6 gate: embedded client can sign, submit, and verify compact state end-to-end
- M7 gate: compatibility and recovery tests pass

## 11. Risks And Mitigations

### Risk: the `no_std` migration is attempted too late

Mitigation:

- make M2 and M3 the priority before major feature growth

### Risk: the embedded client inherits server-side complexity

Mitigation:

- keep the embedded role narrow at first
- require measured justification for every new embedded feature

### Risk: protocol structs drift across products

Mitigation:

- centralize shared schemas and test vectors

### Risk: consensus logic remains coupled to storage and networking

Mitigation:

- enforce crate boundaries and typed interfaces early

### Risk: testing stays host-only

Mitigation:

- require at least one `no_std` smoke build and embedded protocol validation in CI

## 12. Definition Of Done For Version 0.1

Version `0.1` should not mean “all long-term ambitions are complete.” It should mean the architecture is correct and the system works across all three product surfaces.

### Version 0.1 should include

- a `no_std`-capable blockchain engine
- a host node that uses that engine for validation and state transition
- a PC client that can act as a usable wallet and operator CLI
- an embedded client that can authenticate, sign, submit, and verify a compact chain view
- protocol versioning and golden-vector tests
- CI coverage for host, core, and embedded build paths

### Version 0.1 should not require

- embedded full-node behavior
- advanced smart contract execution
- production-grade privacy systems
- advanced interoperability or governance systems

## 13. Recommended Execution Order If Time Is Tight

If the roadmap has to be compressed, follow this order and do not reorder it:

1. split the mixed crate boundaries
2. freeze protocol and canonical encoding
3. extract the deterministic engine
4. make the engine `no_std`
5. rebuild the host node on top of the engine
6. finish the PC client against stable APIs
7. finish the embedded client against stable APIs
8. harden, test, and version

## 14. Final Recommendation

The highest-leverage move for this repository is not adding new blockchain features immediately. It is turning the current mixed `rust-proof-core/` crate into a true layered system where:

- the engine is small, deterministic, and `no_std`
- the host node owns networking, storage, and RPC
- the PC client and embedded client both integrate against stable shared protocol boundaries

Once that separation exists, the roadmap becomes much easier to execute, the embedded client becomes realistic, and the system can evolve without forcing every component to carry every dependency.