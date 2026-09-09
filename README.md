# KCC20 live

A hands-on KCC20 contract and offline Rust transaction example built with [Argent](https://github.com/argent-lang/argent) for Kaspa covenants.

## Run

```sh
cargo run --locked --bin kcc20
```

Run from the repository root with Rust installed; `rust-toolchain.toml` selects Rust 1.94.1. The example compiles `contracts/kcc20.ag` into `build/`, builds a transaction with `argent-runtime`, validates its input scripts locally, and prints its ID and input/output counts.

## Current example

Alice contributes 11 tokens to Bob's existing token UTXO using the amount-threshold borrow path:

| Owner | Before | After | Authorization |
| --- | ---: | ---: | --- |
| Bob | 200 | 211 | Borrow path; increase must exceed 10 |
| Alice | 100 | 89 | Signed `transfer_delegator` |

Bob's input leads the transfer. His output preserves ownership, borrow policy, and native value; Alice signs the delegate input. The total remains 300 tokens across two inputs and two outputs.

This is an offline example with deterministic demo keys, synthetic outpoints, and a synthetic covenant ID. It does not fund or submit a transaction.

## Files

- [`contracts/kcc20.ag`](contracts/kcc20.ag): token state, owner authorization, transfer and delegate logic, and borrow policies. This version allows up to two delegates and two outputs.
- [`src/bin/kcc20/main.rs`](src/bin/kcc20/main.rs): the executable threshold-borrow example.
- [`src/lib.rs`](src/lib.rs): key, signing, transaction-reporting, and TN10 helpers. The example uses the offline helpers.

Argent compiler and runtime revisions are pinned together in `Cargo.toml`. Keep the Kaspa dependencies aligned with the revision used by Argent, and retain `Cargo.lock` for reproducible builds.
