use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, String};

use crate::{
    error::SpringError,
    events::{
        AdminTransferStarted, AdminTransferred, GuardianChanged, MinDelayIncreased, RolledBack,
        TagCreated, UpgradeCancelled, UpgradeExecuted, UpgradeProposed,
    },
    storage::{
        self, Proposal, TagState, VersionKind, VersionRecord, MAX_DELAY, TAG_MAX_LEN,
        TTL_EXTEND_TO, TTL_THRESHOLD,
    },
};

/// Owns CAP-85 executable references and gates every change to them.
///
/// Each tag is a Wasm hash that a fleet of contracts runs from. New code waits
/// out the tag's timelock before it goes live. The guardian can cancel a
/// pending upgrade or roll back one step, but never introduce new code.
#[contract]
pub struct Spring;

#[contractimpl]
impl Spring {
    pub fn __constructor(env: Env, admin: Address, guardian: Address) {
        storage::set_admin(&env, &admin);
        storage::set_guardian(&env, &guardian);
        storage::extend_instance(&env);
    }

    /// Create `tag` pointing at `wasm_hash`. The Wasm must already be
    /// uploaded. No timelock: nothing runs from a new tag yet.
    pub fn create_tag(
        env: Env,
        tag: String,
        wasm_hash: BytesN<32>,
        min_delay: u64,
    ) -> Result<(), SpringError> {
        storage::admin(&env).require_auth();
        if tag.len() == 0 || tag.len() > TAG_MAX_LEN {
            return Err(SpringError::InvalidTag);
        }
        if storage::tag_state(&env, &tag).is_some() {
            return Err(SpringError::TagExists);
        }
        if min_delay > MAX_DELAY {
            return Err(SpringError::DelayTooLong);
        }

        let refs = env.executable_refs();
        refs.set(&tag, &wasm_hash);
        refs.extend_ttl(&tag, TTL_THRESHOLD, TTL_EXTEND_TO);

        let now = env.ledger().timestamp();
        storage::set_tag_state(
            &env,
            &tag,
            &TagState {
                min_delay,
                version: 1,
                current: wasm_hash.clone(),
                previous: None,
                pending: None,
            },
        );
        storage::set_version_record(
            &env,
            &tag,
            1,
            &VersionRecord {
                wasm_hash: wasm_hash.clone(),
                activated_at: now,
                kind: VersionKind::Created,
            },
        );
        storage::extend_instance(&env);

        TagCreated {
            tag,
            wasm_hash,
            min_delay,
        }
        .publish(&env);
        Ok(())
    }

    /// Queue `wasm_hash` for `tag`. Returns the earliest time it can execute.
    pub fn propose(env: Env, tag: String, wasm_hash: BytesN<32>) -> Result<u64, SpringError> {
        storage::admin(&env).require_auth();
        let mut state = storage::tag_state(&env, &tag).ok_or(SpringError::TagNotFound)?;
        if state.pending.is_some() {
            return Err(SpringError::ProposalPending);
        }
        if state.current == wasm_hash {
            return Err(SpringError::SameHash);
        }

        let proposed_at = env.ledger().timestamp();
        let eta = proposed_at + state.min_delay;
        state.pending = Some(Proposal {
            wasm_hash: wasm_hash.clone(),
            proposed_at,
            eta,
        });
        storage::set_tag_state(&env, &tag, &state);
        storage::extend_instance(&env);

        UpgradeProposed {
            tag,
            wasm_hash,
            eta,
        }
        .publish(&env);
        Ok(eta)
    }

    /// Apply the pending proposal for `tag`. Anyone can call this once the
    /// timelock has passed. Returns the new version number.
    pub fn execute(env: Env, tag: String) -> Result<u32, SpringError> {
        let mut state = storage::tag_state(&env, &tag).ok_or(SpringError::TagNotFound)?;
        let proposal = state.pending.clone().ok_or(SpringError::NoProposal)?;
        let now = env.ledger().timestamp();
        if now < proposal.eta {
            return Err(SpringError::TooEarly);
        }

        env.executable_refs().set(&tag, &proposal.wasm_hash);

        state.previous = Some(state.current.clone());
        state.current = proposal.wasm_hash.clone();
        state.version += 1;
        state.pending = None;
        storage::set_tag_state(&env, &tag, &state);
        storage::set_version_record(
            &env,
            &tag,
            state.version,
            &VersionRecord {
                wasm_hash: proposal.wasm_hash.clone(),
                activated_at: now,
                kind: VersionKind::Upgrade,
            },
        );
        storage::extend_instance(&env);

        UpgradeExecuted {
            tag,
            version: state.version,
            wasm_hash: proposal.wasm_hash,
        }
        .publish(&env);
        Ok(state.version)
    }

    /// Drop the pending proposal for `tag`. Admin or guardian.
    pub fn cancel(env: Env, caller: Address, tag: String) -> Result<(), SpringError> {
        require_admin_or_guardian(&env, &caller)?;
        let mut state = storage::tag_state(&env, &tag).ok_or(SpringError::TagNotFound)?;
        if state.pending.is_none() {
            return Err(SpringError::NoProposal);
        }
        state.pending = None;
        storage::set_tag_state(&env, &tag, &state);
        storage::extend_instance(&env);

        UpgradeCancelled { tag, by: caller }.publish(&env);
        Ok(())
    }

    /// Point `tag` back at the hash it held before the last upgrade, with no
    /// timelock: that hash already passed one. Allowed once per upgrade.
    /// Clears any pending proposal. Admin or guardian. Returns the new version.
    pub fn rollback(env: Env, caller: Address, tag: String) -> Result<u32, SpringError> {
        require_admin_or_guardian(&env, &caller)?;
        let mut state = storage::tag_state(&env, &tag).ok_or(SpringError::TagNotFound)?;
        let target = state.previous.clone().ok_or(SpringError::NothingToRollBack)?;

        env.executable_refs().set(&tag, &target);

        let now = env.ledger().timestamp();
        state.current = target.clone();
        state.previous = None;
        state.pending = None;
        state.version += 1;
        storage::set_tag_state(&env, &tag, &state);
        storage::set_version_record(
            &env,
            &tag,
            state.version,
            &VersionRecord {
                wasm_hash: target.clone(),
                activated_at: now,
                kind: VersionKind::Rollback,
            },
        );
        storage::extend_instance(&env);

        RolledBack {
            tag,
            version: state.version,
            wasm_hash: target,
            by: caller,
        }
        .publish(&env);
        Ok(state.version)
    }

    /// Raise the timelock on `tag`. It can never be lowered, so a compromised
    /// admin cannot shorten the window instance users have to pin.
    pub fn increase_min_delay(env: Env, tag: String, new_delay: u64) -> Result<(), SpringError> {
        storage::admin(&env).require_auth();
        let mut state = storage::tag_state(&env, &tag).ok_or(SpringError::TagNotFound)?;
        if new_delay <= state.min_delay {
            return Err(SpringError::DelayNotIncreased);
        }
        if new_delay > MAX_DELAY {
            return Err(SpringError::DelayTooLong);
        }
        state.min_delay = new_delay;
        storage::set_tag_state(&env, &tag, &state);
        storage::extend_instance(&env);

        MinDelayIncreased {
            tag,
            min_delay: new_delay,
        }
        .publish(&env);
        Ok(())
    }

    /// Start a two-step admin transfer. `new_admin` must call `accept_admin`.
    pub fn transfer_admin(env: Env, new_admin: Address) {
        let current = storage::admin(&env);
        current.require_auth();
        storage::set_pending_admin(&env, &new_admin);
        storage::extend_instance(&env);

        AdminTransferStarted {
            current,
            pending: new_admin,
        }
        .publish(&env);
    }

    pub fn accept_admin(env: Env) -> Result<(), SpringError> {
        let pending = storage::pending_admin(&env).ok_or(SpringError::NoPendingAdmin)?;
        pending.require_auth();
        let previous = storage::admin(&env);
        storage::set_admin(&env, &pending);
        storage::clear_pending_admin(&env);
        storage::extend_instance(&env);

        AdminTransferred {
            previous,
            admin: pending,
        }
        .publish(&env);
        Ok(())
    }

    pub fn set_guardian(env: Env, new_guardian: Address) {
        storage::admin(&env).require_auth();
        let previous = storage::guardian(&env);
        storage::set_guardian(&env, &new_guardian);
        storage::extend_instance(&env);

        GuardianChanged {
            previous,
            guardian: new_guardian,
        }
        .publish(&env);
    }

    /// Keep `tag` alive: extends the reference entry, the tag state and this
    /// contract's instance. Anyone can call it.
    pub fn extend_ttl(env: Env, tag: String) -> Result<(), SpringError> {
        let state = storage::tag_state(&env, &tag).ok_or(SpringError::TagNotFound)?;
        env.executable_refs()
            .extend_ttl(&tag, TTL_THRESHOLD, TTL_EXTEND_TO);
        storage::set_tag_state(&env, &tag, &state);
        storage::extend_instance(&env);
        Ok(())
    }

    pub fn tag(env: Env, tag: String) -> Result<TagState, SpringError> {
        storage::tag_state(&env, &tag).ok_or(SpringError::TagNotFound)
    }

    pub fn version(env: Env, tag: String, version: u32) -> Option<VersionRecord> {
        storage::version_record(&env, &tag, version)
    }

    pub fn admin(env: Env) -> Address {
        storage::admin(&env)
    }

    pub fn guardian(env: Env) -> Address {
        storage::guardian(&env)
    }
}

fn require_admin_or_guardian(env: &Env, caller: &Address) -> Result<(), SpringError> {
    caller.require_auth();
    if *caller != storage::admin(env) && *caller != storage::guardian(env) {
        return Err(SpringError::NotAuthorized);
    }
    Ok(())
}
