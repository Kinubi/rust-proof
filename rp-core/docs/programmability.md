# Programmability & Smart Contracts

Currently, the `rust-proof` blockchain supports basic state transitions such as native token transfers and staking operations. To evolve the chain into a fully programmable platform, we can introduce smart contract capabilities.

## 1. WebAssembly (Wasm) Virtual Machine
Integrating a Wasm runtime allows developers to write smart contracts in Rust (or other Wasm-compilable languages), compile them to WebAssembly, and deploy them on-chain.
*   **Runtimes:** We can embed runtimes like `wasmi` (interpreter, good for deterministic execution) or `wasmtime` (JIT compiled, high performance).
*   **State Access:** The VM needs a well-defined host API to read from and write to the blockchain's state trie.
*   **Gas Metering:** Wasm execution must be strictly metered to prevent infinite loops and ensure validators are compensated for computational work.

## 2. EVM Compatibility
To tap into the existing Ethereum ecosystem, we could implement the Ethereum Virtual Machine (EVM).
*   **SputnikVM / Revm:** Rust libraries like `revm` can be integrated to execute Solidity smart contracts.
*   **Tooling:** This allows users to interact with our chain using standard tools like MetaMask, Hardhat, and Foundry.
*   **Account Model:** We would need to ensure our account model maps cleanly to the EVM's 160-bit address space and nonce/balance structure.

## 3. Native Custom Assets
If a full Turing-complete VM is too heavy or complex, we can add protocol-level support for custom tokens.
*   **Built-in Logic:** Similar to Algorand Standard Assets (ASAs) or Cardano native tokens, users can issue new tokens via a special `CreateAsset` transaction.
*   **Performance:** Native assets are processed directly by the core state machine, making them significantly faster and cheaper to transfer than smart contract-based tokens (like ERC-20).
