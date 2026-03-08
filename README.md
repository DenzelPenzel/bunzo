# Bunzo
    
An ERC-4337 bundler written in Rust.

Bunzo receives `UserOperation` objects via a JSON-RPC interface, validates and
pools them, then periodically builds bundles and submits them on-chain through
the EntryPoint contract. It targets EntryPoint v0.7.

## Crates

| Crate | Description |
|-------|-------------|
| `bunzo-types` | Core types: `UserOperation`, `ChainSpec`, entities, gas, events |
| `bunzo-pool` | In-memory operation pool, validation, reputation tracking |
| `bunzo-provider` | Alloy-based EVM/EntryPoint providers, gas oracle |
| `bunzo-builder` | Bundle proposer, submission strategies (direct / conditional), transaction tracker |
| `bunzo-rpc` | JSON-RPC server (`eth_sendUserOperation`, `eth_estimateUserOperationGas`, etc.) |
| `bunzo-signer` | Transaction signing (local key, signer manager with leasing) |

## Building

Requires Rust 1.85+.

```
cargo build --release
```

## Running

```
bunzo \
  --node-url http://localhost:8545 \
  --private-key <hex> \
  --entry-point 0x0000000071727De22E5E9d8BAf0edAc6f37da032
```

Key flags:

- `--rpc-addr` — Listen address (default `127.0.0.1:3000`)
- `--node-url` — Ethereum node RPC URL
- `--chain-id` — Chain ID (`0` = auto-detect)
- `--private-key` — Bundler signer key (omit for RPC-only mode)
- `--max-pool-size` — Max pooled operations (default `4096`)
- `--log` — Log filter (default `info`)

All flags can also be set via `BUNZO_*` environment variables.

## License

MIT
