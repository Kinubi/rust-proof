# rust-proof Embedded libp2p-Compatible Profile

Date: 2026-04-29  
Status: design and implementation plan for a minimal libp2p-compatible subset for `erp-runtime`

## 1. Purpose

This document explains how to make `erp-runtime` a real peer in a libp2p-based network without trying to run the full desktop `libp2p` stack on the ESP.

The target outcome is:

- `rp-runtime` can continue using `libp2p` on desktop or server targets
- `erp-runtime` can interoperate with a defined libp2p-compatible subset
- `rp-node` remains transport-agnostic and does not become coupled to libp2p types
- the embedded runtime stays memory-bounded and implementable on ESP32-P4 + ESP-IDF

This is not a thin-client plan.

The ESP still owns:

- its own runtime loop
- its own validation and import decisions through `rp-node`
- its own storage
- its own sync decisions
- its own node identity

What gets reduced is transport feature breadth, not node authority.

## 2. How To Read This Document

If this is not your area, read in this order:

1. Section 3 for the short version.
2. `docs/embedded-libp2p-implementation-guide.md` if you are actually writing the ERP transport code.
2. Section 4 for the concrete protocol subset.
3. Section 5 for the architectural decisions that matter most.
4. Section 8 and Section 9 for the file-by-file changes.
5. Section 12 for the implementation order.

If you are implementing this later, the most important sections are:

- Section 5: architecture decisions
- Section 8: `rp-node` and `rp-runtime` changes
- Section 9: `erp-runtime` module layout
- Section 10: protocol flow
- Section 12: step-by-step delivery order
- `docs/embedded-libp2p-implementation-guide.md`: coding-order workbook with exact file targets, protocol ids, dependency choices, and compile checkpoints

### 2.1 Naming Rule For This Document

This document uses three kinds of type names:

1. existing types already present in the repository
2. proposed new project-local types that do not exist yet
3. standard library or third-party types used only as examples

When a type in this document is only a proposed local helper, the intent is:

- it is a suggested name, not a magic external dependency
- it should be created inside this repository if we decide to keep that design
- if a simpler existing type already does the job, prefer the simpler existing type

Concrete examples:

- `IdentityManager` is an existing type in `erp-runtime`
- `TransportIdentityManager` is a proposed new local type for transport keys
- `std::net::SocketAddr` is the first implementation choice for ESP-IDF socket endpoints
- `MultiaddrLite` in this document is the proposed reduced address type for config, not a full libp2p multiaddr implementation

If the code already uses a different local name for the same concept, prefer aligning the document to the code that actually exists.

For example, if `erp-runtime/src/network/config.rs` already defines `MultiaddrLite`, keep using that name until there is a concrete reason to rename it everywhere.

The goal of this section is to avoid undefined placeholder names that sound more concrete than they really are.

### 2.2 Rule For Examples And Hidden Dependencies

This document should be implementable without making the reader guess at hidden crates, hidden wire formats, or hidden state-machine rules.

Whenever a section says "implement X", it must answer all of these:

- is `X` a project-local helper or a third-party dependency?
- if `X` is third-party, what is the first recommended crate?
- what wire format does `X` actually speak?
- if the API is `async` over `std::net`, who hides `WouldBlock`, `Interrupted`, or `NotConnected`?
- if the current scaffold already uses a different local name, which name wins?

For this document, the standing answers are:

- custom rust-proof application protocols (`NodeHello`, `Sync`, `Announce`) use postcard
- standard libp2p compatibility layers do not use postcard:
    - multistream-select has its own protocol framing
    - Identify uses the libp2p Identify protobuf schema
    - Noise uses libp2p-compatible Noise XX handshake bytes
    - Yamux uses Yamux framing
- the first ESP socket backend uses `std::net::{TcpListener, TcpStream}` for listener and stream wrappers
- nonblocking outbound connect on ESP-IDF uses a socket created once and polled until completion; a retry loop around `TcpStream::connect` is explicitly wrong
- if this document recommends a crate in one section, that crate should also appear again in Section 13

## 3. Short Version

The minimal embedded profile should be:

- TCP only
- standard libp2p-style multistream negotiation
- Noise security
- Yamux stream multiplexing
- Identify
- optional Ping
- one custom `NodeHello` request-response protocol
- one custom `Sync` request-response protocol
- one custom `Announce` request-response protocol
- static bootstrap peers only in v1

The things we should not require for ESP interoperability in v1 are:

- Kademlia
- Gossipsub
- QUIC
- WebRTC
- relay
- hole punching
- JSON codecs

The single biggest design rule is:

- `rp-node` should keep a canonical node-level peer identity
- transport-level libp2p peer identity should stay in the runtime layer

That means `rp-node` does not need to know what a libp2p `PeerId` is.

## 4. Minimal Compatibility Profile

### 4.1 What Compatibility Means Here

For this project, "libp2p compatible" means:

- `rp-runtime` can expose a specific transport profile using real `libp2p`
- `erp-runtime` can speak the same wire protocols for that profile
- both runtimes can exchange node-level messages over that shared profile

It does not mean:

- the ESP must compile all of `libp2p`
- the ESP must implement every protocol a desktop node may support
- the ESP must support every transport combination a desktop node may support

### 4.2 The v1 Profile

The recommended v1 profile is:

| Layer | v1 requirement | Notes |
| --- | --- | --- |
| IP connectivity | Wi-Fi via ESP32-C6 + ESP-Hosted | existing hardware direction |
| Socket transport | TCP only | no QUIC, no UDP discovery in v1 |
| Negotiation | multistream-select v1 | enough to choose protocols cleanly |
| Secure channel | Noise | match the host profile used by `rp-runtime` |
| Multiplexing | Yamux | one TCP connection can carry control and sync streams |
| Standard control protocol | Identify | lets the host advertise supported protocols |
| Optional control protocol | Ping | useful but not required for the first slice |
| Custom protocol | `/rust-proof/node-hello/1` | binds runtime transport identity to node identity |
| Custom protocol | `/rust-proof/sync/1` | bounded block sync request-response |
| Custom protocol | `/rust-proof/announce/1` | transaction and block announcements with ack |
| Discovery | static bootstrap list | no Kad requirement for ERP peers |

### 4.3 What Is Explicitly Out Of Scope In v1

Do not make these part of the ERP-required profile initially:

- `/ipfs/kad/1.0.0`
- Gossipsub or Floodsub
- QUIC transports
- WebRTC transports
- relay reservation or relay hopping
- hole punching
- browser interoperability
- mDNS as a requirement

mDNS may be added later as a convenience feature, but it should not block the first interoperable implementation.

### 4.4 Locked v1 Decisions

These choices should be treated as fixed for the first end-to-end implementation.

Do not reopen them unless one proves impossible on hardware.

| Topic | Locked v1 decision | Why |
| --- | --- | --- |
| Node identity | keep the existing node identity path in `erp-runtime/src/identity/manager.rs` | avoids redesigning validator or node identity while transport is still being built |
| Transport identity | use a separate libp2p-compatible transport identity persisted in NVS; Ed25519 is the default, ECDSA P-256 is also valid | keeps transport keys separate from node keys without blocking libp2p compatibility |
| Transport peer id | use canonical libp2p `PeerId` bytes derived from the chosen transport public key | keeps ERP transport identity compatible with host-side libp2p semantics |
| Noise handshake | implement libp2p-compatible Noise XX semantics, not a custom Noise variant | interoperability depends on matching the host stack here |
| Socket backend | implement only the ESP-IDF socket backend in the first delivery | avoids splitting time between two network backends before one works |
| Address support | support `Ip4Tcp` first; keep `Dns4Tcp` in the config type if useful, but allow it to return a clear "not implemented yet" error initially | removes DNS from the critical path |
| Peer registry storage | use `Vec<Option<PeerSession>>` with `SessionId` as the slot index | simpler than adding another container abstraction early |
| Ping | do not advertise or implement Ping until NodeHello, Sync, and Announce work end-to-end | keeps the first milestone focused on required functionality |
| Identify consumption | only use Identify for supported protocol checks and transport identity metadata; do not build dialing or peerstore logic from it in v1 | prevents a large side quest into host-like peer management |

The main consequence is that the first shipping ERP transport owns two distinct keys:

- node identity key: existing project node identity
- transport identity key: a libp2p-compatible identity key used for transport peer identity; Ed25519 is the default, ECDSA P-256 is also valid

## 5. Architecture Decisions

This section is the core of the design.

### 5.1 Keep Node Identity Separate From Transport Identity

Do not force the node engine to adopt libp2p's transport identity model.

Instead, use two identities:

1. Node identity
2. Transport identity

Node identity is what `rp-node` cares about.

Transport identity is what the libp2p-compatible transport cares about.

Recommended meaning of each:

- Node identity:
  - stable node-level identity
  - today this already exists in `erp-runtime/src/identity/manager.rs`
  - used for node-level signatures and peer naming inside `rp-node`
- Transport identity:
        - persistent libp2p-compatible identity keypair used by the transport stack
    - used for transport-level peer identity and for authenticating the Noise handshake in a libp2p-compatible way
    - stored in NVS for ERP in the first implementation
  - should not be coupled to consensus or validator identity

For clarity:

- this identity key can be Ed25519 or ECDSA P-256
- if you choose ECDSA P-256, derive the libp2p `PeerId` from the protobuf-encoded `ECDSA` public key
- this does not change the Noise DH algorithm; libp2p Noise still uses X25519 for the static handshake key

This separation is important because desktop libp2p identity constraints and ESP validator identity constraints are not the same problem.

### 5.2 Keep `rp-node::PeerId` As A Canonical Node Peer Identity

The current `rp-node` contract uses:

```rust
pub type PeerId = [u8; 32];
```

That can stay, as long as we clearly define what it means.

Recommended interpretation:

- `rp-node::PeerId` is the canonical node-level peer identity
- it is not the same thing as libp2p transport peer identity
- runtimes are responsible for mapping transport sessions to node peer identity

This is the cleanest approach because:

- `rp-node` stays transport-agnostic
- `rp-node` does not need libp2p types
- ERP and host runtimes can use different transport stacks while still presenting the same node identity to the engine

### 5.3 Add A `NodeHello` Binding Step

Because node identity and transport identity are separate, the runtime needs to prove that they belong together.

That is the purpose of a custom `NodeHello` protocol.

The idea is simple:

- the transport layer first authenticates transport identity through Noise
- after the secure Yamux session is ready, both sides exchange a signed node-level hello
- the hello says, effectively: "this node identity is bound to this transport identity"

Only after this verification succeeds should the runtime emit:

- `RuntimeEvent::PeerConnected { peer }`

to the node engine.

### 5.4 Replace JSON With A Compact Binary Codec

`rp-runtime` currently uses JSON request-response for sync.

That is wrong for the embedded-compatible profile.

The ERP-compatible profile should standardize on a binary codec across both runtimes.

Recommended choice:

- `postcard`

Reasons:

- already used in shared message types
- compact
- deterministic enough for this runtime boundary
- good fit for ESP memory limits

Important scope rule:

- use postcard only for rust-proof application protocols and local helper transcripts
- do not use postcard for multistream-select, Identify, Noise, or Yamux
- if a protocol is a standard libp2p control layer, keep its native wire format and only translate into local structs after parsing

### 5.5 Use ESP-IDF Networking First, Keep The Protocol Engine Portable

Because `erp-runtime` is already set up around ESP-IDF, `esp-idf-hal`, and `esp-idf-svc`, the first shipping implementation should use ESP-IDF sockets.

That means:

- Wi-Fi bring-up through ESP-IDF / ESP-Hosted
- TCP sockets through ESP-IDF / lwIP integration
- time and task coordination through Embassy-style primitives where helpful

Important clarification:

- do not try to run two IP stacks at once
- do not layer `embassy-net` on top of ESP-IDF lwIP in the first slice

Instead, design the transport code behind a small socket abstraction so that a future `embassy-net`-based implementation can be added later if the project moves away from ESP-IDF.

Concrete rule for the first delivery:

- `socket/esp_idf.rs` is the only backend that should be implemented
- `socket/embassy_net.rs` may exist as an empty placeholder file, but it should not consume implementation time before the ESP-IDF path is working end-to-end

### 5.5.1 What The ESP-IDF Socket Backend Means In Code

For v1, "ESP-IDF sockets" means this exact ownership split:

- Wi-Fi bring-up and `esp_netif` readiness are owned by `NetworkManager` and boot code
- the socket backend owns only TCP listener and stream objects
- `esp_netif` is not the socket type and should not be stored inside each accepted connection object

Concrete v1 backend choices:

- listener wrapper: `std::net::TcpListener`
- accepted stream wrapper: `std::net::TcpStream`
- outbound nonblocking connect helper: `socket2::Socket`
- async waiting between retries: `embassy_time::Timer`

Why `socket2` appears here:

- `TcpStream::connect(addr)` is a blocking call
- calling `TcpStream::connect(addr)` again in a retry loop does not "wait for the same connection"
- each call creates a brand-new socket and restarts the dial attempt

That means the first correct async-shaped outbound dial implementation must:

1. create one socket
2. set it nonblocking before `connect`
3. call `connect` once
4. treat `WouldBlock` or `Interrupted` as "dial in progress"
5. poll that same socket until it is connected or reports a real failure

The protocol layers above `SocketFactory` should never need to see `WouldBlock`, `Interrupted`, or `NotConnected`.

Those are backend-internal states.

### 5.6 Keep Peer Counts And Buffers Hard-Bounded

The runtime must define explicit limits for:

- max active peers
- max concurrent outbound dials
- max pending sync requests
- max frame size
- max blocks per sync chunk
- max queued outbound frames per peer
- max idle time before disconnect

If these limits are not decided up front, the first implementation will accidentally become desktop-shaped.

## 6. Recommended Wire-Level Design

### 6.1 Protocol Names

The exact strings can be adjusted, but the profile should converge on a fixed set early.

Recommended protocol names:

- standard negotiation:
  - `/multistream/1.0.0`
- standard transport support:
  - Noise
  - Yamux
- standard libp2p control protocols:
  - Identify
  - Ping
- custom rust-proof protocols:
  - `/rust-proof/node-hello/1`
  - `/rust-proof/sync/1`
  - `/rust-proof/announce/1`

### 6.2 `NodeHello` Protocol

This protocol exists to bind the node identity to the already-authenticated transport identity.

Recommended shape:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHello {
    pub version: u16,
    pub node_public_key: Vec<u8>,
    pub node_peer_id: [u8; 32],
    pub transport_peer_id: Vec<u8>,
    pub max_frame_len: u32,
    pub max_blocks_per_chunk: u16,
    pub capabilities: PeerCapabilities,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeHelloResponse {
    pub accepted: bool,
    pub remote: NodeHello,
    pub reject_reason: Option<NodeHelloRejectReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeHelloRejectReason {
    VersionMismatch,
    InvalidSignature,
    PeerIdMismatch,
    TransportBindingMismatch,
    UnsupportedRequiredProtocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerCapabilities {
    pub supports_sync_v1: bool,
    pub supports_announce_v1: bool,
    pub supports_ping: bool,
}
```

Verification rules on receipt:

1. `node_peer_id` must equal `hash(node_public_key)` using the canonical project rule.
2. `transport_peer_id` must equal the authenticated remote transport peer identity for the current session.
3. `signature` must verify under `node_public_key`.
4. The signature transcript must include:
   - protocol version
   - `node_peer_id`
   - `transport_peer_id`
   - capabilities
   - negotiated frame and chunk limits
5. Required capabilities for the current runtime profile must be present.

The runtime should only mark the peer as ready after this passes.

Important distinction:

- `NodeHello.signature` is signed by the node identity key, not the transport identity key
- the transport identity key is already authenticated by the Noise handshake
- `NodeHello` exists to bind those two identities together after transport authentication succeeds

### 6.3 `Sync` Protocol

This should be a binary request-response protocol.

Do not return an unbounded `Vec<Block>`.

Recommended v1 shape:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequest {
    pub from_height: u64,
    pub to_height: u64,
    pub max_blocks: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResponse {
    pub blocks: Vec<Block>,
    pub has_more: bool,
    pub next_height: Option<u64>,
}
```

This keeps the existing `SyncResponse` name while making it chunk-friendly for ERP.

Field meaning:

- `blocks`: the bounded block batch being returned now
- `has_more`: `true` if the remote still has more blocks in the requested range or beyond the current chunk limit
- `next_height`: the first height the requester should ask for next if it wants to continue

This is more ERP-safe than returning an arbitrary range in one response.

### 6.4 `Announce` Protocol

This protocol should be used for:

- new block announcements
- new transaction announcements

It can also be a request-response protocol with a lightweight ack, which is much easier to implement than a full pubsub system.

Recommended shape:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnounceRequest {
    pub kind: AnnounceKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnnounceKind {
    NewTransaction(Transaction),
    NewBlock(Block),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnnounceResponse {
    pub accepted: bool,
}
```

This matches the current direction in `rp-node`: `NetworkMessage` stays an enum over message structs, and announce-specific detail lives inside `AnnounceRequest.kind`.

Broadcast on ERP then simply means:

- iterate active peers
- send the same `AnnounceRequest`
- ignore or log negative acknowledgements

That is simpler and smaller than implementing Gossipsub on day one.

## 7. Runtime Model And Ownership

### 7.1 `rp-node` Responsibilities

`rp-node` should continue to own:

- node behavior
- chain import decisions
- mempool decisions
- sync decisions
- canonical node-level peer identity handling

It should not own:

- libp2p PeerId types
- Noise implementation details
- Yamux implementation details
- socket APIs
- connection retries
- transport peer mapping

### 7.2 `rp-runtime` Responsibilities

`rp-runtime` should own:

- real `libp2p` integration on host targets
- exposing both a full profile and an ERP-compatible subset profile
- mapping libp2p transport identity to node identity
- using binary codecs compatible with ERP

### 7.3 `erp-runtime` Responsibilities

`erp-runtime` should own:

- ESP-specific Wi-Fi and socket integration
- the custom embedded transport engine
- NodeHello verification
- session lifecycle
- mapping transport sessions to `rp-node::PeerId`

## 8. Proposed Changes By Crate

### 8.1 `rp-node`

The goal here is to keep changes minimal and honest.

Recommended changes:

1. Keep `PeerId = [u8; 32]`, but document clearly that it is node identity, not transport identity.
2. Add bounded sync response types instead of assuming unbounded block vectors.
3. Keep transport-facing messages binary and compact.
4. Avoid introducing libp2p-specific types into public contracts.

Recommended updates in or around `rp-node/src/contract.rs`:

```rust
pub type PeerId = [u8; 32];

// Clarify that this is canonical node identity, not transport identity.
```

Recommended updates in or around `rp-node/src/network/message.rs`:

```rust
#[derive(Serialize, Deserialize, Debug)]
pub struct SyncResponse {
    pub blocks: Vec<Block>,
    pub has_more: bool,
    pub next_height: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AnnounceRequest {
    pub kind: AnnounceKind,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum AnnounceKind {
    NewTransaction(Transaction),
    NewBlock(Block),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AnnounceResponse {
    pub accepted: bool,
}
```

You do not need to redesign the node engine around libp2p.

That would be a mistake.

### 8.2 `rp-runtime`

`rp-runtime` should stop assuming that the ERP-compatible profile is the same thing as its current full libp2p behavior.

Today, `rp-runtime/src/network/manager.rs` is shaped around:

- Gossipsub
- Kademlia
- JSON request-response
- fresh auto-generated identity

That is fine for experimentation, but it is the wrong contract for ERP interoperability.

Recommended structural split:

```rust
pub enum HostNetworkProfile {
    Full,
    EmbeddedCompatible,
}
```

Recommended behavior split:

```rust
#[derive(NetworkBehaviour)]
pub struct EmbeddedCompatibleBehaviour {
    pub identify: identify::Behaviour,
    pub ping: ping::Behaviour,
    pub node_hello: request_response::Behaviour<ReqResPostcardCodec<NodeHello, NodeHelloResponse>>,
    pub sync: request_response::Behaviour<ReqResPostcardCodec<SyncRequest, SyncResponse>>,
    pub announce: request_response::Behaviour<ReqResPostcardCodec<AnnounceRequest, AnnounceResponse>>,
}
```

Then optionally keep a richer host-only behavior for non-ERP peers.

Recommended implementation changes in `rp-runtime`:

1. Stop using `.with_new_identity()` for every startup.
2. Load or configure a persistent transport identity.
3. Replace JSON codecs with custom binary codecs.
4. Delay surfacing a peer to `rp-node` until NodeHello validation succeeds.
5. Keep Kademlia and Gossipsub optional for host-only profiles, not required for ERP compatibility.

### 8.3 `erp-runtime`

This is where most of the new code will live.

Recommended high-level rule:

- do not make `erp-runtime/src/network/manager.rs` itself do all protocol work inline

Instead, turn it into an orchestrator around smaller modules.

## 9. Proposed `erp-runtime` Module Structure

Recommended directory layout:

```text
erp-runtime/src/network/
  mod.rs
  manager.rs
  config.rs
  bootstrap.rs
  peer_registry.rs
  session.rs
  socket/
    mod.rs
    traits.rs
    esp_idf.rs
  transport/
    mod.rs
    multistream.rs
    noise.rs
    yamux.rs
  protocol/
    mod.rs
    identify.rs
    node_hello.rs
    sync.rs
    announce.rs
    ping.rs
  codec/
    mod.rs
    postcard_codec.rs
    length_prefixed.rs
```

This is more files than you have today, but each file has a narrow job.

### 9.1 `config.rs`

Purpose:

- central place for transport limits and bootstrap settings

Recommended shape:

```rust
pub struct NetworkConfig {
    pub listen_port: u16,
    pub max_peers: usize,
    pub max_outbound_dials: usize,
    pub max_frame_len: u32,
    pub max_blocks_per_chunk: u16,
    pub idle_timeout_ms: u64,
    pub bootstrap_peers: Vec<BootstrapPeer>,
}

pub struct BootstrapPeer {
    pub address: MultiaddrLite,
    pub expected_transport_peer: Option<Vec<u8>>,
}

pub enum MultiaddrLite {
    Ip4Tcp {
        addr: [u8; 4],
        port: u16,
    },
    Dns4Tcp {
        host: String,
        port: u16,
    },
}
```

`MultiaddrLite` is intentionally not a full libp2p multiaddr.

It is only a configuration type for the address forms we expect to dial in v1.

Recommended use:

- keep `MultiaddrLite` in config and persistence code
- resolve it into `std::net::SocketAddr` right before dialing
- add more address forms only when the runtime actually needs them

Concrete v1 rule:

- implement `Ip4Tcp` first
- if `Dns4Tcp` is present before DNS dialing exists, fail fast with a clear runtime error instead of silently ignoring it

Recommended helper to make that rule concrete:

```rust
pub fn resolve_bootstrap_addr(addr: &MultiaddrLite) -> Result<std::net::SocketAddr, RuntimeError>;
```

Expected behavior:

- `Ip4Tcp` maps directly into `SocketAddr::from(([a, b, c, d], port))`
- `Dns4Tcp` returns a clear `RuntimeError::Config("Dns4Tcp not implemented yet")` until DNS support is actually added
- do not silently skip an unsupported bootstrap address, because that makes bootstrap failures impossible to debug

### 9.2 `socket/traits.rs`

Purpose:

- isolate protocol code from the concrete socket provider

Recommended shape:

```rust
pub trait SocketFactory {
    type TcpStream;
    type TcpListener;

    async fn bind(&self, port: u16) -> Result<Self::TcpListener, RuntimeError>;
    async fn accept(
        &self,
        listener: &mut Self::TcpListener,
    ) -> Result<(Self::TcpStream, std::net::SocketAddr), RuntimeError>;
    async fn connect(
        &self,
        addr: std::net::SocketAddr,
    ) -> Result<Self::TcpStream, RuntimeError>;
}
```

Use `std::net::SocketAddr` here for the first ESP-IDF implementation.

Why this is acceptable:

- `erp-runtime` currently targets ESP-IDF with `std` available
- the socket layer already depends on ESP-IDF networking rather than a pure `no_std` stack
- using a standard endpoint type removes one extra local abstraction from the first implementation

If the runtime later moves away from ESP-IDF and `std`, this can be replaced by a project-local endpoint type.

Implementation plan:

- `esp_idf.rs` is the first real implementation
- `embassy_net.rs` remains an empty placeholder until the ESP-IDF backend is working end-to-end

This keeps the higher protocol code portable even if the transport substrate changes later.

Concrete v1 backend shape:

```rust
pub struct EspSocketFactory {
    pub addr: std::net::SocketAddr,
}

pub struct EspTcpListener {
    listener: std::net::TcpListener,
}

pub struct EspTcpStream {
    stream: std::net::TcpStream,
}

impl EspTcpStream {
    pub fn stream(&self) -> &std::net::TcpStream;
    pub fn stream_mut(&mut self) -> &mut std::net::TcpStream;
    pub fn into_inner(self) -> std::net::TcpStream;
}
```

Required dependency for the first real outbound dial implementation:

- `socket2`

Why this is required in practice:

- `accept` can be implemented correctly with a nonblocking `TcpListener`
- `connect` cannot be implemented correctly by looping on `TcpStream::connect`
- a correct nonblocking dial needs one socket that survives across poll iterations

Concrete method behavior for `socket/esp_idf.rs`:

- `bind(port)`:
  - call `TcpListener::bind(SocketAddr::new(self.addr.ip(), port))`
  - immediately call `set_nonblocking(true)`
- `accept(listener)`:
  - loop on `listener.accept()`
  - on success call `set_nonblocking(true)` on the accepted stream and return `(EspTcpStream, peer_addr)`
  - on `ErrorKind::WouldBlock`, yield with `Timer::after(Duration::from_millis(SOCKET_RETRY_DELAY_MS)).await`
- `connect(addr)`:
  - create one `socket2::Socket`
  - set it nonblocking before `connect`
  - call `connect` once
  - if the initial result is `WouldBlock` or `Interrupted`, keep polling that same socket
  - use `take_error()` plus `peer_addr()` on the same socket to detect completion or failure
  - only convert into `TcpStream` after success

Concrete skeleton for `connect`:

```rust
use socket2::{ Domain, Protocol, SockAddr, Socket, Type };

const SOCKET_RETRY_DELAY_MS: u64 = 150;

async fn connect(&self, addr: SocketAddr) -> Result<EspTcpStream, RuntimeError> {
    let socket = Socket::new(
        match addr {
            SocketAddr::V4(_) => Domain::IPV4,
            SocketAddr::V6(_) => Domain::IPV6,
        },
        Type::STREAM,
        Some(Protocol::TCP),
    )?;

    socket.set_nonblocking(true)?;

    match socket.connect(&SockAddr::from(addr)) {
        Ok(()) => return Ok(EspTcpStream { stream: socket.into() }),
        Err(error) if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::Interrupted) => {}
        Err(error) => return Err(RuntimeError::NetworkError(error)),
    }

    loop {
        if let Some(error) = socket.take_error()? {
            return Err(RuntimeError::NetworkError(error));
        }

        match socket.peer_addr() {
            Ok(_) => return Ok(EspTcpStream { stream: socket.into() }),
            Err(error) if error.kind() == ErrorKind::NotConnected => {
                Timer::after(Duration::from_millis(SOCKET_RETRY_DELAY_MS)).await;
            }
            Err(error) => return Err(RuntimeError::NetworkError(error)),
        }
    }
}
```

If the first milestone accepts a blocking dial temporarily, say that explicitly in both code comments and the document.

Do not make it look async if it is still blocking.

### 9.3 `peer_registry.rs`

Purpose:

- own the mapping between runtime transport sessions and canonical node peer identity

Recommended shape:

```rust
pub type SessionId = usize;

pub struct PeerSession {
    pub id: SessionId,
    pub node_peer_id: Option<[u8; 32]>,
    pub transport_peer_id: Vec<u8>,
    pub state: SessionState,
    pub max_frame_len: u32,
    pub max_blocks_per_chunk: u16,
    pub last_seen_ms: u64,
}

pub enum SessionState {
    TcpConnected,
    NoiseReady,
    YamuxReady,
    Identified,
    NodeReady,
    Closing,
}

pub struct PeerRegistry {
    sessions: Vec<Option<PeerSession>>,
    by_node_peer: BTreeMap<[u8; 32], SessionId>,
    max_peers: usize,
}
```

Notes on the storage containers:

- `BTreeMap` here means `alloc::collections::BTreeMap`
- `sessions[index] = Some(peer_session)` means the slot is occupied
- `sessions[index] = None` means the slot is free
- `SessionId` is just the index into `sessions`

Responsibilities:

- allocate and free session slots
- map node peer id to active session
- reject duplicate or excess peers
- provide fanout targets for broadcast

### 9.4 `session.rs`

Purpose:

- run the lifecycle of one connection from raw socket to ready peer

Recommended shape:

```rust
pub struct SessionWorker<S> {
    pub session_id: SessionId,
    pub stream: S,
    pub role: ConnectionRole,
    pub node_identity: IdentityManager,
    pub transport_identity: TransportIdentityManager,
    pub config: NetworkConfig,
}

pub enum ConnectionRole {
    Inbound,
    Outbound,
}
```

Type meaning here:

- `IdentityManager` is the existing node-identity type in `erp-runtime`
- `TransportIdentityManager` is a proposed new local type that loads, stores, and signs with the transport keypair used by Noise and transport peer identification
- `SessionWorker<S>` is not a framework type; it is just the proposed local connection-state worker

Recommended method breakdown:

```rust
impl<S> SessionWorker<S> {
    pub async fn run(self) -> Result<(), RuntimeError>;

    async fn negotiate_multistream(&mut self) -> Result<(), RuntimeError>;
    async fn upgrade_noise(&mut self) -> Result<(), RuntimeError>;
    async fn upgrade_yamux(&mut self) -> Result<(), RuntimeError>;
    async fn exchange_identify(&mut self) -> Result<IdentifyInfo, RuntimeError>;
    async fn exchange_node_hello(&mut self) -> Result<VerifiedPeer, RuntimeError>;
    async fn run_ready_loop(&mut self, verified: VerifiedPeer) -> Result<(), RuntimeError>;
}
```

Concrete `run()` order for v1:

1. obtain a raw TCP stream from the socket backend
2. negotiate multistream for the secure transport upgrade
3. run the Noise handshake and extract the authenticated remote transport peer id
4. negotiate the multiplexer upgrade
5. start Yamux in inbound or outbound mode depending on `ConnectionRole`
6. open one control substream and run Identify
7. open one control substream and run NodeHello
8. only after NodeHello verification succeeds, register the peer in `PeerRegistry`
9. enter the ready loop for Sync and Announce traffic

Do not skip from "TCP connected" directly to "peer connected".

The session worker is responsible for every intermediate transport state.

Recommended `IdentifyInfo` shape:

```rust
pub struct IdentifyInfo {
    pub protocol_version: String,
    pub agent_version: String,
    pub listen_addrs: Vec<MultiaddrLite>,
    pub supported_protocols: Vec<String>,
    pub observed_addr: Option<std::net::SocketAddr>,
    pub transport_peer_id: Vec<u8>,
}
```

This does not need to mirror every field from desktop libp2p internals.

It only needs the fields the runtime actually consumes during compatibility checks.

This is the right place to keep protocol sequencing, rather than bloating `NetworkManager`.

### 9.5 `transport/multistream.rs`

Purpose:

- implement the minimal negotiation needed to pick Noise, Yamux, and the application protocols

This module should do one thing:

- read and write multistream protocol selections safely under explicit size caps

Concrete wire-format rule:

- multistream-select is not postcard
- protocol names are exchanged as unsigned-varint length-prefixed UTF-8 strings ending in `\n`
- the first negotiation step on a fresh stream is the multistream header `/multistream/1.0.0\n`

Recommended first dependency:

- `unsigned-varint`

Recommended local helpers:

```rust
pub async fn write_multistream_proto<S>(stream: &mut S, protocol: &str) -> Result<(), RuntimeError>;
pub async fn read_multistream_proto<S>(stream: &mut S, max_len: usize) -> Result<String, RuntimeError>;
pub async fn select_one_of<S>(stream: &mut S, protocols: &[&str]) -> Result<String, RuntimeError>;
```

Required protocol selection order for the first outbound session:

1. multistream header
2. Noise protocol
3. multistream header again on the secured stream if the chosen implementation expects a fresh negotiation boundary
4. Yamux protocol
5. after Yamux is running, one protocol selection per substream for Identify, NodeHello, Sync, or Announce

### 9.6 `transport/noise.rs`

Purpose:

- implement the minimal Noise handshake compatible with the host's libp2p profile

Recommended dependency direction:

- prefer a `snow` configuration that does not depend on `ring`

This is important because previous experiments already showed that `ring` is a bad fit for the ESP build target.

Concrete v1 requirement:

- this module must implement libp2p-compatible Noise XX behavior for the chosen transport identity, not just any Noise-secured stream
- do not invent a custom peer-id derivation or a custom handshake transcript here
- the output of this stage must be an authenticated remote transport peer id plus a secure byte stream for Yamux

This is one of the easiest places to lose compatibility by guessing.

Concrete guidance:

- first dependency choice: `snow`
- source of truth: a host-side interop test against `rp-runtime`, not generic Noise tutorials
- deliverable from this module: a secure stream plus the authenticated remote transport public key and derived transport peer id

If the implementation cannot explain how the remote transport peer id is derived from the remote authenticated transport key, then the implementation is not done yet.

Do not proceed to Yamux until that mapping is proven in a host interop test.

### 9.7 `transport/yamux.rs`

Purpose:

- manage logical substreams over one secure transport session

Responsibilities:

- open a control substream for Identify and NodeHello
- open request-response substreams for Sync and Announce
- enforce substream limits

Concrete v1 rule:

- do not hand-roll a Yamux frame parser unless the crate option has already been proven impossible on target
- first dependency choice: `yamux`
- the first implementation may open a fresh substream per protocol exchange instead of inventing a long-lived custom control channel abstraction

The important part is standard Yamux interoperability, not clever stream lifecycle design.

### 9.8 `protocol/identify.rs`

Purpose:

- wrap the minimum Identify exchange needed for protocol discovery and peer metadata

For v1, keep the Identify implementation narrow and consistent with what the host expects.

The important thing is not fancy peerstore behavior.

The important thing is:

- protocol compatibility check
- visibility of supported protocols
- transport identity confirmation

Concrete v1 rule:

- only parse or emit the Identify fields the ERP runtime actually checks
- treat `listen_addrs`, `agent_version`, and `observed_addr` as informational only
- do not update bootstrap config or routing state from Identify in v1

Concrete wire-format rule:

- Identify is a standard protobuf message, not postcard
- do not invent a local Identify wire format just because the custom protocols use postcard

Recommended first dependency:

- `quick-protobuf` on ERP because it is lighter-weight
- `prost` is acceptable if the project later prefers one protobuf toolchain everywhere

Required fields to parse or emit in v1:

- public key
- protocol version
- agent version
- listen addresses
- observed address
- supported protocols

Unknown protobuf fields should be ignored, not treated as a decode failure.

### 9.9 `protocol/node_hello.rs`

Purpose:

- exchange and verify node identity binding

This module should contain:

- `NodeHello`
- `NodeHelloResponse`
- transcript construction for signature verification
- capability compatibility checks

Concrete transcript rule:

- build a dedicated local transcript struct
- postcard-encode that transcript struct
- sign the transcript bytes with the node identity key
- verify the same transcript bytes on receipt

Do not sign the already-serialized `NodeHello` frame directly.

That tends to make future field additions or framing changes harder to reason about.

### 9.10 `protocol/sync.rs`

Purpose:

- send and receive bounded sync requests and responses

Responsibilities:

- clamp request sizes to configured limits
- reject oversized responses
- convert wire messages to `RuntimeEvent::FrameReceived` or direct sync handling as needed

### 9.11 `protocol/announce.rs`

Purpose:

- send transaction and block announcements with explicit ack behavior

Responsibilities:

- fanout to ready peers
- bound the number of in-flight announces
- surface failures as logs or disconnects, not panics

Concrete v1 rule:

- announce delivery should be request-response with `AnnounceResponse { accepted: bool }`
- ERP should not implement pubsub semantics in this module
- ERP should treat `accepted = false` as a logged peer-level failure, not as a node-engine state transition

### 9.12 `manager.rs`

Purpose:

- orchestration only

Recommended shape:

```rust
pub struct NetworkManager<F: SocketFactory> {
    network_rx: NetworkRx,
    event_tx: EventTx,
    node_identity: IdentityManager,
    transport_identity: TransportIdentityManager,
    sockets: F,
    config: NetworkConfig,
    peers: PeerRegistry,
}
```

If the current code already has a concrete `NetworkManager` that owns Wi-Fi directly, it is fine to keep that concrete type in v1 and store `EspSocketFactory` as a concrete field.

The important seam is the `SocketFactory` boundary itself, not whether `NetworkManager` is generic on day one.

Recommended responsibilities:

- start the listener loop
- start the bootstrap dial loop
- receive `NetworkCommand`s from the node runtime
- route commands to ready sessions
- own registry updates and disconnect cleanup

Recommended non-responsibilities:

- raw protocol parsing
- Noise transcript logic
- Yamux framing details

## 10. Protocol Flow Step By Step

This section is the handholding version of the runtime sequence.

### 10.1 Outbound Dial Flow

1. `NetworkManager` chooses a bootstrap address from config.
2. `SocketFactory::connect` opens a TCP stream.
3. `SessionWorker` begins protocol negotiation.
4. Multistream selects the secure transport upgrade.
5. Noise authenticates transport identity.
6. Yamux starts.
7. Identify exchange confirms supported protocols and remote transport identity metadata.
8. `NodeHello` request-response binds node identity to that transport identity.
9. `PeerRegistry` stores `transport_session -> node_peer_id`.
10. `NetworkManager` emits `RuntimeEvent::PeerConnected { peer: node_peer_id }`.
11. The session is now allowed to carry Sync and Announce traffic.

Important implementation meaning of step 2:

- `SocketFactory::connect` should return only when the stream is actually usable or when the backend has a real error
- `WouldBlock`, `Interrupted`, and `NotConnected` are backend-internal states, not states the caller should handle

### 10.2 Inbound Accept Flow

1. Listener accepts a TCP connection.
2. `SessionWorker` runs the same negotiation sequence.
3. If Noise succeeds but NodeHello fails, disconnect.
4. If all checks pass, register the peer and emit `PeerConnected`.

### 10.3 Send Frame Flow

1. `rp-node` emits `NodeAction::SendFrame { peer, frame }`.
2. `NodeManager` translates that into `NetworkCommand::SendFrame`.
3. `NetworkManager` resolves `peer` through `PeerRegistry`.
4. The ready session sends the encoded request over the Announce protocol or Sync protocol depending on context.
5. On failure, either retry within configured bounds or disconnect the peer.

### 10.4 Broadcast Flow

1. `rp-node` emits `BroadcastFrame`.
2. `NetworkManager` takes a snapshot of ready peers.
3. The same bounded message is sent to each peer one at a time or under a small concurrency cap.
4. Failures are logged and may demote the peer.

### 10.5 Sync Flow

1. `rp-node` emits `RequestBlocks { peer, from_height, to_height }`.
2. `NetworkManager` clamps the request to `max_blocks_per_chunk`.
3. The peer receives a `SyncRequest`.
4. The remote responds with `SyncResponse`.
5. The runtime converts each block into the existing node-level flow.
6. If `has_more` is true, the runtime issues the next request only when ready.

## 11. Detailed Trait And Type Recommendations

This section is the most implementation-oriented part of the plan.

### 11.1 Keep The Existing Node Identity Trait

The existing shape is already close to what we need:

```rust
pub trait Identity {
    fn peer_id(&self) -> PeerId;
    fn public_key(&self) -> Vec<u8>;
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, ContractError>;
}
```

That is a good node-identity trait.

Do not mutate it into a libp2p identity trait.

### 11.2 Add A Runtime-Local Transport Identity Abstraction

Recommended runtime-local trait in `erp-runtime`:

```rust
pub trait TransportIdentity {
    fn transport_peer_id(&self) -> Vec<u8>;
    fn public_key_bytes(&self) -> Vec<u8>;
    fn signature_public_key_bytes(&self) -> Vec<u8>;
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, RuntimeError>;
}
```

Notes:

- this trait belongs in the runtime crate, not in `rp-node`
- it is fine for host and ERP runtimes to implement this differently
- transport identity should be persistent across restarts

Recommended first concrete shape:

```rust
pub enum TransportIdentityAlgorithm {
    Ed25519,
    EcdsaP256,
}

pub struct TransportIdentityManager {
    // Owns the persistent libp2p-compatible transport identity keypair.
    // Keep the algorithm explicit so Ed25519 and ECDSA P-256 both fit cleanly.
    // Back this with NVS in erp-runtime.
}
```

Concrete storage rule for v1:

- store one algorithm-tagged fixed-size secret key or scalar in NVS
- derive the public key and transport peer id at boot
- do not store an opaque host-side libp2p keypair blob if a simpler fixed-size record will do

Recommended first dependency:

- `ed25519-dalek` if you keep Ed25519, or `p256` if you switch to ECDSA P-256

Recommended first record shape:

```rust
pub struct TransportIdentityRecord {
    pub algorithm: TransportIdentityAlgorithm,
    pub secret_key: [u8; 32],
}
```

That record is intentionally boring.

The goal is persistence and reproducibility, not clever serialization.

Recommended first methods:

```rust
impl TransportIdentityManager {
    pub fn load_or_create(/* storage deps */) -> Result<Self, RuntimeError>;
    pub fn peer_id_bytes(&self) -> &[u8];
    pub fn public_key_bytes(&self) -> &[u8];
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, RuntimeError>;
}
```

For v1, this manager should own the transport identity only.

Do not merge it with the existing node `IdentityManager`.

### 11.3 Add A NodeHello Builder And Verifier

Recommended helper type:

```rust
pub struct NodeHelloBuilder<'a> {
    pub node_identity: &'a dyn Identity,
    pub transport_identity: &'a dyn TransportIdentity,
    pub max_frame_len: u32,
    pub max_blocks_per_chunk: u16,
    pub capabilities: PeerCapabilities,
}

impl<'a> NodeHelloBuilder<'a> {
    pub fn build(&self) -> Result<NodeHello, RuntimeError>;
}

pub struct NodeHelloVerifier;

impl NodeHelloVerifier {
    pub fn verify(
        remote: &NodeHello,
        authenticated_transport_peer: &[u8],
    ) -> Result<VerifiedPeer, RuntimeError>;
}
```

Recommended `VerifiedPeer`:

```rust
pub struct VerifiedPeer {
    pub node_peer_id: [u8; 32],
    pub node_public_key: Vec<u8>,
    pub transport_peer_id: Vec<u8>,
    pub max_frame_len: u32,
    pub max_blocks_per_chunk: u16,
    pub capabilities: PeerCapabilities,
}
```

Recommended transcript shape:

```rust
pub struct NodeHelloTranscript<'a> {
    pub version: u16,
    pub node_peer_id: [u8; 32],
    pub transport_peer_id: &'a [u8],
    pub max_frame_len: u32,
    pub max_blocks_per_chunk: u16,
    pub capabilities: &'a PeerCapabilities,
}
```

Build this transcript first, postcard-encode it, then sign those bytes.

### 11.4 Add A Binary Codec Layer

Recommended helper type:

```rust
pub trait ValueCodec<T> {
    fn encode(item: &T) -> Result<Vec<u8>, RuntimeError>;
    fn decode(bytes: &[u8]) -> Result<T, RuntimeError>;
}

pub struct PostcardCodec<T>(core::marker::PhantomData<T>);

pub struct ReqResPostcardCodec<Req, Resp>(
    core::marker::PhantomData<(Req, Resp)>
);
```

Meaning of these two helpers:

- `PostcardCodec<T>` is the simple local helper for one postcard-encoded type
- `ReqResPostcardCodec<Req, Resp>` is the host-side adapter shape for `libp2p::request_response`, where one protocol has one request type and one response type

The purpose is not abstraction for its own sake.

The purpose is to avoid scattering `postcard::to_allocvec` and `postcard::from_bytes` across every protocol handler.

Concrete scope rule:

- this codec layer is for `NodeHello`, `Sync`, `Announce`, and local transcript structs
- it is not for multistream-select, Identify protobuf, Noise handshake payloads, or Yamux frames

Recommended framing helper next to postcard helpers:

```rust
pub fn encode_length_prefixed(payload: &[u8], max_len: u32) -> Result<Vec<u8>, RuntimeError>;
pub fn decode_length_prefixed(frame: &[u8], max_len: u32) -> Result<&[u8], RuntimeError>;
```

Recommended first dependency for the length prefix helper:

- `unsigned-varint`

### 11.5 Add A Small Transport Upgrade Boundary

Recommended helper type:

```rust
pub struct TransportUpgrader;

pub struct MuxedSession<M> {
    pub muxer: M,
    pub remote_transport_peer_id: Vec<u8>,
}

impl TransportUpgrader {
    pub async fn secure_and_mux<S, M>(stream: S) -> Result<MuxedSession<M>, RuntimeError>;
}
```

`MuxedSession<M>` is just the project-local result of a successful transport upgrade.

It should own:

- the authenticated secure channel state
- the Yamux multiplexer instance
- the authenticated remote transport peer id

This keeps the protocol sequencing testable outside `NetworkManager`.

## 12. Implementation Order

This is the recommended delivery sequence.

### M1. Freeze The Embedded-Compatible Profile

Deliverables:

- this document accepted
- protocol names accepted
- decision made that ERP v1 does not require Kademlia or Gossipsub
- decision made to keep node identity separate from transport identity

### M2. Make `rp-runtime` Speak The Embedded Profile Cleanly

Tasks:

1. Split host behavior into `Full` and `EmbeddedCompatible`.
2. Replace JSON sync codec with a binary codec.
3. Add persistent host transport identity loading.
4. Add NodeHello protocol and verification.
5. Ensure a host node can operate without Kad and Gossipsub when running in embedded-compatible mode.

Exit criteria:

- host runtime exposes a stable ERP-compatible protocol surface

### M3. Add ERP Transport Identity Storage

Tasks:

1. Keep the existing node identity path in `erp-runtime/src/identity/manager.rs`.
2. Add a separate transport identity manager backed by NVS or another bounded store.
3. Ensure transport identity survives reboot.

Exit criteria:

- ERP has both node identity and transport identity available at boot

### M4. Build The ERP Session Skeleton

Tasks:

1. Introduce `NetworkConfig`, `PeerRegistry`, and `SessionWorker`.
2. Replace the current stub-only `NetworkManager` with an orchestrator.
3. Add socket abstraction and implement the ESP-IDF socket backend.
4. Use `std::net::{TcpListener, TcpStream}` for the listener and stream wrappers.
5. Use `socket2` for the outbound nonblocking dial path instead of retrying `TcpStream::connect`.

Exit criteria:

- TCP listener and outbound dialer exist
- outbound dial uses one socket per connection attempt, not redial-in-a-loop behavior
- session slots are tracked and bounded

### M5. Implement The Transport Stack

Tasks:

1. Implement multistream negotiation with `unsigned-varint` framing helpers.
2. Implement Noise with a named dependency such as `snow` and prove host interop before proceeding.
3. Implement Yamux with a crate-based multiplexer instead of a custom frame parser.
4. Add Identify support with a real protobuf implementation such as `quick-protobuf`.

Exit criteria:

- ERP can establish a secure multiplexed session with a host node in embedded-compatible mode
- the transport stack no longer depends on undocumented or hand-invented wire formats

### M6. Implement NodeHello And Peer Registration

Tasks:

1. Define NodeHello request and response types.
2. Implement transcript signing and verification.
3. Register `node_peer_id` only after validation succeeds.
4. Emit `PeerConnected` and `PeerDisconnected` only at the node-identity layer.

Exit criteria:

- `rp-node` sees stable node peers, not transport sessions

### M7. Implement Sync And Announce

Tasks:

1. Add bounded `SyncResponse` flow.
2. Add `AnnounceRequest` and `AnnounceResponse` flow.
3. Wire these through the existing `NetworkCommand` and `RuntimeEvent` machinery.

Exit criteria:

- ERP can request blocks from a host node
- ERP can announce blocks and transactions to a host node

### M8. Tighten Limits And Failure Handling

Tasks:

1. Enforce frame limits.
2. Enforce chunk limits.
3. Add handshake and idle timeouts.
4. Disconnect peers on repeated decode or signature failures.
5. Add simple backoff for redial.

Exit criteria:

- the runtime behaves like an embedded transport implementation, not a desktop prototype

## 13. Cargo And Dependency Guidance

This is guidance, not a lockfile.

### 13.1 `erp-runtime`

Likely keep:

- `esp-idf-hal`
- `esp-idf-svc`
- `embassy-time`
- `postcard`
- `sha2`
- current identity dependencies

Likely add:

- `socket2` for one-socket nonblocking outbound connect on ESP-IDF
- a bounded registry helper such as `slab`
- a multistream framing helper such as `unsigned-varint`
- a Noise implementation such as `snow` that does not drag in `ring`
- a Yamux implementation such as `yamux`
- a protobuf implementation for Identify, with `quick-protobuf` as the first ERP recommendation
- `p256` if the transport identity uses ECDSA P-256, or `ed25519-dalek` if it uses Ed25519
- `serde` derives for protocol structs if not already available through shared crates

Dependency-to-purpose table for ERP v1:

| Dependency | Where it shows up | Why it is needed |
| --- | --- | --- |
| `socket2` | `socket/esp_idf.rs` | nonblocking outbound connect on one socket instead of redialing in a loop |
| `unsigned-varint` | `transport/multistream.rs`, `codec/length_prefixed.rs` | multistream framing and bounded length prefixes |
| `snow` | `transport/noise.rs` | Noise XX handshake engine without `ring` |
| `yamux` | `transport/yamux.rs` | standard stream multiplexing |
| `quick-protobuf` | `protocol/identify.rs` | Identify protobuf parsing and encoding |
| `p256` | transport identity manager | choose this if the libp2p identity key is ECDSA P-256 |
| `ed25519-dalek` | transport identity manager | choose this if the libp2p identity key remains Ed25519 |

If `embassy-net` is desired later, add it behind a feature and implement only the `SocketFactory` backend switch.

### 13.2 `rp-runtime`

Likely keep:

- `libp2p`

Likely change:

- remove dependency on JSON request-response for ERP-compatible protocols
- add a custom codec for postcard-encoded NodeHello, Sync, and Announce traffic
- make Kademlia and Gossipsub optional at the profile level

## 14. Testing Plan

Recommended validation sequence:

1. Unit-test NodeHello transcript creation and signature verification.
2. Unit-test peer registry behavior under duplicate and overflow conditions.
3. Unit-test binary codec bounds and reject oversized frames.
4. Host-only integration test: `rp-runtime` embedded-compatible mode talking to a mock ERP transport peer.
5. ERP hardware smoke test: boot, connect to one host node, complete NodeHello, exchange one sync request and one announce.

Recommended first real smoke criteria:

- ERP boots without crashing
- ERP connects to a host bootstrap peer
- ERP completes secure session setup
- ERP emits `PeerConnected`
- ERP requests a bounded range of blocks
- ERP receives and processes at least one valid block

## 15. Common Mistakes To Avoid

1. Do not make `rp-node` own libp2p peer types.
2. Do not treat transport identity and node identity as the same problem.
3. Do not require Kademlia and Gossipsub for the embedded profile.
4. Do not keep JSON in the ERP-compatible wire path.
5. Do not allow unbounded sync responses.
6. Do not emit `PeerConnected` before NodeHello verification succeeds.
7. Do not let `NetworkManager` become a thousand-line god object.
8. Do not try to run a full desktop transport feature set on the ESP first.
9. Do not assume postcard applies to standard libp2p control protocols.
10. Do not implement async dial by looping on `TcpStream::connect`.
11. Do not leave a section at "add a helper type" if the helper also needs a named crate, wire format, or retry rule.

## 16. Definition Of Done For The First Embedded-Compatible Delivery

This plan is successfully implemented when all of the following are true:

- `rp-runtime` can run in an embedded-compatible profile without Kad and Gossipsub
- `erp-runtime` can open and accept secure multiplexed sessions to that profile
- ERP uses bounded sync and announce protocols with binary codecs
- `rp-node` continues to see canonical node peer identity rather than transport identity
- ERP can connect to at least one host node and exchange real traffic
- the implementation stays explicit about memory and concurrency limits

At that point, the ESP is not a thin client.

It is a real node using a deliberately narrow transport profile.