# rust-proof Client

`rust-proof-client/` is the current repository path for the future `rp-client` crate.

Its target role is the wallet and operator application for the `rust-proof` network.

## Target role

In the target architecture, `rp-client` is not the canonical node and not the shared node engine.

It is the user-facing application layer responsible for:

- wallet UX
- operator CLI flows
- transaction construction and signing
- runtime interaction and diagnostics
- optional static wallet web assets that a runtime may host

## What belongs elsewhere

The following do not belong in `rp-client`:

- consensus rules
- block validation rules
- sync state machines
- transport integration for peer networking
- embedded-specific runtime behavior

Those belong in `rp-core`, `rp-node`, `rp-runtime`, and `erp-runtime`.

## Interaction model

`rp-client` should be able to operate against either runtime surface:

- desktop or server nodes hosted by `rp-runtime`
- embedded nodes hosted by `erp-runtime`

That keeps the wallet and operator workflows independent from the node-hosting environment.

## Optional web-wallet direction

The same wallet project may later produce static web assets.

Those assets can be:

- served by a desktop runtime
- served by an embedded runtime
- used locally during development

That keeps wallet UX in one place while still allowing device-hosted wallet pages.

## Current state

Today this crate is still a small scaffold.

It is expected to evolve toward the `rp-client` role as the engine and runtime boundaries become stable.
