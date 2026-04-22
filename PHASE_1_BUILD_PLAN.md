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

Phase 1 is about creating honest boundaries. It is not about finishing the full implementation.

## What Phase 1 Must Achieve

By the end of Phase 1, the project should have:

- frozen docs and naming for the new architecture
- a clear split between blockchain engine and node engine responsibilities
- a clear split between shared node behavior and runtime-specific code
- a clear position for the wallet as a separate application layer

## Working Decisions

These are fixed for Phase 1.

1. `rp-core` is the blockchain engine.
2. `rp-node` is the shared node engine.
3. `rp-runtime` and `erp-runtime` are runtime shells around `rp-node`.
4. `rp-client` is the wallet and operator application.
5. The wallet may later ship web assets, but wallet UX is not part of the node engine.

## Definition Of Done For Phase 1

Phase 1 is done when all of the following are true:

- the architecture docs all agree on the five-crate model
- the current mixed crate has a concrete extraction plan for `rp-core` and `rp-node`
- the placeholder desktop runtime has a clear target role as `rp-runtime`
- the embedded runtime has a clear target role as `erp-runtime`
- the wallet crate has a clear target role as `rp-client`

## Phase 1 Execution Order

## Step 0. Freeze The Architecture In Docs

### Outcome

- system design doc
- updated ADR
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

## Step 3. Fill In `rp-runtime/`

### Objective

Treat the current `rp-runtime/` crate as the host runtime shell, not as the canonical node engine.

### Constraints

- keep host runtime logic out of the future shared node engine
- avoid putting protocol or consensus logic into the runtime shell

## Step 4. Evolve `erp-runtime/`

### Objective

Treat the embedded crate as the embedded node runtime rather than as a simple client shell.

### Responsibilities to document and then implement

- transport integration
- storage integration
- timing and wake integration
- identity management
- bounded memory policy
- optional local web-wallet hosting later

## Step 5. Evolve `rp-client/`

### Objective

Keep the wallet and operator workflows out of the node engines.

### Responsibilities

- wallet UX
- CLI or operator commands
- transaction construction and signing
- interaction with runtimes
- optional web-wallet build target later

## Step 6. Create The First Extraction Plan

### Objective

After the role boundaries are clear, identify the first safe code movement slices.

### Recommended slices

1. move `main.rs` out of the future engine path
2. isolate storage adapters from blockchain logic
3. isolate peer and sync logic from runtime glue
4. define the node event and action boundary

## The First Real Coding Task After Phase 1 Docs

Once the docs are accepted, the highest-leverage coding task is:

**Split the current `rp-core/` code into blockchain-engine concerns and node-engine concerns before doing any runtime-specific feature work.**

That is the move that makes every later crate boundary honest.

## Things Not To Do In Phase 1

- do not add more host-only logic to the mixed crate
- do not make embedded networking decisions the definition of the node engine
- do not move wallet UX into the node engines
- do not create a separate web-wallet crate yet unless `rp-client` becomes too large
- do not bind `rp-node` to a specific transport framework too early

## Phase 1 Success Signal

You know Phase 1 has worked when a newcomer can read the docs and immediately understand:

- what `rp-core` does
- what `rp-node` does
- why `rp-runtime` and `erp-runtime` both exist
- why `rp-client` is not the node
- why the embedded side is a first-class peer runtime
