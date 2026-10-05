use soroban_sdk::{contracttype, panic_with_error, Address, BytesN, Env, String};

use crate::error::SpringError;

/// Longest allowed timelock: 30 days. Caps a mistyped delay that would
/// freeze a tag for good, since delays can never be lowered.
pub const MAX_DELAY: u64 = 2_592_000;
pub const TAG_MAX_LEN: u32 = 64;
/// Extend an entry once its TTL drops below ~30 days of ledgers.
pub const TTL_THRESHOLD: u32 = 518_400;
/// Extend entries to ~180 days of ledgers.
pub const TTL_EXTEND_TO: u32 = 3_110_400;

#[contracttype]
pub enum DataKey {
    Admin,
    PendingAdmin,
    Guardian,
    Tag(String),
    Version(String, u32),
}

/// Everything about one tag in a single read. `current` mirrors the
/// executable reference entry the protocol stores under the same tag.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagState {
    pub min_delay: u64,
    pub version: u32,
    pub current: BytesN<32>,
    pub previous: Option<BytesN<32>>,
    pub pending: Option<Proposal>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Proposal {
    pub wasm_hash: BytesN<32>,
    pub proposed_at: u64,
    pub eta: u64,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VersionKind {
    Created,
    Upgrade,
    Rollback,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VersionRecord {
    pub wasm_hash: BytesN<32>,
    pub activated_at: u64,
    pub kind: VersionKind,
}

pub fn extend_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
}

pub fn admin(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .unwrap_or_else(|| panic_with_error!(env, SpringError::NotAuthorized))
}

pub fn set_admin(env: &Env, admin: &Address) {
    env.storage().instance().set(&DataKey::Admin, admin);
}

pub fn pending_admin(env: &Env) -> Option<Address> {
    env.storage().instance().get(&DataKey::PendingAdmin)
}

pub fn set_pending_admin(env: &Env, pending: &Address) {
    env.storage().instance().set(&DataKey::PendingAdmin, pending);
}

pub fn clear_pending_admin(env: &Env) {
    env.storage().instance().remove(&DataKey::PendingAdmin);
}

pub fn guardian(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Guardian)
        .unwrap_or_else(|| panic_with_error!(env, SpringError::NotAuthorized))
}

pub fn set_guardian(env: &Env, guardian: &Address) {
    env.storage().instance().set(&DataKey::Guardian, guardian);
}

pub fn tag_state(env: &Env, tag: &String) -> Option<TagState> {
    env.storage()
        .persistent()
        .get(&DataKey::Tag(tag.clone()))
}

pub fn set_tag_state(env: &Env, tag: &String, state: &TagState) {
    let key = DataKey::Tag(tag.clone());
    env.storage().persistent().set(&key, state);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}

pub fn version_record(env: &Env, tag: &String, version: u32) -> Option<VersionRecord> {
    env.storage()
        .persistent()
        .get(&DataKey::Version(tag.clone(), version))
}

pub fn set_version_record(env: &Env, tag: &String, version: u32, record: &VersionRecord) {
    let key = DataKey::Version(tag.clone(), version);
    env.storage().persistent().set(&key, record);
    env.storage()
        .persistent()
        .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
}
