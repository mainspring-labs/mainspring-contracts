//! Helpers for contracts that run from a Mainspring executable reference.
//!
//! A contract deployed by a Mainspring factory does not hold its own Wasm hash.
//! It runs whatever hash its spring stores under a tag (CAP-85), so the
//! spring's admin can change its code. These helpers give the instance the
//! three operations it needs on itself:
//!
//! - [`attach`] moves an existing contract onto a spring tag.
//! - [`pin`] detaches the contract onto the exact Wasm it runs right now. This
//!   is the instance user's exit right: call it before a proposed upgrade
//!   executes and the upgrade will not reach this contract.
//! - [`extend_ttl`] keeps the instance, its code and the ref entry alive.
//!
//! None of these functions check auth. The calling contract decides who may
//! attach or pin it, usually its owner.
#![no_std]

use soroban_sdk::{
    contractevent, Address, ContractExecutable, ContractExecutableRef, Env, Executable, String,
};

/// Emitted when a contract points itself at a spring tag.
#[contractevent(topics = ["attached"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Attached {
    #[topic]
    pub owner: Address,
    #[topic]
    pub tag: String,
}

/// Emitted when a contract detaches onto a fixed Wasm hash.
#[contractevent(topics = ["pinned"])]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pinned {
    pub wasm_hash: soroban_sdk::BytesN<32>,
}

/// Point the current contract at `owner`'s executable reference `tag`.
///
/// From the next invocation on, the contract runs whatever Wasm hash the
/// owner stores under `tag`, and the owner can change it.
///
/// ### Panics
///
/// If `owner` has no executable reference entry under `tag`.
pub fn attach(env: &Env, owner: Address, tag: String) {
    env.deployer()
        .update_current_contract(ContractExecutable::ExternalRef(ContractExecutableRef {
            owner: owner.clone(),
            tag: tag.clone(),
        }));
    Attached { owner, tag }.publish(env);
}

/// Detach the current contract onto the Wasm it is running right now.
///
/// Later updates to the spring tag no longer reach this contract. Pinning a
/// contract that already runs a fixed Wasm hash is a no-op apart from the
/// event.
pub fn pin(env: &Env) {
    let wasm_hash = match env.current_contract_address().executable() {
        Some(Executable::Wasm(hash)) => hash,
        // The current contract is executing, so it exists and is Wasm.
        _ => unreachable!(),
    };
    env.deployer()
        .update_current_contract(ContractExecutable::Wasm(wasm_hash.clone()));
    Pinned { wasm_hash }.publish(env);
}

/// Extend the TTL of the current contract's instance, its code and, for a
/// contract running from an executable reference, the reference entry.
pub fn extend_ttl(env: &Env, threshold: u32, extend_to: u32) {
    env.deployer()
        .extend_ttl(env.current_contract_address(), threshold, extend_to);
}
