# ADR 0001: Shared Engine and Multi-Runtime Boundary

- Status: Accepted
- Date: 2026-04-22

## Context

The project needs to support all of the following simultaneously:

- one canonical blockchain engine
- one canonical node engine
- a desktop or server runtime that can host a node
- an embedded runtime that can host a node
- a separate wallet and operator application

The previous architecture assumptions were too host-centric and treated the embedded side as a lighter client tier. That does not match the intended direction. The ESP must be able to participate as a first-class peer.

At the same time, the project should avoid duplicating node behavior across desktop and embedded codebases.

## Decision

The target architecture is a five-crate model.

### `rp-core`

`rp-core` is the blockchain engine.

It is intended to be `no_std + alloc` and owns:

- consensus rules
- state transition
- canonical encoding
- hashing
- validation
- typed consensus errors

### `rp-node`

`rp-node` is the device-agnostic node engine.

It is intended to be `no_std + alloc` and owns:

- peer state machines
- sync logic
- mempool policy
- import orchestration
- message handling
- runtime boundary traits for transport, storage, clock, wake, and identity

### `rp-runtime`

`rp-runtime` is the desktop or server runtime that hosts `rp-node`.

It is `std`-based and owns:

- transport integration
- storage integration
- process lifecycle
- observability
- optional local APIs and optional wallet web hosting

### `erp-runtime`

`erp-runtime` is the embedded runtime that hosts `rp-node`.

It is target-specific and owns:

- embedded transport integration
- embedded storage integration
- timers and wakeups
- device identity integration
- optional on-device wallet web hosting

### `rp-client`

`rp-client` is the wallet and operator application.

It is `std`-based and owns:

- wallet UX
- operator CLI
- transaction construction and signing flows
- optional static web assets that may be served by a runtime

## Consequences

### Architectural consequences

- node logic is shared once in `rp-node`
- blockchain logic is shared once in `rp-core`
- desktop and embedded peers are differentiated by runtime adapters, not by duplicated protocol logic
- wallet functionality remains outside the node engines

### Repository consequences

The directory names now match the target crate names, but the internal code is still transitional.

Target mapping:

- current `rp-core/` must still be split internally into `rp-core` and `rp-node` boundaries
- current `rp-node/` is the destination shared node-engine crate
- current `rp-runtime/` is the destination host runtime crate
- current `rp-client/` is the wallet application crate
- current `erp-runtime/` is the embedded runtime crate

### Implementation consequences

Immediate refactor priorities are:

1. isolate blockchain engine logic from runtime concerns
2. isolate node engine logic from runtime concerns
3. define the trait and event boundary used by runtimes
4. keep runtime shells thin and target-specific

## Alternatives considered

### Keep a host-only node runtime as the canonical node

Rejected.

That would make the embedded side a second-class participant and would duplicate node behavior across device classes.

### Make the full runtime itself device-agnostic

Rejected.

The shared part should be the node engine, not the concrete runtime shell. Transport stacks, storage backends, timers, and process control remain platform-specific.

## Scope implied by this decision

Version `0.1` should prove that:

- the same blockchain engine works across runtimes
- the same node engine works across runtimes
- desktop and embedded runtimes can both host real peers
- the wallet remains a separate application layer