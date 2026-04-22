# Phase 1 Build Plan

Date: 2026-04-22  
Scope: first execution phase for the five-crate architecture

## Goal

Move the repository from a mixed host-centric shape toward this target:

- `rp-core`
- `rp-node`
- `rp-runtime`
- `erp-runtime`
- `rp-client`

Phase 1 is about creating honest boundaries and making the shared engines truly `no_std + alloc`.
It is not about rewriting runtimes or finishing the full implementation.

## What Phase 1 Must Achieve

By the end of Phase 1, the project should have:

- frozen docs and naming for the new architecture
- a clear split between blockchain engine and node engine responsibilities
- a clear split between shared node behavior and runtime-specific code
- an explicit constraint that `rp-core` and `rp-node` remain `no_std + alloc` shared engines
- a clear position for the wallet as a separate application layer

## Working Decisions

These are fixed for Phase 1.

1. `rp-core` is the blockchain engine.
2. `rp-node` is the shared node engine.
3. `rp-runtime` and `erp-runtime` are runtime shells around `rp-node`.
4. `rp-client` is the wallet and operator application.
5. The wallet may later ship web assets, but wallet UX is not part of the node engine.

## Shared Engine Constraint

The Phase 1 split must preserve the target execution model of the shared engines:

- `rp-core` stays `no_std + alloc`
- `rp-node` stays `no_std + alloc`
- transport, storage drivers, async runtimes, process lifecycle, and target-specific integration stay in `rp-runtime/` or `erp-runtime/`

If a Phase 1 boundary decision would force `std` into `rp-core` or `rp-node`, that is a sign the boundary is wrong.

## Definition Of Done For Phase 1

Phase 1 is done when all of the following are true:

- the architecture docs all agree on the five-crate model
- the current mixed crate has a concrete extraction plan for `rp-core` and `rp-node`
- the shared-engine path no longer depends on transport, storage drivers, async runtimes, or process lifecycle
- `rp-core` and `rp-node` have a concrete `no_std + alloc` conversion path and first implementation slices
- the runtime rewrites and wallet/application follow-on work are explicitly deferred to Phase 2

## Phase 1 Execution Order

## Step 0. Freeze The Architecture In Docs

### Outcome

- system design doc
- updated roadmap
- updated root and crate READMEs

This step prevents implementation churn from dragging the project back to the older host-node plus embedded-client model.

## Step 1. Reduce The Current Mixed `rp-core/` Crate

### Objective

Identify and separate what belongs to `rp-core` versus what belongs to `rp-node`.

### Questions to answer explicitly

- which modules are purely blockchain-engine concerns?
- which modules are actually node-engine concerns?
- which modules are runtime concerns and must leave the shared engine layer entirely?

### Immediate code surfaces to inspect

- `src/blockchain.rs`
- `src/state.rs`
- `src/mempool.rs`
- `src/storage.rs`
- `src/node.rs`
- `src/network/`
- `src/main.rs`

## Step 2. Define The `rp-node` Contract

### Objective

Write down the actual runtime boundary before moving large amounts of code.

### Deliverables

- list of input events into the node engine
- list of output actions from the node engine
- first draft of transport, storage, clock, wake, and identity boundaries

### Why this matters

Without this step, the runtime split will collapse into ad hoc callbacks and duplicated logic.

## Step 3. Convert The Shared Engines To `no_std + alloc`

### Objective

Remove `std` assumptions from the future shared-engine path before any runtime rewrite begins.

### Deliverables

- identify the `std`-bound APIs and dependencies still leaking into `rp-core` and `rp-node`
- replace shared-engine assumptions with `alloc`-friendly data structures and boundaries where required
- push host and embedded integration details back behind runtime traits or event boundaries
- define the first compile and validation path for shared-engine code that must stay runtime-agnostic

### Why this matters

Without this step, runtime work will drag host assumptions back into the shared engines and make the split dishonest.

## Step 4. Create The First Extraction Plan

### Objective

After the role boundaries and `no_std` constraints are clear, identify the first safe code movement slices.

### Recommended slices

1. move `main.rs` out of the future engine path
2. isolate storage adapters from blockchain logic
3. isolate peer and sync logic from runtime glue
4. remove `std`-bound shared-engine assumptions as each slice moves

## Deferred To Phase 2

The following work is intentionally not part of Phase 1:

- rewrite `erp-runtime/` around the shared node engine
- rewrite `rp-runtime/` around the shared node engine
- evolve `rp-client/` beyond role-definition and interface planning
- compatibility, hardening, and later application-layer work

## The First Real Coding Task After Phase 1 Docs

Once the docs are accepted, the highest-leverage coding task is:

**Split the current `rp-core/` code into blockchain-engine concerns and node-engine concerns, then remove `std` dependencies from the future shared-engine path before doing any runtime-specific feature work.**

That is the move that makes every later crate boundary honest.

## Things Not To Do In Phase 1

- do not add more host-only logic to the mixed crate
- do not rewrite `erp-runtime/` yet
- do not rewrite `rp-runtime/` yet
- do not make embedded networking decisions the definition of the node engine
- do not move wallet UX into the node engines
- do not evolve `rp-client/` beyond role and interface planning yet
- do not bind `rp-node` to a specific transport framework too early

## Phase 1 Success Signal

You know Phase 1 has worked when a newcomer can read the docs and immediately understand:

- what `rp-core` does
- what `rp-node` does
- why `rp-runtime` and `erp-runtime` both exist
- why `rp-client` is not the node
- why the embedded side is a first-class peer runtime
