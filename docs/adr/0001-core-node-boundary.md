# ADR 0001: Core Engine and Host Runtime Boundary

- Status: Accepted
- Date: 2026-04-22

## Context

The workspace already has the correct product split at a high level:

- `rust-proof-core/` contains blockchain logic, storage, networking, and the current node binary
- `rust-proof-client/` is the PC client scaffold
- `erp-client/` is the embedded client scaffold

The problem is that `rust-proof-core/` is currently a mixed crate. It contains both deterministic blockchain logic and host-only runtime concerns such as:

- `tokio` actor orchestration
- persistent storage via `sled`
- `libp2p` networking
- the binary entry point

That mixed boundary blocks the planned `no_std` migration, because only the deterministic blockchain engine should move to `no_std`. The host runtime must stay `std`-based.

## Decision

We split responsibilities as follows.

### `rust-proof-core`

`rust-proof-core` is the deterministic blockchain engine crate.

It owns:

- transaction, block, and slash-proof models
- canonical binary encoding and hashing
- signature verification logic
- mempool ordering rules
- ledger state transition logic
- validator selection logic
- block validation logic
- state-root computation
- typed consensus and validation errors

It does not own:

- process startup
- async runtimes
- channels or actor orchestration
- filesystem or database access
- network sockets or peer discovery
- JSON-RPC or external transport concerns

`no_std` applies to this engine crate only.

### `rust-proof-node`

`rust-proof-node` is the host runtime crate.

It owns:

- the binary entry point
- `tokio` runtime orchestration
- node command handling and actor wiring
- persistent storage adapters
- `libp2p` networking and sync
- RPC and host-facing APIs
- observability and operator runtime concerns

It depends on `rust-proof-core` for all consensus-critical validation and state transition behavior.

### `rust-proof-client`

`rust-proof-client` is the desktop client crate.

It owns:

- wallet and key management for desktop workflows
- transaction construction and signing
- RPC interaction with `rust-proof-node`
- operator and developer CLI flows

It must not reimplement consensus-critical validation logic that can drift from `rust-proof-core`.

### `erp-client`

`erp-client` is the embedded client crate.

For the first release it is a signer and light client, not a full node.

It owns:

- device connectivity and retry logic
- device identity and signing
- compact protocol interaction with a host node
- limited verification such as headers, checkpoints, or compact proofs where feasible

It does not attempt:

- full archival storage
- full host-node feature parity
- full embedded swarm participation in the first release

## Consequences

### Immediate consequences

- the package currently named `rust-proof` inside `rust-proof-core/Cargo.toml` should be renamed to `rust-proof-core`
- a new `rust-proof-node` crate should be added before major feature work continues
- the current `src/main.rs`, `node.rs`, `storage.rs`, and `network/` code paths must move out of the future core crate

### Development consequences

- feature work in consensus, state transition, and canonical encoding belongs in `rust-proof-core`
- feature work in networking, storage, RPC, and process orchestration belongs in `rust-proof-node`
- client-facing protocol types should be shared explicitly rather than copied between clients
- the embedded client should target stable node-facing protocols instead of depending on internal host-node details

### Migration consequences

The implementation order is:

1. rename the core package so package identity matches responsibility
2. add `rust-proof-node` as a host runtime crate
3. move the binary entry point into `rust-proof-node`
4. move actor, storage, and networking concerns into `rust-proof-node`
5. extract a pure engine API inside `rust-proof-core`
6. migrate that engine API to `no_std`

## Alternatives considered

### Make the entire node `no_std`

Rejected.

That would force networking, storage, async orchestration, and host integrations into a target they do not fit. It also does not match the actual product split in this workspace.

### Keep one mixed crate and use feature flags

Rejected.

That would hide the boundary instead of fixing it. The runtime and engine responsibilities would remain coupled, making both testing and the later `no_std` migration harder.

## First-release scope implied by this decision

The first release targets:

- one or more host nodes running the blockchain runtime
- a PC client that can create keys, sign, submit, and query
- an embedded client that can authenticate, sign, submit, and verify a compact chain view

The first release does not target:

- embedded full-node behavior
- advanced smart contracts
- privacy systems
- advanced interoperability or governance features