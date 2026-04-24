# Embedded Rust-Proof Runtime

`erp-runtime/` is the embedded runtime crate.

Its target role is not “thin client.” Its target role is:

- embedded runtime that hosts the shared `rp-node` engine
- first-class peer in the P2P network
- constrained runtime implementation for ESP-class hardware

## Target role

In the target architecture, this crate becomes the embedded runtime shell around the shared engines.

It should eventually provide:

- transport integration for the embedded network stack
- bounded storage integration
- timer and wake integration
- identity and signing integration
- device-specific recovery and restart behavior
- optional hosting of a wallet webpage from device storage

The node logic itself should live in `rp-node`, and the blockchain rules should live in `rp-core`.

## What does not belong here

This crate should not become the only place where peer logic exists.

It should not own:

- consensus rules
- block validation rules
- canonical encoding rules
- node sync state machines that need to be shared with desktop nodes

Those belong in `rp-core` and `rp-node`.

## Current state

This crate has moved past pure embedded bring-up and now contains the first real runtime-host slice around `rp-node`.

Current implemented pieces include:

- runtime bootstrap that constructs `NodeEngine`
- runtime event loop and `NodeAction` dispatch
- NVS-backed snapshot and block persistence
- wake scheduling, including a test-only startup heartbeat
- runtime identity and signing integration

It still includes bring-up-oriented behavior in a few places, especially transport. The major incomplete area is still the embedded network adapter.

It currently also focuses on:

- ESP32-P4 host firmware in Rust
- ESP-Hosted integration for Wi-Fi via an ESP32-C6 co-processor
- async Wi-Fi connection flow using `AsyncWifi`
- embedded task and timing experiments

That means the crate is now an in-progress embedded runtime host, but not yet the finished embedded node runtime.

## Hardware direction

Current target setup:

- ESP32-P4 as the host MCU
- ESP32-C6 as the Wi-Fi co-processor
- ESP-Hosted over SDIO

## Runtime direction

The long-term design is:

- `rp-core` provides blockchain rules
- `rp-node` provides node behavior
- `erp-runtime` provides the embedded environment implementation for those engines

This lets the ESP participate as a real peer without duplicating node logic.

## Wallet web hosting

If desired, this runtime may later host a wallet webpage directly from the device.

That is a runtime capability, not a node-engine capability.

Recommended security model:

- webpage is served by the runtime
- key material remains on the device by default
- browser requests operations from the runtime instead of becoming the default source of truth for keys

## Build and flash

The embedded target configuration for this crate is still target-specific and separate from normal host workflows.

Build from inside this directory with your Wi-Fi credentials when needed.

`esp-idf-sys` is configured to treat `erp-runtime/` as its local workspace for ESP-IDF assets. That keeps ESP downloads and generated files under this crate, primarily in `erp-runtime/.embuild/`, instead of the repository root.

The root `Cargo.toml` is still required because this repository is a Cargo workspace. It is the workspace manifest, not a duplicate package manifest for `erp-runtime`.
 