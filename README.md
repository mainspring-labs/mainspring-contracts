<h1 align="center">Mainspring</h1>

<p align="center">
  Timelocked, rollback-capable fleet upgrades for Soroban contracts, with an exit right for every instance.
</p>

<p align="center">
  <a href="https://github.com/mainspring-labs/mainspring-contracts/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/mainspring-labs/mainspring-contracts/actions/workflows/ci.yml/badge.svg"></a>
  <img alt="soroban-sdk 28" src="https://img.shields.io/badge/soroban--sdk-28.0.0-blue">
  <img alt="Protocol 28" src="https://img.shields.io/badge/Stellar-Protocol%2028-black">
  <img alt="License: Apache-2.0" src="https://img.shields.io/badge/License-Apache_2.0-blue.svg">
</p>

---

## The problem

Protocol 28 (live on mainnet since 16 September 2026) added [CAP-85](https://github.com/stellar/stellar-protocol/blob/master/core/cap-0085.md): a contract can run code that another contract stores as a Wasm hash under a tag. Point a thousand escrows, vaults or pools at one tag, change the tag, and all of them run the new code. That's one write instead of a thousand upgrade transactions.

It also means whoever controls the owner contract can change the code behind every instance at any moment. CAP-85 leaves the rules for that power to each team. Every team running a factory now has to write its own owner contract from scratch.

Mainspring is that owner contract, written once and tested:

- **Slow.** New code waits out a per-tag timelock. The timelock can be raised but never lowered.
- **Visible.** Every proposal, execution, cancellation and rollback emits an event with the Wasm hash and when it takes effect.
- **Escapable.** An instance's user can `pin` it to the code it runs today before a proposed upgrade lands. Pinned instances are untouched by later upgrades.
- **Recoverable.** A guardian can cancel a pending upgrade or roll back one step immediately. The guardian can never introduce code that didn't pass the timelock.

## How it fits together

```
  admin (multisig / DAO)             guardian (ops key)
        │ create_tag, propose               │ cancel, rollback
        ▼                                   ▼
  ┌──────────────────────── spring ────────────────────────┐
  │ tag "tipjar" ──► Wasm hash (CAP-85 executable ref)     │
  │ timelock · pending proposal · version history          │
  └────────────────────────────────────────────────────────┘
        ▲ runs from                         ▲ runs from
  ┌───────────┐  ┌───────────┐  ┌───────────┐
  │ instance  │  │ instance  │  │ instance  │ ◄── deployed by factory
  └───────────┘  └───────────┘  └─────┬─────┘
                                      │ pin()  (owner of this instance)
                                      ▼
                              fixed Wasm hash, leaves the fleet
```

| Crate | Role |
|---|---|
| [`contracts/spring`](contracts/spring) | Owner contract. Holds tags, timelocks, proposals, version history, rollback. |
| [`contracts/factory`](contracts/factory) | Deploys instances bound to one spring tag, with deployer-bound salts, and lists them. |
| [`crates/mainspring-instance`](crates/mainspring-instance) | Library for instance contracts: `attach`, `pin`, `extend_ttl`. |
| [`examples/tipjar-v1`](examples/tipjar-v1), [`tipjar-v2`](examples/tipjar-v2) | A minimal fleet instance and its upgrade, used by the tests. |

The full contract specification, including every function, auth rule, error code and event, is in [`docs/contract-spec.md`](docs/contract-spec.md).

### The upgrade lifecycle

1. The admin calls `create_tag("tipjar", v1_hash, 259200)`, setting a 3-day timelock.
2. Users deploy their own instances through the factory. Each one runs v1 through the tag.
3. The admin calls `propose("tipjar", v2_hash)`. An `upgrade_proposed` event carries the `eta`.
4. Any instance owner who doesn't want v2 calls `pin()` before the `eta`.
5. After the `eta`, **anyone** can call `execute("tipjar")`. Every unpinned instance now runs v2.
6. If v2 misbehaves, the guardian calls `rollback`. Unpinned instances return to v1 at once.

### Why use the factory

CAP-85 notes that custom accounts written before Protocol 28 may refuse to authorize creating a contract with an external-ref executable. The factory is the deployer, not the user, so a user's account only authorizes a plain `deploy` call. [`contracts/factory/src/test.rs`](contracts/factory/src/test.rs) shows such an account failing when it deploys directly and succeeding through the factory.

## Quick start

Requirements: Rust 1.91 or newer with the `wasm32v1-none` target, and [stellar-cli](https://github.com/stellar/stellar-cli) 25.2.0 or newer (CI uses 28.1.0). soroban-sdk 28 refuses to build contracts without it.

```bash
git clone https://github.com/mainspring-labs/mainspring-contracts
cd mainspring-contracts
make test     # builds the example Wasm, then runs every test
make build    # builds spring and factory Wasm
make lint     # rustfmt + clippy -D warnings
```

Without `make`:

```bash
stellar contract build --package tipjar-v1
stellar contract build --package tipjar-v2
cargo test --workspace
```

## Status

v0.1, unaudited. Protocol 28 and soroban-sdk 28.0.0 are both a few weeks old. Don't put real value behind a spring until it has been reviewed. See [SECURITY.md](SECURITY.md).

## Contributing

Issues are labelled by area (`spring`, `factory`, `instance`, `docs`) and complexity. Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a PR: one logical change per commit, conventional commit messages, and `make lint test` passing.

## Maintainers

| Name | GitHub |
|---|---|
| Dillon Ofili | [@0dillon](https://github.com/0dillon) |

## Contributors

<a href="https://github.com/mainspring-labs/mainspring-contracts/graphs/contributors">
  <img src="https://contrib.rocks/image?repo=mainspring-labs/mainspring-contracts" alt="Contributors" />
</a>

## License

[Apache-2.0](LICENSE)
