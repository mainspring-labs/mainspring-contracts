# Mainspring — Contract Specification (Phase 5)

Repo: `mainspring-labs/mainspring-contracts`
Status: implemented in v0.1, 2026-10-05

## What this is

Protocol 28 (CAP-85, live on mainnet 2026-09-16) lets a contract run code that is stored as a Wasm hash in a different contract's storage, under a string tag. Every contract that points at that tag runs whatever hash the entry holds. Changing the entry upgrades all of them in one write.

The contract that owns the entry can change the code behind every instance at any time. CAP-85 leaves the rules for that power to each team. Mainspring is a standard, reviewed owner contract plus a factory and an instance library. Together they make that power slow, visible and escapable:

- **Slow:** new code waits out a per-tag timelock before it goes live.
- **Visible:** every proposal, execution, cancellation and rollback emits an event with the Wasm hash.
- **Escapable:** an instance's user can pin their instance to the code it runs today before a proposal executes.

## Verified platform facts (soroban-sdk 28.0.0, released 2026-09-18)

These come from the SDK source at tag `v28.0.0`, not from memory.

| Fact | Source |
|---|---|
| `env.executable_refs().set(&tag: &String, &wasm_hash: &BytesN<32>)` creates or updates the ref entry owned by the current contract | `soroban-sdk/src/executable_refs.rs` |
| `set` panics if the Wasm has not been uploaded | same |
| Ref entries are always persistent and **can never be removed**. They can be archived and restored. | same |
| `executable_refs().get / has / extend_ttl / extend_ttl_with_limits` | same |
| Deploy an instance: `env.deployer().with_current_contract(salt).deploy_contract(ContractExecutable::ExternalRef(ContractExecutableRef { owner, tag }), args)` | `deploy.rs` |
| An existing contract can switch itself: `env.deployer().update_current_contract(ContractExecutable::ExternalRef(..))` or back with `ContractExecutable::Wasm(hash)`. This takes effect after the invocation finishes. | `deploy.rs` |
| `address.executable()` returns the **resolved** `Executable::Wasm(hash)` for ref-based contracts | `address.rs`, `tests/contract_executable_ref.rs` |
| `env.deployer().extend_ttl(addr, threshold, extend_to)` extends instance, code **and** the ref entry | `deploy.rs` |
| `ConstructorArgs` is implemented for `Vec<Val>` | `constructor_args.rs` |
| Toolchain: Rust ≥ 1.91.0, target `wasm32v1-none`, stellar-cli ≥ 25.2.0 | workspace `Cargo.toml`, SDK 28 release notes |
| Custom accounts that read the auth context receive `ContractExecutable::ExternalRef` for ref-based deploys | `tests/contract_executable_ref.rs` |

## Workspace layout

```
mainspring-contracts/
├── Cargo.toml                    # workspace, edition 2021, soroban-sdk = "28.0.0"
├── rust-toolchain.toml           # stable, targets = ["wasm32v1-none"]
├── crates/
│   └── mainspring-instance/      # library: attach, pin, extend_ttl helpers + events
├── contracts/
│   ├── spring/                   # owner contract: tags, timelock, rollback
│   └── factory/                  # deploys and records fleet instances
└── examples/
    ├── tipjar-v1/                # example instance, uses mainspring-instance
    └── tipjar-v2/                # same storage, one added function (upgrade target)
```

## Dependency and build order

```
mainspring-instance (lib)
        │
        ▼
tipjar-v1, tipjar-v2 (example Wasm, needed by every test below)
        │
        ▼
spring  ──────────►  factory  ──────────►  integration tests (full flow)
```

`spring` does not call `factory`. `factory` does not call `spring` at runtime. It only stores the spring's address and tag and passes them to the deployer. The order follows test needs: spring tests deploy example instances, and factory tests need a deployed spring with a tag.

## User flow (every function below maps to a step here)

1. The team uploads `tipjar-v1.wasm`, deploys `spring(admin = team multisig, guardian = ops key)`, and calls `create_tag("tipjar", v1_hash, 259200 /* 3 days */)`.
2. The team deploys `factory(spring, "tipjar", admin = None)`.
3. A user calls `factory.deploy(user, salt, args)` and gets their own tip jar, which runs v1 through the ref.
4. The team uploads v2 and calls `spring.propose("tipjar", v2_hash)`. An `UpgradeProposed` event fires with `eta`.
5. Watchers (Mainspring CLI or indexer, in the app repo) alert instance users. A user who does not want v2 calls `pin()` on their tip jar before `eta`. Their instance detaches onto the v1 hash and stays on it.
6. After `eta`, anyone calls `spring.execute("tipjar")`. Every unpinned instance now runs v2.
7. If v2 is broken, the guardian calls `spring.rollback(guardian, "tipjar")`. Every unpinned instance returns to v1 immediately.
8. Anyone can call `spring.extend_ttl("tipjar")` so the ref entry never archives under the fleet.

---

## Contract 1: `spring` (owner contract)

Single responsibility: own executable-ref tags and gate every change to them.

### Storage

```rust
#[contracttype]
pub enum DataKey {
    Admin,                  // instance: Address
    PendingAdmin,           // instance: Address (absent when none)
    Guardian,               // instance: Address
    Tag(String),            // persistent: TagState
    Pending(String),        // persistent: Proposal (absent when none)
    Version(String, u32),   // persistent: VersionRecord
}

#[contracttype]
pub struct TagState {
    pub min_delay: u64,                 // seconds; only ever increases
    pub version: u32,                   // current version number, starts at 1
    pub current: BytesN<32>,            // mirrors the ref entry value
    pub previous: Option<BytesN<32>>,   // rollback target; None after a rollback
}

#[contracttype]
pub struct Proposal {
    pub wasm_hash: BytesN<32>,
    pub proposed_at: u64,               // ledger timestamp
    pub eta: u64,                       // proposed_at + min_delay
}

#[contracttype]
pub struct VersionRecord {
    pub wasm_hash: BytesN<32>,
    pub activated_at: u64,
    pub kind: VersionKind,
}

#[contracttype]
pub enum VersionKind { Created, Upgrade, Rollback }
```

The ref entry itself lives under the protocol-defined `ExecutableTag` key through `env.executable_refs()`. `TagState.current` mirrors it.

The pending proposal is stored under its own `Pending(tag)` key rather than as an `Option<Proposal>` field on `TagState`. soroban-sdk 28.0.0's testutils can't convert a `contracttype` struct field of type `Option<user-defined type>` to `ScVal`, so that layout doesn't compile in tests.

Constants:

| Name | Value | Why |
|---|---|---|
| `MAX_DELAY` | `2_592_000` (30 days) | Caps a mistyped delay that would freeze a tag |
| `TAG_MAX_LEN` | `64` | Bounds storage key size |
| `TTL_THRESHOLD` | `518_400` ledgers (~30 days at 5s) | Extend when below this |
| `TTL_EXTEND_TO` | `3_110_400` ledgers (~180 days) | Target TTL |

### Errors

```rust
#[contracterror]
#[repr(u32)]
pub enum SpringError {
    NotAuthorized       = 1,
    TagExists           = 2,
    TagNotFound         = 3,
    ProposalPending     = 4,
    NoProposal          = 5,
    TooEarly            = 6,
    SameHash            = 7,
    NothingToRollBack   = 8,
    DelayTooLong        = 9,
    DelayNotIncreased   = 10,
    NoPendingAdmin      = 11,
    InvalidTag          = 12,
}
```

### Functions

| Function | Auth | Behaviour | Flow step |
|---|---|---|---|
| `__constructor(env, admin: Address, guardian: Address)` | none (deploy-time) | Store admin and guardian. | 1 |
| `create_tag(env, tag: String, wasm_hash: BytesN<32>, min_delay: u64) -> Result<(), SpringError>` | `admin.require_auth()` | Tag length 1..=64, else `InvalidTag`. Tag already exists → `TagExists`. `min_delay > MAX_DELAY` → `DelayTooLong`. Calls `executable_refs().set` (panics if the Wasm isn't uploaded). Writes `TagState { version: 1, previous: None }` and `Version(tag, 1)` with kind `Created`. Extends TTLs. | 1 |
| `propose(env, tag: String, wasm_hash: BytesN<32>) -> Result<u64, SpringError>` | `admin.require_auth()` | `TagNotFound`, `ProposalPending` if one exists, `SameHash` if equal to `current`. Stores `Pending(tag)` with `eta = now + min_delay`. Returns `eta`. | 4 |
| `execute(env, tag: String) -> Result<u32, SpringError>` | **none** (permissionless after eta) | `NoProposal`, `TooEarly` if `now < eta`. Calls `executable_refs().set(tag, hash)`, sets `previous = current`, `current = hash`, `version += 1`, writes `Version` with kind `Upgrade`, removes `Pending(tag)`. Returns the new version. | 6 |
| `cancel(env, caller: Address, tag: String) -> Result<(), SpringError>` | `caller.require_auth()`; caller must be admin or guardian | `NoProposal` if none. Removes `Pending(tag)`. | 5 (team changes mind) |
| `rollback(env, caller: Address, tag: String) -> Result<u32, SpringError>` | `caller.require_auth()`; caller must be admin or guardian | `NothingToRollBack` if `previous` is `None`. Sets the ref to `previous`, `current = previous`, `previous = None`, `version += 1`, writes `Version` with kind `Rollback`. Also removes any `Pending(tag)`. **No timelock**, because the target hash already passed one. Returns the new version. | 7 |
| `increase_min_delay(env, tag: String, new_delay: u64) -> Result<(), SpringError>` | `admin.require_auth()` | `DelayNotIncreased` if `new_delay <= min_delay`, `DelayTooLong` if over the cap. The delay can never go down, so a compromised admin can't shorten the exit window. | trust setup |
| `transfer_admin(env, new_admin: Address)` | `admin.require_auth()` | Sets `PendingAdmin`. | ops |
| `accept_admin(env) -> Result<(), SpringError>` | `pending.require_auth()` | `NoPendingAdmin` if unset. Moves pending to admin. | ops |
| `set_guardian(env, new_guardian: Address)` | `admin.require_auth()` | Replaces the guardian. | ops |
| `extend_ttl(env, tag: String) -> Result<(), SpringError>` | **none** | Extends the ref entry, `Tag(tag)`, and instance storage to `TTL_EXTEND_TO` when below `TTL_THRESHOLD`. | 8 |
| `tag(env, tag: String) -> Result<TagState, SpringError>` | none | Read. | watchers |
| `pending(env, tag: String) -> Option<Proposal>` | none | Read. | 5 (watchers alert users) |
| `version(env, tag: String, version: u32) -> Option<VersionRecord>` | none | Read. | watchers |
| `admin(env) -> Address`, `guardian(env) -> Address` | none | Read. | watchers |

**Guardian powers are deliberately narrow.** The guardian can cancel a pending proposal and roll back one step. It can never introduce a Wasm hash that didn't already pass the timelock.

### Events (`#[contractevent]`)

| Event | Topics | Data |
|---|---|---|
| `TagCreated` | `tag` | `wasm_hash`, `min_delay` |
| `UpgradeProposed` | `tag` | `wasm_hash`, `eta` |
| `UpgradeExecuted` | `tag` | `version`, `wasm_hash` |
| `UpgradeCancelled` | `tag` | `by: Address` |
| `RolledBack` | `tag` | `version`, `wasm_hash`, `by: Address` |
| `MinDelayIncreased` | `tag` | `min_delay` |
| `AdminTransferStarted` | — | `current`, `pending` |
| `AdminTransferred` | — | `previous`, `admin` |
| `GuardianChanged` | — | `previous`, `guardian` |

---

## Contract 2: `factory`

Single responsibility: deploy instances bound to one spring tag, and keep an enumerable list of them.

### Storage

```rust
#[contracttype]
pub enum DataKey {
    Spring,           // instance: Address
    Tag,              // instance: String
    Admin,            // instance: Address (absent = open deploys)
    Count,            // instance: u32
    Instance(u32),    // persistent: Address
}
```

### Errors

```rust
#[contracterror]
#[repr(u32)]
pub enum FactoryError {
    NotAuthorized = 1,
    IndexOutOfRange = 2,
    NotInitialized = 3,
}
```

### Functions

| Function | Auth | Behaviour | Flow step |
|---|---|---|---|
| `__constructor(env, spring: Address, tag: String, admin: Option<Address>)` | none | Store config, `Count = 0`. | 2 |
| `deploy(env, deployer: Address, salt: BytesN<32>, constructor_args: Vec<Val>) -> Result<Address, FactoryError>` | `deployer.require_auth()`. If `Admin` is set, `deployer` must equal it, else `NotAuthorized`. | Effective salt = `sha256(deployer.to_xdr() ‖ salt)`, so nobody can claim another user's address first. Deploys with `with_current_contract(eff_salt).deploy_contract(ExternalRef { owner: spring, tag }, constructor_args)`. Stores `Instance(count)`, increments `Count`, extends TTLs. Returns the address. | 3 |
| `deployed_address(env, deployer: Address, salt: BytesN<32>) -> Address` | none | Same salt derivation, `with_current_contract(eff_salt).deployed_address()`. | 3 (client pre-compute) |
| `count(env) -> u32` | none | Read. | watchers |
| `instance(env, index: u32) -> Result<Address, FactoryError>` | none | `IndexOutOfRange` if `index >= count`. | watchers |
| `fleet(env) -> (Address, String)` | none | Returns `(spring, tag)`. | watchers |

### Events

| Event | Topics | Data |
|---|---|---|
| `InstanceDeployed` | `spring`, `tag` | `instance`, `deployer`, `index` |

---

## Crate 3: `mainspring-instance` (library, not a contract)

Single responsibility: give instance contracts the three operations they need on themselves, with consistent events the indexer can read.

```rust
/// Point the current contract at `owner`'s `tag`. For existing contracts joining a fleet.
pub fn attach(env: &Env, owner: Address, tag: String);

/// Detach from the fleet onto the exact Wasm this instance runs right now.
/// The caller contract is responsible for gating this with its own user's auth.
pub fn pin(env: &Env);

/// Extend instance, code and ref entry TTLs.
pub fn extend_ttl(env: &Env, threshold: u32, extend_to: u32);
```

`pin` reads `env.current_contract_address().executable()`. That returns the resolved Wasm hash, as the SDK tests confirm. `pin` then calls `update_current_contract(ContractExecutable::Wasm(hash))`.

Events emitted by the library:

| Event | Topics | Data |
|---|---|---|
| `Attached` | `owner`, `tag` | — |
| `Pinned` | — | `wasm_hash` |

---

## Examples: `tipjar-v1` and `tipjar-v2`

These exist so the tests run against real Wasm and so integrators have something to copy.

- `__constructor(env, owner: Address, token: Address)`
- `tip(env, from: Address, amount: i128)`: `from.require_auth()`, SAC transfer into the jar.
- `withdraw(env, amount: i128)`: `owner.require_auth()`, transfer out to the owner.
- `pin(env)`: `owner.require_auth()`, then `mainspring_instance::pin`. **This is the user's exit right.**
- `extend_ttl(env)`: permissionless, calls the library.
- `version(env) -> u32`: returns `1` or `2`, so tests can tell which code is running.
- **v2 only:** `withdraw_to(env, to: Address, amount: i128)`.

Storage is identical between v1 and v2 so the upgrade is a pure code swap.

---

## Required test coverage

Every row below must be a test before the corresponding commit is considered done.

| Area | Cases |
|---|---|
| spring | create, then a duplicate tag fails; invalid tag lengths; delay over cap; propose with the same hash fails; propose twice fails; execute before eta fails; execute at eta works and repoints a live instance; cancel by admin, guardian and a stranger (stranger fails); rollback once works, a second rollback fails; rollback clears pending; increase delay (equal or lower fails); two-step admin transfer; extend_ttl raises TTL of the ref entry |
| factory | open deploy; admin-only deploy rejects others; `deployed_address` matches `deploy`; same salt from two deployers gives two addresses; count and instance enumeration; out of range |
| instance lib | pin detaches onto the current hash and survives a later spring upgrade; attach moves an existing Wasm contract into the fleet |
| integration | the full user flow steps 1–8 with two instances, one pinned and one not, asserting `version()` on each after execute and after rollback |
| custom account | a pre-Protocol-28 custom account that rejects non-Wasm executables **fails** when deploying an `ExternalRef` contract directly, and **succeeds** through the factory, because the factory is the deployer and the account only authorizes a plain `deploy` call |

## Explicitly out of scope for v0.1

- On-chain governance voting. The admin is any `Address`, so a multisig account or a DAO contract is already supported.
- Decreasing `min_delay`. To shorten it, create a new tag.
- Per-instance opt-in to each upgrade. Pinning is the opt-out. Opt-in would turn every upgrade back into N transactions, which is the problem CAP-85 removes.
- Wasm verification or source matching. That belongs in the app repo's indexer.
