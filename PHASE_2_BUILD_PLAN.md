# rust-proof Phase 2 Build Plan

Date: 2026-04-23  
Status: execution plan for the first runtime-hosting phase after shared-engine extraction

## Status Update: 2026-04-24

`erp-runtime/` has moved beyond pure bring-up and now hosts a first embedded runtime shell around `rp-node`.

Current implemented slices include:

- runtime bootstrap that constructs `NodeEngine`
- runtime event pump and `NodeAction` dispatch
- NVS-backed snapshot and block persistence
- wake scheduling plus a test-only startup heartbeat
- runtime identity selection and signing integration

The largest remaining Phase 2 gap is transport: the embedded network adapter is still mostly scaffolding, and repeatable hardware smoke validation is still required.

## 1. Purpose

Phase 2 begins after the shared-engine split.

Phase 1 produced the first honest shared engines:

- `rp-core` for blockchain rules
- `rp-node` for node behavior and runtime boundaries

Phase 2 is the runtime-hosting phase.

The first priority is not to redesign everything at once. The first priority is to evolve `erp-runtime/` from an embedded bring-up crate into the first real runtime that hosts `rp-node`, while keeping it buildable throughout the transition.

## 2. Current Baseline

These facts are true at the start of Phase 2:

- `rp-core/` and `rp-node/` are now the shared-engine path.
- `erp-runtime/` already passes `cargo check` when built from inside its own directory.
- the current `erp-runtime/` build is still a device bring-up application, not a runtime shell around `rp-node`.
- `erp-runtime/` is intentionally not a member of the root workspace.
- `erp-runtime/Cargo.toml` declares its own local `[workspace]` so ESP-IDF assets stay under `erp-runtime/.embuild/` rather than polluting the repository root.

That means the Phase 2 objective is not "make `erp-runtime` compile for the first time". It already compiles. The objective is to make `erp-runtime` compile and boot as a real embedded host for `rp-node`.

## 3. Phase 2 Objective

Build `erp-runtime` as the first runtime host for `rp-node`.

In practical terms, that means:

- `erp-runtime` owns ESP-specific transport, storage, timing, identity, and recovery code
- `rp-node` remains the canonical node engine
- `erp-runtime` drives `NodeEngine::step` with real runtime events and executes the resulting `NodeAction`s
- the crate remains buildable during every milestone

## 4. Non-Goals For The First Phase 2 Slice

The first Phase 2 slice should not try to finish the whole product.

Out of scope for the first `erp-runtime` build milestone:

- full wallet-web hosting
- archival storage
- desktop runtime rewrite in parallel
- protocol expansion beyond the current message and import path
- production metrics stack
- final OTA or fleet-management workflow

## 5. Design Constraints

1. `rp-node` stays device-agnostic.
2. `erp-runtime` stays target-specific and memory-bounded.
3. Runtime adapters must be thin and explicit.
4. `erp-runtime` should remain a crate-local workspace rather than being forced into the root Cargo workspace.
5. Every milestone must keep `cd erp-runtime && cargo check` green.
6. Buildable is necessary but not sufficient; the runtime must progressively host more of the real node contract.

## 6. Definition Of Done For The First Phase 2 Delivery

Phase 2 for `erp-runtime` is complete when all of the following are true:

- `erp-runtime` depends on `rp-node` and constructs a `NodeEngine`
- startup can initialize runtime services and create the engine cleanly
- the runtime can feed at least these inputs into the node engine:
  - `Tick`
  - `PeerConnected`
  - `PeerDisconnected`
  - `FrameReceived`
  - `StorageLoaded`
- the runtime can execute at least these actions correctly:
  - `SendFrame`
  - `PersistBlock`
  - `PersistSnapshot`
  - `RequestBlocks`
  - `ScheduleWake`
  - `LoadSnapshot`
  - `ReportEvent`
- startup recovery can either:
  - boot from genesis, or
  - load the latest persisted snapshot and continue
- `cd erp-runtime && cargo check` passes
- hardware smoke testing shows the runtime boots and runs its event loop without crashing immediately

## 7. Delivery Order

The order matters.

Do not start by rewriting Wi-Fi, storage, and the app loop all at once. Start by making `erp-runtime` a runtime shell in structure, then replace bring-up code incrementally.

### M3.0 Freeze The Embedded Runtime Boundary

Objective:

Freeze what `erp-runtime` is responsible for and what remains in `rp-node`.

Tasks:

- confirm that `erp-runtime` remains a standalone ESP-local workspace
- confirm that root workspace membership is not required for the embedded crate
- document the runtime-owned adapter surfaces:
  - transport
  - storage
  - clock
  - wake
  - identity
- define the smallest boot path that can drive `NodeEngine`

Exit criteria:

- Phase 2 file accepted
- build command for the embedded crate is explicit and stable
- there is no ambiguity about runtime versus engine ownership

### M3.1 Introduce `rp-node` Into `erp-runtime`

Objective:

Make `erp-runtime` compile while depending on the shared node engine.

Tasks:

- add path dependencies from `erp-runtime/` to `../rp-node` and `../rp-core` as needed
- create a minimal runtime bootstrap module that owns:
  - `NodeEngine`
  - runtime state
  - event queue or event pump
- keep the current Wi-Fi bring-up path only as scaffolding until the node loop exists
- ensure all dependency features stay compatible with the embedded target

Recommended files:

- `erp-runtime/Cargo.toml`
- `erp-runtime/src/main.rs`
- new modules such as:
  - `erp-runtime/src/runtime.rs`
  - `erp-runtime/src/config.rs`

Exit criteria:

- `erp-runtime` compiles with a constructed `NodeEngine`
- the existing bring-up main path is reduced to bootstrapping the runtime shell

### M3.2 Implement Minimal Adapter Skeletons

Objective:

Compile the runtime against the `rp-node` boundary with explicit adapter implementations, even if some are still stubbed.

Tasks:

- implement embedded adapter skeletons for:
  - transport
  - storage
  - clock
  - wake
  - identity
- define bounded buffer sizes and storage limits up front
- choose where peer IDs and node identity come from on device
- decide how timer wake requests map to Embassy or ESP-IDF timing primitives

Recommended files:

- `erp-runtime/src/transport.rs`
- `erp-runtime/src/storage.rs`
- `erp-runtime/src/identity.rs`
- `erp-runtime/src/wake.rs`

Exit criteria:

- adapter modules compile cleanly
- `main.rs` no longer contains runtime logic inline

### M3.3 Wire The Event Loop

Objective:

Turn `erp-runtime` into a real host that drives `NodeEngine::step`.

Tasks:

- create the runtime event pump
- translate runtime events into `NodeInput`
- execute `NodeAction`s by calling embedded adapters
- handle persistence requests asynchronously or in a bounded serialized loop
- connect timer expiry to `Tick`
- map inbound network bytes to `FrameReceived`

Important rule:

`rp-node` decides behavior. `erp-runtime` only translates between hardware or OS events and the node contract.

Exit criteria:

- the runtime can drive a real `step` loop
- the current bring-up loop is replaced by runtime-driven execution

### M3.4 Implement Embedded Recovery And Persistence

Objective:

Make restart behavior honest.

Tasks:

- define where snapshots and blocks live on device
- implement `LoadSnapshot` and `PersistSnapshot`
- implement `PersistBlock`
- define recovery policy for missing or corrupt snapshots
- define what state is durable versus reconstructible
- define retention policy and bounded cleanup rules

Important rule:

The embedded runtime is not required to be archival. It only needs enough persisted state to recover and participate correctly.

Exit criteria:

- startup recovery path exists
- snapshot and block persistence integrate with the node engine contract

### M3.5 Implement Embedded Transport Around Existing Wi-Fi Bring-Up

Objective:

Turn the current connectivity experiments into a runtime transport adapter.

Tasks:

- keep the working Wi-Fi connection path
- add frame send and receive plumbing on top of the embedded transport channel
- define peer discovery or initial static peer configuration for the first milestone
- define framing, retries, and bounded queue behavior
- surface peer lifecycle changes to the node engine

Exit criteria:

- runtime can receive frames and turn them into `NodeInput::FrameReceived`
- runtime can execute `SendFrame`, `BroadcastFrame`, and `RequestBlocks`

### M3.6 Embedded Smoke Validation

Objective:

Prove that `erp-runtime` is no longer just a buildable experiment.

Tasks:

- boot the device successfully
- initialize the runtime shell and node engine
- observe tick scheduling and event loop progress in logs
- perform at least one frame send or receive smoke path
- perform at least one snapshot save or load smoke path

Exit criteria:

- crate still passes `cargo check`
- device boot smoke test is repeatable
- the embedded crate is now a real runtime host for `rp-node`

## 8. Concrete First File Plan

The first edits in Phase 2 should be concentrated in these files:

- `erp-runtime/Cargo.toml`
  - add shared-engine dependencies
  - keep ESP-local workspace behavior intact
- `erp-runtime/src/main.rs`
  - reduce to bootstrapping, not app logic
- `erp-runtime/src/runtime.rs`
  - own engine construction, event loop, and action dispatch
- `erp-runtime/src/transport.rs`
  - frame send and receive adapter
- `erp-runtime/src/storage.rs`
  - bounded snapshot and block persistence
- `erp-runtime/src/identity.rs`
  - node identity and signing bridge
- `erp-runtime/src/wake.rs`
  - timer and scheduled wake bridge

Existing experiment modules such as `button.rs`, `led.rs`, `channel.rs`, and `time.rs` should either be removed, repurposed, or isolated behind explicit bring-up-only code paths once the runtime shell exists.

## 9. Validation Commands

The minimum validation contract for every step in this phase is:

```sh
cd erp-runtime && cargo check
```

Recommended additional checkpoints:

```sh
cd erp-runtime && cargo build
cd erp-runtime && cargo run
```

Use hardware smoke logs as part of Phase 2 validation. Compile success alone is not enough.

## 10. Immediate Next Slice

The first execution slice should be small:

1. add `rp-node` to `erp-runtime/Cargo.toml`
2. create `erp-runtime/src/runtime.rs`
3. construct `NodeEngine` during boot
4. replace the current infinite app loop with a runtime-owned event loop skeleton
5. keep `cargo check` green after each edit

That is the correct first Phase 2 increment because it changes the crate shape before it changes the device behavior.