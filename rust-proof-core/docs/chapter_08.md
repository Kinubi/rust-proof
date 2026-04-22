# Chapter 8: Networking, Node Engines, And Runtime Boundaries

This chapter is no longer framed as “add libp2p directly to the core crate.”

The architecture has moved to a stricter model:

- blockchain logic belongs in `rp-core`
- shared node behavior belongs in `rp-node`
- transport and runtime specifics belong in runtime crates

## Why this change matters

The project needs both desktop and embedded peers to share one node engine.

If networking logic is written directly against one runtime stack, the node behavior becomes hard to reuse on constrained devices.

So the networking chapter is now about designing a shared node engine and then connecting it to real runtimes.

## The key idea

A blockchain node has two different layers:

1. node behavior
2. runtime integration

### Node behavior

This includes:

- peer state
- sync logic
- message routing
- import orchestration
- mempool admission policy
- capability negotiation

This belongs in `rp-node`.

### Runtime integration

This includes:

- how bytes move across the network
- how blocks are stored
- how timers are delivered
- how the process or device is driven

This belongs in `rp-runtime` and `erp-runtime`.

## Learning objective of this chapter

The goal is to understand how to model networking in a way that both desktop and embedded runtimes can share.

That means you should think in terms of:

- input events into the node engine
- output actions from the node engine
- runtime traits or adapter boundaries

## Recommended runtime boundary concepts

The exact types can evolve, but the architecture should support these concepts:

- transport boundary
- storage boundary
- clock boundary
- wake or scheduler boundary
- identity boundary

## Example node-engine flow

A simple flow looks like this:

1. runtime receives bytes from a peer
2. runtime decodes them into a node-engine input
3. `rp-node` processes the input
4. `rp-node` emits actions such as send, broadcast, persist, or request
5. runtime executes those actions using its own environment

This lets one node engine drive two very different runtime shells.

## What about libp2p?

`libp2p` may still be a good choice for a desktop or server runtime.

But it should be a runtime implementation detail, not the definition of the node engine.

That keeps the shared architecture flexible enough for embedded participation.

## What you build from this chapter onward

By the end of this chapter’s redesigned direction, you should be aiming for:

- a shared node engine that does not assume desktop-only runtime behavior
- a host runtime that can drive that engine
- an embedded runtime that can drive that same engine
