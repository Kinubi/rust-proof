# Phase 1 Build Plan

Date: 2026-04-22  
Scope: the next coding phase after `ROADMAP.md`

## Goal

Get the workspace from its current mixed state to a clean starting point for the real `no_std` migration.

By the end of this phase, you should have:

- a real `rust-proof-core` library package that contains protocol and engine logic only
- a new `rust-proof-node` host package that owns runtime concerns
- the current binary boot path moved out of the core package
- the first pure engine boundary extracted so the later `no_std` work is mechanical instead of architectural

This phase is not the `no_std` migration itself. It is the refactor that makes the `no_std` migration possible.

## What You Should Build First

Build the host/runtime split first.

Do not start by changing collections to `BTreeMap`, removing `std`, or rewriting embedded code. If you do that before the package and responsibility boundaries are clean, you will create churn in the wrong places.

The correct first coding target is:

**Create a separate `rust-proof-node` crate and make the current package layout honest.**

## Working Decisions For This Phase

These decisions are treated as fixed for the purpose of implementation.

1. Only the deterministic blockchain engine is the `no_std` target.
2. Networking, storage, RPC, async orchestration, and process lifecycle remain `std`-only.
3. The embedded client is a signer and light client first, not a full peer in the first release.
4. The `rust-proof-protocol` crate can be deferred until after the node split if that keeps the first refactor smaller.
5. The first build phase should optimize for correct boundaries, not feature growth.

## Definition Of Done For Phase 1

Phase 1 is done when all of the following are true:

- the workspace has a `rust-proof-node` crate
- `rust-proof-core` is a library-first crate with no binary entry point
- `src/main.rs`, actor orchestration, storage integration, and networking are not part of the core package anymore
- the remaining core code is small enough to begin deterministic engine extraction without touching host runtime code
- host builds still work for the node and PC client

## The Build Order

Implement this phase in the exact order below.

## Step 0. Create The ADR Before Code Moves

### Why

This prevents package churn and naming churn during the refactor.

### Deliverable

Create a short architecture decision record that states:

- `rust-proof-core` is the deterministic engine crate
- `rust-proof-node` is the host runtime crate
- `rust-proof-client` is the desktop client crate
- `erp-client` is the embedded client crate
- `no_std` applies only to the engine

### Suggested file

- `docs/adr/0001-core-node-boundary.md`

### Acceptance check

- one page or less
- no ambiguity about which files belong in which crate

## Step 1. Normalize Package Identity

### Why

The current package name in `rust-proof-core/Cargo.toml` is `rust-proof`, which will become confusing as soon as you add `rust-proof-node`.

### Changes

- rename the package in `rust-proof-core/Cargo.toml` from `rust-proof` to `rust-proof-core`
- keep the directory name as `rust-proof-core`
- update imports from `rust_proof::...` to `rust_proof_core::...`
- keep the workspace member path unchanged

### Files likely touched

- `Cargo.toml`
- `rust-proof-core/Cargo.toml`
- `rust-proof-core/src/main.rs`
- `rust-proof-client` only if it imports the old crate name later
- tests inside `rust-proof-core/`

### Acceptance check

- `cargo check -p rust-proof-core`
- no ambiguous package naming remains in the workspace

## Step 2. Add A New `rust-proof-node` Crate

### Why

You need a place for host-only code before you can move anything out of core.

### Changes

- create `rust-proof-node/Cargo.toml`
- create `rust-proof-node/src/main.rs`
- add `rust-proof-node` to the workspace members and default members
- give `rust-proof-node` dependencies on:
  - `anyhow`
  - `tokio`
  - `rust-proof-core`
  - later, host runtime dependencies as code moves in

### Initial implementation

Start with the smallest possible binary:

- boot the runtime
- print startup info
- call into the existing library where needed

Do not move networking or storage in the same commit that creates the crate.

### Acceptance check

- `cargo check -p rust-proof-node`
- `cargo run -p rust-proof-node` boots a minimal process

## Step 3. Move The Current Binary Entry Point Out Of Core

### Why

The core package cannot keep a host runtime `main.rs` if it is supposed to become a pure engine crate.

### Changes

- move the logic from `rust-proof-core/src/main.rs` into `rust-proof-node/src/main.rs`
- remove the binary target from the core package
- keep the core package as a library only

### Files likely touched

- `rust-proof-core/src/main.rs`
- `rust-proof-node/src/main.rs`
- `rust-proof-core/Cargo.toml`

### Acceptance check

- `cargo check -p rust-proof-core`
- `cargo check -p rust-proof-node`
- `cargo run -p rust-proof-node`

## Step 4. Move Actor Orchestration To `rust-proof-node`

### Why

`tokio::sync`, channels, and command handling are runtime concerns, not engine concerns.

### Current source to move

- `rust-proof-core/src/node.rs`

### Refactor strategy

Move the actor shell first, even if it still calls mixed engine code under the hood.

That means:

- `Node`
- `NodeCommand`
- `tokio` channel wiring
- request and response orchestration

stay in `rust-proof-node`

The temporary compromise is acceptable:

- `rust-proof-node` may initially depend on engine functions that are not fully pure yet
- the point of this step is to remove async orchestration from the future core crate

### Acceptance check

- node crate owns the runtime loop
- core crate compiles without `tokio` imports in its public library path

## Step 5. Split The Current `blockchain.rs` Into Engine And Host Responsibilities

### Why

This is the real inflection point. Right now the current blockchain type mixes:

- chain state
- mempool
- validation
- fork choice
- storage persistence

That prevents both package separation and later `no_std` work.

### Target split

Introduce a pure engine type with responsibilities limited to:

- transaction validation
- block validation
- state application
- state-root computation
- fork-choice evaluation inputs and outputs

Move out of the engine:

- storage ownership
- persistence side effects
- node command handling
- network import flow

### Recommended implementation pattern

Replace this style:

- `Blockchain::new(storage: Box<dyn Storage>)`

with this style:

- `ChainEngine::new(genesis_state)`
- `ChainEngine::apply_block(...) -> EngineResult`
- runtime code persists blocks and snapshots after successful application

### Current files involved

- `rust-proof-core/src/blockchain.rs`
- `rust-proof-core/src/state.rs`
- `rust-proof-core/src/mempool.rs`
- `rust-proof-core/src/models/*`

### Acceptance check

- the engine can validate and apply a block without directly touching a storage backend
- persistence is triggered from runtime code after engine success

## Step 6. Move Storage Out Of Core

### Why

As long as `sled` lives in the core package, the package split is still lying.

### Current source to move

- `rust-proof-core/src/storage.rs`

### Refactor strategy

Do not move storage until Step 5 is complete enough that the engine does not own storage.

Once that is true:

- move `Storage` and `SledStorage` into `rust-proof-node`
- keep persistence adapters around the engine, not inside it
- let the node runtime decide when to save blocks and state snapshots

### Acceptance check

- `rust-proof-core` no longer depends on `sled`
- state snapshots and blocks are still persisted from node code

## Step 7. Move Networking Out Of Core

### Why

`libp2p`, wire I/O, peer discovery, and sync orchestration are host runtime responsibilities.

### Current source to move

- `rust-proof-core/src/network/`

### Refactor strategy

- move network manager and message handling into `rust-proof-node`
- make imported blocks and transactions go through the runtime boundary into the engine
- keep protocol DTOs separate from engine internals if that boundary starts to get muddy

### Acceptance check

- `rust-proof-core` has no direct `libp2p` dependency
- host networking imports transactions and blocks through explicit engine calls

## Step 8. Introduce Typed Core Errors Before `no_std`

### Why

The current engine path uses `String` and `&'static str` in consensus-sensitive logic. That becomes painful during the `no_std` migration and weakens testability now.

### Changes

- introduce `CoreError`, `TxValidationError`, `BlockValidationError`, or similar enums
- use typed results in transaction validation, block validation, and slash validation
- map those enums to RPC or user-facing errors in the node or client crates, not in core

### Acceptance check

- core validation paths return enums, not free-form strings
- tests assert on structured error variants

## Step 9. Prepare The Core For `no_std`, But Do Not Finish It Yet

### Why

Once the runtime split is done, the remaining work becomes much more mechanical.

### Changes

- identify remaining `std` imports in the core crate
- plan collection replacements from `HashMap` to `BTreeMap`
- plan `alloc` imports for vectors and maps
- move `std::cmp::Ordering` imports to `core::cmp::Ordering`
- gate `serde` where necessary

### Important constraint

Do not combine this step with major runtime moves. Keep it as a separate slice.

### Acceptance check

- you can enumerate the exact remaining blockers to `#![no_std]`
- every blocker lives inside the core crate only

## The First Three Commits To Make

If you want the most pragmatic starting sequence, do these first:

### Commit 1

- add the ADR
- rename the core package to `rust-proof-core`

### Commit 2

- create `rust-proof-node`
- add a minimal `main.rs`
- wire it into the workspace

### Commit 3

- move the current runtime entry point from `rust-proof-core/src/main.rs` into `rust-proof-node/src/main.rs`
- keep behavior the same

After those three commits, the workspace shape will stop fighting you.

## Suggested Commands After Each Slice

Run these checks after each step, not only at the end.

### Package and workspace checks

- `cargo check -p rust-proof-core`
- `cargo check -p rust-proof-node`
- `cargo check -p rust-proof-client`
- `cargo check`

### Core behavior checks

- `cargo test -p rust-proof-core`

### Client sanity checks

- `cargo run -p rust-proof-client -- status`

### Embedded caution

Do not pull `erp-client` into every host refactor check. Keep the current workspace behavior where the embedded toolchain stays outside normal desktop validation.

## Things You Should Explicitly Not Do In This Phase

- do not add smart contracts
- do not expand embedded P2P ambitions
- do not redesign consensus rules at the same time as the split
- do not rewrite serialization and package layout in the same commit
- do not attempt the full `no_std` migration before runtime code is out of the core package

## Recommended Task Breakdown For Your Board

Use these as actual tasks in your tracker.

1. Write ADR for core/node/client boundaries.
2. Rename `rust-proof-core` package and update imports.
3. Create `rust-proof-node` crate and workspace wiring.
4. Move binary entry point into `rust-proof-node`.
5. Move actor runtime shell into `rust-proof-node`.
6. Extract pure engine API from `blockchain.rs`.
7. Move storage adapter into `rust-proof-node`.
8. Move libp2p networking into `rust-proof-node`.
9. Replace string errors in core with typed enums.
10. List and isolate remaining `no_std` blockers.

## The Single Next Task

If you want the immediate next coding task with the best leverage, start here:

**Rename the current `rust-proof-core` package, add a new `rust-proof-node` crate, and move the runtime `main.rs` into it without changing behavior.**

That is the smallest change that improves the architecture immediately and unlocks the rest of the plan.