# rust-proof workspace

`rust-proof` is split into a core node, a desktop client, and an embedded ESP32 client.

## Workspace layout

- `rust-proof-core/`: core blockchain crate and node binary
- `rust-proof-client/`: desktop or PC client scaffold
- `erp-client/`: embedded ESP32 firmware client

## Why this layout works

- `rust-proof-core` keeps the chain, node, networking, and persistence logic together.
- `rust-proof-client` can grow into a wallet or operator CLI without inheriting embedded toolchain constraints.
- `erp-client` stays isolated because it has its own target, linker, flashing flow, and size tuning.

The workspace deliberately keeps the embedded client out of `default-members`, so plain desktop commands such as `cargo check`, `cargo test`, and `cargo run -p rust-proof-client` do not require the ESP-IDF toolchain.

## Common commands

```sh
cargo check
cargo test -p rust-proof
cargo run -p rust-proof
cargo run -p rust-proof-client -- --help
cargo build -p erp
```

## Next structural step, if needed

If the desktop and embedded clients start sharing transaction or wire types that should not depend on the full node crate, add a fourth crate such as `rust-proof-protocol` or `rust-proof-types`. For now, this three-part split is a solid base.