# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Development

Requires Rust 1.85+ (edition 2024).

```bash
cargo build                    # Debug build
cargo build --release          # Release build
cargo check                    # Type-check without building
cargo clippy                   # Lint
cargo test                     # Run all tests
cargo test -p bunzo-pool       # Run tests for a single crate
cargo test test_name           # Run a single test by name
```

The workspace default member is `bin/bunzo`, so `cargo run` launches the bundler binary.

## Architecture

Bunzo is an ERC-4337 bundler targeting EntryPoint v0.7. It receives UserOperations via JSON-RPC, validates and pools them, then builds bundles and submits them on-chain.

### Crate dependency graph

```
bin/bunzo (binary entry point, CLI via clap)
├── bunzo-rpc        → JSON-RPC server (jsonrpsee)
├── bunzo-builder    → Bundle proposal, submission FSM, fee escalation
├── bunzo-pool       → In-memory mempool, validation, reputation (ERC-7562)
├── bunzo-provider   → Alloy-based EVM & EntryPoint contract interface
├── bunzo-signer     → Transaction signing, signer pool with leasing
└── bunzo-types      → Shared domain types (no deps on other bunzo crates)
```

### Key data flow

1. **Inbound**: `eth_sendUserOperation` → `Validator::validate_sync()` → `OperationPool::add()`
2. **Bundle building**: `BundleProposer::make_bundle()` pulls best ops from pool → simulates via EntryPoint → packs valid ops
3. **Submission**: `StrategyRouter` tries strategies in order (direct → conditional) → `TransactionTracker` monitors pending tx
4. **Lifecycle FSM** (in `BundlerTask`): `Idle → Building → Submitting → Pending → [Escalating] → Confirming → Confirmed`

### Core abstractions (trait-based, all in `crates/provider/src/traits.rs`)

- **`EvmProvider`** — Low-level EVM RPC (call, get_logs, send_raw_transaction, etc.)
- **`EntryPointProvider`** — EntryPoint contract (simulate_validation, encode_handle_ops, get_user_op_hash)
- **`BundleHandler`** — Bundle execution (call_handle_ops, build_handle_ops_tx)

All provider traits have Alloy implementations in `crates/provider/src/alloy/`.

### Pool ordering

Operations are ordered by `(gas_price DESC, sequence ASC, hash ASC)` in a BTreeSet. Replacement uses `UserOperationId` (sender, nonce) as key with a configurable fee bump requirement (default 10%).

### Submission strategies (`crates/builder/src/strategy/`)

- **`DirectSubmissionStrategy`** — Standard `send_raw_transaction`
- **`ConditionalSubmissionStrategy`** — `eth_sendRawTransactionConditional` with storage slot checks (WIP: submit/cancel are `todo!()`)
- **`StrategyRouter`** — Tries strategies in order, fails over on `StrategyFailed`

### Fee escalation (`crates/builder/src/tracker.rs`)

Two strategies: `Linear` (fixed % bumps per round) and `NetworkTracking` (queries network gas price + premium).

### Reputation system (`crates/pool/src/reputation.rs`)

ERC-7562 entity reputation tracking. Entities (Account, Paymaster, Factory, Aggregator) transition through `Ok → Throttled → Banned` based on inclusion ratios.

## Conventions

- All I/O is abstracted behind traits for testability (mock implementations).
- Async throughout using tokio; `async_trait` for trait objects.
- Error types are per-crate using `thiserror` (e.g., `ValidationError`, `PoolError`, `BuilderError`).
- Event bus pattern via tokio broadcast channels (`bunzo-types/src/event.rs`).
- Structured logging via `tracing`; metrics via `metrics` crate.
- Concurrency: `parking_lot` RwLocks, atomics where possible.
- Contract bindings generated via `alloy_sol_macro` in `crates/provider/src/contracts.rs`.
