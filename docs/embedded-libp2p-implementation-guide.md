# rust-proof Embedded libp2p Implementation Guide

Date: 2026-04-29  
Status: companion coding workbook for `docs/embedded-libp2p-profile.md`

## 1. What This Guide Is For

The profile document explains the architecture.

This document explains what to code, in what order, with which dependencies, and what must already be true before you move to the next file.

Use this document when you are actively implementing `erp-runtime`.

## 2. Current Repo Baseline

As of this guide:

- Wi-Fi bring-up already lives in `erp-runtime/src/runtime/host.rs` and `erp-runtime/src/network/manager.rs`
- `erp-runtime/src/network/socket/esp_idf.rs` already has a real TCP listener and outbound dial path
- `erp-runtime/src/network/config.rs` already defines `NetworkConfig`, `BootstrapPeer`, and `MultiaddrLite`
- the rest of the network stack is still mostly empty files

That means the first real coding work is not "invent a network architecture".

It is:

1. finish the socket and framing helpers
2. add the peer/session bookkeeping
3. add the wire codecs and protocol modules
4. only then build the transport upgrade path

## 3. Locked Decisions For v1

Do not reopen these while implementing:

- keep node identity separate from transport identity
- keep `rp-node::PeerId = [u8; 32]` as node identity
- use TCP only
- use multistream-select, Noise, Yamux, Identify, NodeHello, Sync, and Announce
- do not require Kad or Gossipsub for ERP compatibility
- use postcard only for rust-proof application protocols and local transcript structs
- use native wire formats for multistream-select, Identify, Noise, and Yamux

## 4. Exact Protocol IDs And Wire Formats

These are the concrete protocol identifiers to use in code.

| Layer | Protocol id | Wire format | Notes |
| --- | --- | --- | --- |
| multistream-select | `/multistream/1.0.0` | unsigned-varint length prefix + UTF-8 line ending in `\n` | first negotiation step on raw and substreams |
| Noise | `/noise` | libp2p Noise XX handshake | use protocol name `Noise_XX_25519_ChaChaPoly_SHA256` |
| Yamux | `/yamux/1.0.0` | Yamux framing | multiplex after secure channel |
| Identify | `/ipfs/id/1.0.0` | protobuf `proto2` | request-response on a substream |
| Ping | `/ipfs/ping/1.0.0` | standard ping bytes | optional, not first milestone |
| NodeHello | `/rust-proof/node-hello/1` | postcard + bounded length prefix | custom protocol |
| Sync | `/rust-proof/sync/1` | postcard + bounded length prefix | custom protocol |
| Announce | `/rust-proof/announce/1` | postcard + bounded length prefix | custom protocol |

Recommended local constants:

```rust
pub const MULTISTREAM_V1: &str = "/multistream/1.0.0";
pub const NOISE_PROTOCOL: &str = "/noise";
pub const YAMUX_PROTOCOL: &str = "/yamux/1.0.0";
pub const IDENTIFY_PROTOCOL: &str = "/ipfs/id/1.0.0";
pub const PING_PROTOCOL: &str = "/ipfs/ping/1.0.0";
pub const NODE_HELLO_PROTOCOL: &str = "/rust-proof/node-hello/1";
pub const SYNC_PROTOCOL: &str = "/rust-proof/sync/1";
pub const ANNOUNCE_PROTOCOL: &str = "/rust-proof/announce/1";
pub const NOISE_PROTOCOL_NAME: &str = "Noise_XX_25519_ChaChaPoly_SHA256";
```

## 5. Dependencies To Add And Why

Already present or already chosen:

- `postcard`
- `embassy-time`
- `socket2`

Add these when you reach the matching step:

- `unsigned-varint`: multistream and bounded length prefixes
- `quick-protobuf`: Identify and Noise handshake payload protobufs
- `snow`: Noise XX handshake engine without `ring`
- `yamux`: standard multiplexer implementation
- `ed25519-dalek`: transport identity key storage and signing unless an existing accepted dependency already covers it

Optional later:

- `slab`: if you decide `Vec<Option<_>>` becomes too awkward

## 6. One Critical Execution-Model Decision

This is the implementation choice that avoids the next hidden blocker.

### 6.1 Recommended v1 model

Use a small number of dedicated session threads for the transport-heavy work.

Reason:

- `std::net::TcpStream` is easy to use on ESP-IDF
- crates in the transport path often expect `futures::io::AsyncRead` and `AsyncWrite`
- `futures::io::AllowStdIo` exists, but it blocks the executor while it performs I/O
- blocking the current shared `LocalExecutor` network/storage/wake thread is the wrong tradeoff

Recommended split:

- keep `NetworkManager` as the async control-plane orchestrator
- listener accept loop and command routing stay there
- each accepted or dialed session is handed off to a dedicated thread
- inside that session thread, convert the accepted `TcpStream` to blocking mode and wrap it with `futures::io::AllowStdIo` only if a third-party crate requires async I/O traits

### 6.2 What not to do in v1

Do not do these unless you intentionally decide to build a real I/O reactor:

- do not implement `futures::io::AsyncRead` or `AsyncWrite` directly over nonblocking `TcpStream` by waking in a spin loop
- do not run `AllowStdIo` on the shared `LocalExecutor`
- do not retry async dial by calling `TcpStream::connect` repeatedly

### 6.3 Current helper already available

`erp-runtime/src/network/socket/esp_idf.rs` should expose these conversions:

```rust
impl EspTcpStream {
    pub fn into_blocking(self) -> Result<TcpStream, RuntimeError>;
    pub fn into_futures_io(self) -> Result<futures::io::AllowStdIo<TcpStream>, RuntimeError>;
}
```

Use `into_blocking()` for your own explicit read/write loops.

Use `into_futures_io()` only inside a dedicated session thread when a crate expects `AsyncRead` / `AsyncWrite`.

## 7. Coding Order

Follow this order exactly.

Do not jump to Noise or Yamux before the earlier steps are real and tested.

### Step 1. Finish `network/bootstrap.rs`

Goal:

- remove bootstrap address resolution as an undefined later task

Put these items in `erp-runtime/src/network/bootstrap.rs`:

```rust
use std::net::SocketAddr;

use crate::network::config::{BootstrapPeer, MultiaddrLite, NetworkConfig};
use crate::runtime::errors::RuntimeError;

pub fn resolve_bootstrap_addr(addr: &MultiaddrLite) -> Result<SocketAddr, RuntimeError>;
pub fn bootstrap_targets(config: &NetworkConfig) -> &[BootstrapPeer];
```

Behavior:

- `Ip4Tcp` maps to `SocketAddr::from(([a, b, c, d], port))`
- `Dns4Tcp` returns a clear config/runtime error until DNS is really implemented

Compile checkpoint:

- `cargo check` with only bootstrap helpers added

### Step 2. Finish `network/peer_registry.rs`

Goal:

- own session allocation and node-peer mapping before sessions exist

Put these items in `erp-runtime/src/network/peer_registry.rs`:

```rust
use alloc::collections::BTreeMap;

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

impl PeerRegistry {
    pub fn new(max_peers: usize) -> Self;
    pub fn alloc(&mut self, transport_peer_id: Vec<u8>) -> Result<SessionId, RuntimeError>;
    pub fn get(&self, id: SessionId) -> Option<&PeerSession>;
    pub fn get_mut(&mut self, id: SessionId) -> Option<&mut PeerSession>;
    pub fn register_node_peer(&mut self, id: SessionId, node_peer_id: [u8; 32]) -> Result<(), RuntimeError>;
    pub fn session_for_node(&self, peer: &[u8; 32]) -> Option<SessionId>;
    pub fn ready_sessions(&self) -> Vec<SessionId>;
    pub fn remove(&mut self, id: SessionId) -> Option<PeerSession>;
}
```

Do not add networking logic here.

This file is bookkeeping only.

### Step 3. Finish the stream helpers in `network/socket/esp_idf.rs`

Goal:

- make the socket layer complete enough that higher layers stop caring about `WouldBlock`

Add these methods to `EspTcpStream`:

```rust
impl EspTcpStream {
    pub async fn read_exact_nonblocking(&mut self, buf: &mut [u8]) -> Result<(), RuntimeError>;
    pub async fn write_all_nonblocking(&mut self, buf: &[u8]) -> Result<(), RuntimeError>;
    pub async fn flush_nonblocking(&mut self) -> Result<(), RuntimeError>;
    pub fn shutdown(&mut self) -> Result<(), RuntimeError>;
}
```

Implementation rule:

- on `WouldBlock` or `Interrupted`, wait with `Timer::after(Duration::from_millis(SOCKET_RETRY_DELAY_MS)).await`
- on EOF during `read_exact_nonblocking`, return a real runtime error

Do not make `codec` or `protocol` modules handle raw `WouldBlock`.

### Step 4. Fill `network/codec/postcard_codec.rs`

Goal:

- centralize postcard encode/decode instead of scattering it everywhere

Put these items in `erp-runtime/src/network/codec/postcard_codec.rs`:

```rust
use core::marker::PhantomData;
use serde::{Serialize, de::DeserializeOwned};

use crate::runtime::errors::RuntimeError;

pub trait ValueCodec<T> {
    fn encode(item: &T) -> Result<Vec<u8>, RuntimeError>;
    fn decode(bytes: &[u8]) -> Result<T, RuntimeError>;
}

pub struct PostcardCodec<T>(PhantomData<T>);

impl<T> ValueCodec<T> for PostcardCodec<T>
where
    T: Serialize + DeserializeOwned,
{
    fn encode(item: &T) -> Result<Vec<u8>, RuntimeError>;
    fn decode(bytes: &[u8]) -> Result<T, RuntimeError>;
}
```

Only use this for custom rust-proof protocols and local transcript structs.

### Step 5. Fill `network/codec/length_prefixed.rs`

Goal:

- keep bounded framing logic in one place

Add `unsigned-varint` first.

Put these items in `erp-runtime/src/network/codec/length_prefixed.rs`:

```rust
use crate::runtime::errors::RuntimeError;

pub fn encode_length_prefixed(payload: &[u8], max_len: u32) -> Result<Vec<u8>, RuntimeError>;
pub fn decode_length_prefix(input: &[u8], max_len: u32) -> Result<(usize, &[u8]), RuntimeError>;
```

Then add stream helpers over `EspTcpStream` only after the pure byte helpers work.

### Step 6. Create `network/transport_identity.rs`

This file does not exist yet.

Create it here:

- `erp-runtime/src/network/transport_identity.rs`

Then export it from `erp-runtime/src/network/mod.rs`.

Put these items in the new file:

```rust
pub trait TransportIdentity {
    fn transport_peer_id(&self) -> Vec<u8>;
    fn public_key_bytes(&self) -> Vec<u8>;
    fn sign(&self, message: &[u8]) -> Result<Vec<u8>, RuntimeError>;
}

pub struct TransportIdentityRecord {
    pub secret_key: [u8; 32],
}

pub struct TransportIdentityManager {
    // NVS-backed Ed25519 transport keypair
}

impl TransportIdentityManager {
    pub fn load_or_create(/* nvs deps */) -> Result<Self, RuntimeError>;
    pub fn peer_id_bytes(&self) -> &[u8];
    pub fn public_key_bytes(&self) -> &[u8];
    pub fn sign(&self, message: &[u8]) -> Result<Vec<u8>, RuntimeError>;
}
```

Storage rule:

- store one fixed-size Ed25519 secret key or seed in NVS
- derive public key and transport peer id at boot

### Step 7. Fill `protocol/node_hello.rs`

Goal:

- make node/transport identity binding explicit before any transport registration occurs

Put these items in `erp-runtime/src/network/protocol/node_hello.rs`:

```rust
pub struct NodeHello { /* fields from the profile doc */ }
pub struct NodeHelloResponse { /* fields from the profile doc */ }
pub enum NodeHelloRejectReason { /* variants from the profile doc */ }
pub struct PeerCapabilities { /* supports_sync_v1 etc */ }

pub struct NodeHelloTranscript<'a> {
    pub version: u16,
    pub node_peer_id: [u8; 32],
    pub transport_peer_id: &'a [u8],
    pub max_frame_len: u32,
    pub max_blocks_per_chunk: u16,
    pub capabilities: &'a PeerCapabilities,
}

pub struct VerifiedPeer {
    pub node_peer_id: [u8; 32],
    pub node_public_key: Vec<u8>,
    pub transport_peer_id: Vec<u8>,
    pub max_frame_len: u32,
    pub max_blocks_per_chunk: u16,
    pub capabilities: PeerCapabilities,
}

pub struct NodeHelloBuilder<'a> { /* refs to node + transport identity */ }
pub struct NodeHelloVerifier;
```

Rules:

- sign `NodeHelloTranscript`, not the framed `NodeHello` bytes
- verify `node_peer_id == hash(node_public_key)` using the project rule
- verify `transport_peer_id` equals the authenticated transport peer for the session

### Step 8. Fill `protocol/sync.rs` and `protocol/announce.rs`

Goal:

- wrap the existing `rp-node` message types instead of inventing parallel wire types

In `erp-runtime/src/network/protocol/sync.rs`, add helpers like:

```rust
pub fn encode_sync_request(req: &rp_node::network::message::SyncRequest, max_len: u32) -> Result<Vec<u8>, RuntimeError>;
pub fn decode_sync_request(frame: &[u8], max_len: u32) -> Result<rp_node::network::message::SyncRequest, RuntimeError>;
pub fn encode_sync_response(resp: &rp_node::network::message::SyncResponse, max_len: u32) -> Result<Vec<u8>, RuntimeError>;
pub fn decode_sync_response(frame: &[u8], max_len: u32) -> Result<rp_node::network::message::SyncResponse, RuntimeError>;
```

In `erp-runtime/src/network/protocol/announce.rs`, add the same shape for `AnnounceRequest` and `AnnounceResponse`.

Do not duplicate the existing message definitions if `rp-node` already owns them.

### Step 9. Fill `transport/multistream.rs`

Goal:

- make protocol negotiation explicit and testable before Noise exists

Add `unsigned-varint` first.

Put these items in `erp-runtime/src/network/transport/multistream.rs`:

```rust
pub const MULTISTREAM_V1: &str = "/multistream/1.0.0";

pub async fn write_protocol<S>(stream: &mut S, protocol: &str) -> Result<(), RuntimeError>;
pub async fn read_protocol<S>(stream: &mut S, max_len: usize) -> Result<String, RuntimeError>;
pub async fn dialer_select<S>(stream: &mut S, protocol: &str) -> Result<(), RuntimeError>;
pub async fn listener_select<S>(stream: &mut S, supported: &[&str]) -> Result<String, RuntimeError>;
```

V1 rule:

- do not implement `V1Lazy` first
- do plain V1 and get it interoperating first

Exact behavior:

- both sides send `/multistream/1.0.0\n` first
- dialer proposes a protocol string
- listener echoes it back on success or sends `na\n` on failure

### Step 10. Fill `protocol/identify.rs`

Goal:

- implement one standard control protocol end to end before tackling Noise/Yamux integration details

Add `quick-protobuf` first.

Use this exact protobuf schema from the Identify spec:

```protobuf
syntax = "proto2";
message Identify {
  optional string protocolVersion = 5;
  optional string agentVersion = 6;
  optional bytes publicKey = 1;
  repeated bytes listenAddrs = 2;
  optional bytes observedAddr = 4;
  repeated string protocols = 3;
}
```

Put these items in `erp-runtime/src/network/protocol/identify.rs`:

```rust
pub struct IdentifyInfo {
    pub protocol_version: String,
    pub agent_version: String,
    pub listen_addrs: Vec<MultiaddrLite>,
    pub supported_protocols: Vec<String>,
    pub observed_addr: Option<std::net::SocketAddr>,
    pub transport_peer_id: Vec<u8>,
}

pub fn encode_identify(info: &IdentifyInfo) -> Result<Vec<u8>, RuntimeError>;
pub fn decode_identify(bytes: &[u8]) -> Result<IdentifyInfo, RuntimeError>;
```

V1 simplification:

- keep only the fields you actually consume
- ignore unknown protobuf fields

### Step 11. Fill `transport/noise.rs`

Goal:

- build a secure channel that proves remote transport identity before NodeHello runs

Add `snow` and `quick-protobuf` first.

Use these exact libp2p Noise facts:

- multistream protocol id: `/noise`
- Noise protocol name: `Noise_XX_25519_ChaChaPoly_SHA256`
- only XX is guaranteed for interop

Noise handshake payload schema to parse or emit:

```protobuf
syntax = "proto2";
message NoiseExtensions {
  repeated bytes webtransport_certhashes = 1;
  repeated string stream_muxers = 2;
}

message NoiseHandshakePayload {
  optional bytes identity_key = 1;
  optional bytes identity_sig = 2;
  optional NoiseExtensions extensions = 4;
}
```

Put these items in `erp-runtime/src/network/transport/noise.rs`:

```rust
pub struct NoiseUpgradeOutput<S> {
    pub stream: S,
    pub remote_transport_peer_id: Vec<u8>,
    pub remote_transport_public_key: Vec<u8>,
}

pub async fn upgrade_outbound<S>(stream: S, identity: &TransportIdentityManager) -> Result<NoiseUpgradeOutput<S>, RuntimeError>;
pub async fn upgrade_inbound<S>(stream: S, identity: &TransportIdentityManager) -> Result<NoiseUpgradeOutput<S>, RuntimeError>;
```

V1 compatibility rule:

- do not implement inlined muxer negotiation first
- tolerate `extensions.stream_muxers` if present
- after Noise succeeds, do a normal secured multistream negotiation of `/yamux/1.0.0`

### Step 12. Fill `transport/yamux.rs`

Goal:

- turn one secure byte stream into multiple protocol substreams

Add `yamux` first.

Protocol id:

- `/yamux/1.0.0`

Put these items in `erp-runtime/src/network/transport/yamux.rs`:

```rust
pub struct YamuxSession<M> {
    pub muxer: M,
}

pub fn upgrade_outbound<S>(stream: S) -> Result<YamuxSession</* muxer type */>, RuntimeError>;
pub fn upgrade_inbound<S>(stream: S) -> Result<YamuxSession</* muxer type */>, RuntimeError>;
```

Start simple:

- one fresh substream per Identify request
- one fresh substream per NodeHello exchange
- one fresh substream per Sync request
- one fresh substream per Announce request

Do not invent a custom long-lived control channel in v1.

### Step 13. Fill `network/session.rs`

Goal:

- make the connection lifecycle explicit in one place

Put these items in `erp-runtime/src/network/session.rs`:

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

impl<S> SessionWorker<S> {
    pub fn run(self) -> Result<(), RuntimeError>;
}
```

Recommended first concrete flow:

- use a dedicated session thread, not the shared executor
- if the code path uses third-party async I/O traits, convert the stream with `into_futures_io()` inside that thread
- run this order:
  - multistream for `/noise`
  - Noise XX handshake
  - multistream for `/yamux/1.0.0`
  - Yamux startup
  - Identify substream
  - NodeHello substream
  - ready loop for Sync and Announce

### Step 14. Rewrite `network/manager.rs`

Goal:

- turn the current stub network manager into an orchestrator only

What `NetworkManager` should own:

- Wi-Fi readiness
- listener startup
- bootstrap dialing schedule
- `PeerRegistry`
- session thread spawn and teardown
- routing of `NetworkCommand` to already-ready sessions

What `NetworkManager` should not own:

- raw protocol parsing
- Noise bytes
- Yamux frame handling
- NodeHello transcript construction

Concrete next rewrite steps:

1. keep the existing Wi-Fi ownership
2. add `NetworkConfig`
3. add `EspSocketFactory`
4. bind a listener on `listen_port`
5. spawn one accept loop
6. spawn one bootstrap dial loop
7. replace log-only `SendFrame` / `BroadcastFrame` handling with lookups into `PeerRegistry`

## 8. Compile Checkpoints

Run these after each stage, not just at the end.

1. after Steps 1 to 3: `cargo check`
2. after Steps 4 to 6: `cargo check`
3. after Steps 7 to 10: `cargo check`
4. after Steps 11 to 14: `cargo check`

Then add one host-side interop target:

- `rp-runtime` in embedded-compatible mode should accept TCP + Noise + Yamux + Identify from ERP before you add Sync/Announce traffic

## 9. Do Not Guess These

If you hit one of these questions, the answer is already fixed.

- Do application protocols use postcard? Yes.
- Do multistream, Identify, Noise, or Yamux use postcard? No.
- Is `esp_netif` the socket object? No.
- Is looping on `TcpStream::connect` a valid async dial? No.
- Should `WouldBlock` escape `SocketFactory`? No.
- Should ERP do inlined muxer negotiation inside Noise first? No, not in v1.
- Should Identify be custom JSON or postcard? No, use the standard protobuf schema.
- Should NodeHello be signed as a whole framed message? No, sign the explicit transcript struct.
- Should `NetworkManager` parse protocols directly? No.

## 10. Definition Of “Nothing Standing In Your Way”

You are ready to write real transport code when all of these are true:

- bootstrap address resolution is implemented
- peer/session registry exists
- `EspTcpStream` can do bounded read/write helpers and blocking conversion
- codec layer exists for postcard and length prefixes
- transport identity storage exists
- NodeHello structs and verifier exist
- multistream constants and helpers exist
- Identify protobuf schema is checked in or generated
- Noise and Yamux dependencies are chosen and added intentionally
- session worker file owns the full connection lifecycle

At that point the remaining work is implementation, not architecture archaeology.