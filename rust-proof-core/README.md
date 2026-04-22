# rust-proof Core Transition

`rust-proof-core/` currently contains a mixed set of code paths that are on the way to two different target crates:

- `rp-core`
- `rp-node`

## Target split

### `rp-core`

Target role:

- blockchain engine
- `no_std + alloc`
- canonical models, encoding, hashing, validation, state transition, and fork-choice inputs and outputs

### `rp-node`

Target role:

- node engine
- `no_std + alloc`
- peer state, sync logic, mempool policy, import orchestration, capability negotiation, and runtime boundary definitions

## Why this split matters

The current mixed crate still contains host runtime concerns such as:

- runtime orchestration
- storage integration
- networking integration
- binary entry points

Those do not belong in the final shared engine layer.

The architecture requires one blockchain engine and one node engine that can be reused by both desktop and embedded runtimes.

## Current direction

This directory should be reduced over time until:

- blockchain-engine logic is isolated cleanly
- node-engine logic is isolated cleanly
- runtime-specific code is removed into runtime crates

## Design references

For the system-level design, see the root [system design](/home/barend/repos/rust-proof/docs/system-design.md).

For the current migration plan, see [ROADMAP.md](/home/barend/repos/rust-proof/ROADMAP.md).
