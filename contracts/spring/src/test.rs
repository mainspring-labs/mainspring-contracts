#![cfg(test)]
extern crate std;

use soroban_sdk::{
    testutils::{Address as _, Ledger as _},
    Address, BytesN, ContractExecutable, ContractExecutableRef, Env, Executable, String,
};

use crate::{Spring, SpringClient, SpringError, VersionKind, MAX_DELAY};

mod tipjar_v1 {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/tipjar_v1.wasm");
}
mod tipjar_v2 {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/tipjar_v2.wasm");
}

const DELAY: u64 = 259_200; // 3 days

struct Setup<'a> {
    env: Env,
    spring: SpringClient<'a>,
    admin: Address,
    guardian: Address,
    tag: String,
    v1: BytesN<32>,
    v2: BytesN<32>,
}

fn setup<'a>() -> Setup<'a> {
    let env = Env::default();
    env.mock_all_auths();
    env.ledger().with_mut(|l| l.timestamp = 1_000_000);

    let admin = Address::generate(&env);
    let guardian = Address::generate(&env);
    let spring_id = env.register(Spring, (&admin, &guardian));
    let spring = SpringClient::new(&env, &spring_id);

    let v1 = env.deployer().upload_contract_wasm(tipjar_v1::WASM);
    let v2 = env.deployer().upload_contract_wasm(tipjar_v2::WASM);
    let tag = String::from_str(&env, "tipjar");
    spring.create_tag(&tag, &v1, &DELAY);

    Setup {
        env,
        spring,
        admin,
        guardian,
        tag,
        v1,
        v2,
    }
}

/// Deploy a tip jar that runs from the spring's tag.
fn deploy_instance(s: &Setup, salt: u8) -> tipjar_v1::Client<'static> {
    let owner = Address::generate(&s.env);
    let token = s
        .env
        .register_stellar_asset_contract_v2(Address::generate(&s.env))
        .address();
    let addr = s
        .env
        .deployer()
        .with_address(s.admin.clone(), BytesN::from_array(&s.env, &[salt; 32]))
        .deploy_contract(
            ContractExecutable::ExternalRef(ContractExecutableRef {
                owner: s.spring.address.clone(),
                tag: s.tag.clone(),
            }),
            (owner, token),
        );
    tipjar_v1::Client::new(&s.env, &addr)
}

fn advance(env: &Env, seconds: u64) {
    env.ledger().with_mut(|l| l.timestamp += seconds);
}

#[test]
fn create_tag_records_version_one() {
    let s = setup();
    let state = s.spring.tag(&s.tag);
    assert_eq!(state.version, 1);
    assert_eq!(state.current, s.v1);
    assert_eq!(state.previous, None);
    assert_eq!(s.spring.pending(&s.tag), None);
    assert_eq!(state.min_delay, DELAY);

    let record = s.spring.version(&s.tag, &1).unwrap();
    assert_eq!(record.wasm_hash, s.v1);
    assert_eq!(record.kind, VersionKind::Created);
}

#[test]
fn create_tag_rejects_duplicate() {
    let s = setup();
    assert_eq!(
        s.spring.try_create_tag(&s.tag, &s.v2, &DELAY),
        Err(Ok(SpringError::TagExists))
    );
}

#[test]
fn create_tag_rejects_bad_length() {
    let s = setup();
    let empty = String::from_str(&s.env, "");
    let long = String::from_str(&s.env, &"x".repeat(65));
    let max = String::from_str(&s.env, &"x".repeat(64));
    assert_eq!(
        s.spring.try_create_tag(&empty, &s.v1, &DELAY),
        Err(Ok(SpringError::InvalidTag))
    );
    assert_eq!(
        s.spring.try_create_tag(&long, &s.v1, &DELAY),
        Err(Ok(SpringError::InvalidTag))
    );
    s.spring.create_tag(&max, &s.v1, &DELAY);
}

#[test]
fn create_tag_rejects_delay_over_cap() {
    let s = setup();
    let tag = String::from_str(&s.env, "other");
    assert_eq!(
        s.spring.try_create_tag(&tag, &s.v1, &(MAX_DELAY + 1)),
        Err(Ok(SpringError::DelayTooLong))
    );
}

#[test]
fn propose_rejects_same_hash_and_double_proposal() {
    let s = setup();
    assert_eq!(
        s.spring.try_propose(&s.tag, &s.v1),
        Err(Ok(SpringError::SameHash))
    );
    s.spring.propose(&s.tag, &s.v2);
    assert_eq!(
        s.spring.try_propose(&s.tag, &s.v2),
        Err(Ok(SpringError::ProposalPending))
    );
}

#[test]
fn propose_unknown_tag_fails() {
    let s = setup();
    let missing = String::from_str(&s.env, "missing");
    assert_eq!(
        s.spring.try_propose(&missing, &s.v2),
        Err(Ok(SpringError::TagNotFound))
    );
}

#[test]
fn execute_respects_timelock() {
    let s = setup();
    let eta = s.spring.propose(&s.tag, &s.v2);
    assert_eq!(eta, s.env.ledger().timestamp() + DELAY);

    advance(&s.env, DELAY - 1);
    assert_eq!(s.spring.try_execute(&s.tag), Err(Ok(SpringError::TooEarly)));

    advance(&s.env, 1);
    assert_eq!(s.spring.execute(&s.tag), 2);
}

#[test]
fn execute_without_proposal_fails() {
    let s = setup();
    assert_eq!(
        s.spring.try_execute(&s.tag),
        Err(Ok(SpringError::NoProposal))
    );
}

#[test]
fn execute_repoints_live_instances() {
    let s = setup();
    let a = deploy_instance(&s, 1);
    let b = deploy_instance(&s, 2);
    assert_eq!(a.version(), 1);
    assert_eq!(b.version(), 1);

    s.spring.propose(&s.tag, &s.v2);
    advance(&s.env, DELAY);
    s.spring.execute(&s.tag);

    assert_eq!(a.version(), 2);
    assert_eq!(b.version(), 2);
    assert_eq!(a.address.executable(), Some(Executable::Wasm(s.v2.clone())));

    let state = s.spring.tag(&s.tag);
    assert_eq!(state.current, s.v2);
    assert_eq!(state.previous, Some(s.v1.clone()));
    assert_eq!(s.spring.pending(&s.tag), None);
    assert_eq!(
        s.spring.version(&s.tag, &2).unwrap().kind,
        VersionKind::Upgrade
    );
}

#[test]
fn pinned_instance_ignores_upgrade() {
    let s = setup();
    let pinned = deploy_instance(&s, 1);
    let follower = deploy_instance(&s, 2);

    s.spring.propose(&s.tag, &s.v2);
    pinned.pin();
    advance(&s.env, DELAY);
    s.spring.execute(&s.tag);

    assert_eq!(pinned.version(), 1);
    assert_eq!(follower.version(), 2);
}

#[test]
fn cancel_by_admin_and_guardian() {
    let s = setup();
    s.spring.propose(&s.tag, &s.v2);
    s.spring.cancel(&s.admin, &s.tag);
    assert_eq!(s.spring.pending(&s.tag), None);

    s.spring.propose(&s.tag, &s.v2);
    s.spring.cancel(&s.guardian, &s.tag);
    assert_eq!(s.spring.pending(&s.tag), None);

    assert_eq!(
        s.spring.try_cancel(&s.admin, &s.tag),
        Err(Ok(SpringError::NoProposal))
    );
}

#[test]
fn cancel_by_stranger_fails() {
    let s = setup();
    s.spring.propose(&s.tag, &s.v2);
    let stranger = Address::generate(&s.env);
    assert_eq!(
        s.spring.try_cancel(&stranger, &s.tag),
        Err(Ok(SpringError::NotAuthorized))
    );
}

#[test]
fn rollback_once_then_fails() {
    let s = setup();
    let jar = deploy_instance(&s, 1);
    s.spring.propose(&s.tag, &s.v2);
    advance(&s.env, DELAY);
    s.spring.execute(&s.tag);
    assert_eq!(jar.version(), 2);

    assert_eq!(s.spring.rollback(&s.guardian, &s.tag), 3);
    assert_eq!(jar.version(), 1);
    let state = s.spring.tag(&s.tag);
    assert_eq!(state.current, s.v1);
    assert_eq!(state.previous, None);
    assert_eq!(
        s.spring.version(&s.tag, &3).unwrap().kind,
        VersionKind::Rollback
    );

    assert_eq!(
        s.spring.try_rollback(&s.guardian, &s.tag),
        Err(Ok(SpringError::NothingToRollBack))
    );
}

#[test]
fn rollback_before_any_upgrade_fails() {
    let s = setup();
    assert_eq!(
        s.spring.try_rollback(&s.admin, &s.tag),
        Err(Ok(SpringError::NothingToRollBack))
    );
}

#[test]
fn rollback_clears_pending() {
    let s = setup();
    s.spring.propose(&s.tag, &s.v2);
    advance(&s.env, DELAY);
    s.spring.execute(&s.tag);

    s.spring.propose(&s.tag, &s.v1);
    s.spring.rollback(&s.admin, &s.tag);
    assert_eq!(s.spring.pending(&s.tag), None);
}

#[test]
fn rollback_by_stranger_fails() {
    let s = setup();
    s.spring.propose(&s.tag, &s.v2);
    advance(&s.env, DELAY);
    s.spring.execute(&s.tag);
    let stranger = Address::generate(&s.env);
    assert_eq!(
        s.spring.try_rollback(&stranger, &s.tag),
        Err(Ok(SpringError::NotAuthorized))
    );
}

#[test]
fn min_delay_only_increases() {
    let s = setup();
    assert_eq!(
        s.spring.try_increase_min_delay(&s.tag, &DELAY),
        Err(Ok(SpringError::DelayNotIncreased))
    );
    assert_eq!(
        s.spring.try_increase_min_delay(&s.tag, &(DELAY - 1)),
        Err(Ok(SpringError::DelayNotIncreased))
    );
    assert_eq!(
        s.spring.try_increase_min_delay(&s.tag, &(MAX_DELAY + 1)),
        Err(Ok(SpringError::DelayTooLong))
    );
    s.spring.increase_min_delay(&s.tag, &(DELAY * 2));
    assert_eq!(s.spring.tag(&s.tag).min_delay, DELAY * 2);

    let eta = s.spring.propose(&s.tag, &s.v2);
    assert_eq!(eta, s.env.ledger().timestamp() + DELAY * 2);
}

#[test]
fn admin_transfer_is_two_step() {
    let s = setup();
    assert_eq!(
        s.spring.try_accept_admin(),
        Err(Ok(SpringError::NoPendingAdmin))
    );

    let next = Address::generate(&s.env);
    s.spring.transfer_admin(&next);
    assert_eq!(s.spring.admin(), s.admin);

    s.spring.accept_admin();
    assert_eq!(s.spring.admin(), next);
    assert_eq!(
        s.spring.try_accept_admin(),
        Err(Ok(SpringError::NoPendingAdmin))
    );
}

#[test]
fn set_guardian_replaces_guardian() {
    let s = setup();
    let next = Address::generate(&s.env);
    s.spring.set_guardian(&next);
    assert_eq!(s.spring.guardian(), next);

    s.spring.propose(&s.tag, &s.v2);
    assert_eq!(
        s.spring.try_cancel(&s.guardian, &s.tag),
        Err(Ok(SpringError::NotAuthorized))
    );
    s.spring.cancel(&next, &s.tag);
}

#[test]
fn admin_functions_require_admin_auth() {
    let env = Env::default();
    let admin = Address::generate(&env);
    let guardian = Address::generate(&env);
    let spring_id = env.register(Spring, (&admin, &guardian));
    let spring = SpringClient::new(&env, &spring_id);
    let v1 = env.deployer().upload_contract_wasm(tipjar_v1::WASM);
    let tag = String::from_str(&env, "tipjar");

    // No auths mocked: the admin has not signed.
    assert!(spring.try_create_tag(&tag, &v1, &DELAY).is_err());
}

#[test]
fn extend_ttl_raises_ref_entry_ttl() {
    let s = setup();
    let ttl_before = s.env.as_contract(&s.spring.address, || {
        s.env.executable_refs().get_ttl(&s.tag)
    });

    s.env.ledger().with_mut(|l| l.sequence_number += 3_000_000);
    s.spring.extend_ttl(&s.tag);

    let ttl_after = s.env.as_contract(&s.spring.address, || {
        s.env.executable_refs().get_ttl(&s.tag)
    });
    assert!(ttl_before > 3_000_000);
    assert_eq!(ttl_after, crate::storage::TTL_EXTEND_TO);
}
