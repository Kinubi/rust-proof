# rust-proof workspace

`rust-proof` is being redesigned around a single blockchain engine, a single node engine, multiple runtimes, and one wallet application.

## Target architecture

The target logical crates are:

- `rp-core`
	- blockchain engine
	- `no_std + alloc`
- `rp-node`
	- device-agnostic node engine
	- `no_std + alloc`
- `rp-runtime`
	- desktop or server runtime that hosts `rp-node`
	- `std`
- `erp-runtime`
	- embedded runtime that hosts `rp-node`
	- target-specific embedded integration
- `rp-client`
	- wallet and operator application
	- `std`

The core idea is simple:

- consensus and state transition live in one place
- node behavior and sync logic live in one place
- desktop and embedded nodes share the same node engine
- runtimes implement transport, storage, timers, and process integration
- the wallet is a separate application layer and may optionally ship static web assets that a runtime can host

## Current repository state

The repository now uses the target crate names at the directory level, but the code inside those crates is still in transition.

Current directories map to the target design like this:

- `rp-core/`
	- still a transitional mixed crate that must be split internally into true `rp-core` and `rp-node` boundaries
- `rp-node/`
	- crate skeleton for the shared node engine
- `rp-runtime/`
	- runtime shell crate for the desktop or server side
- `rp-client/`
	- wallet and operator application scaffold
- `erp-runtime/`
	- embedded runtime scaffold

## Design rules

1. `rp-core` owns blockchain rules and nothing else.
2. `rp-node` owns node behavior and nothing else.
3. `rp-runtime` and `erp-runtime` are thin runtime shells.
4. Desktop and embedded peers use the same node engine.
5. The embedded side is a first-class peer runtime, not a second-class client.
6. The wallet lives in `rp-client`, not inside the node engine.

## Detailed design

See [docs/system-design.md](docs/system-design.md) for the full architecture.

See [ROADMAP.md](ROADMAP.md) for the migration plan.

See [PHASE_1_BUILD_PLAN.md](PHASE_1_BUILD_PLAN.md) for the next execution phase.

## Current build notes

The workspace is still in a transitional state.

- `erp-runtime/` keeps embedded target-specific configuration
- host-oriented and node-engine code are still being split out of the current mixed `rp-core/` crate
- the crate names now match the target architecture, but the internal code boundaries do not yet