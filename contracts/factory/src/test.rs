#![cfg(test)]
extern crate std;

use mainspring_spring::{Spring, SpringClient};
use soroban_sdk::{
    auth::{Context, CustomAccountInterface},
    contract, contracterror, contractimpl,
    crypto::Hash,
    testutils::{Address as _, Ledger as _},
    xdr::{
        InvokeContractArgs, ScVal, SorobanAddressCredentials, SorobanAuthorizationEntry,
        SorobanAuthorizedFunction, SorobanAuthorizedInvocation, SorobanCredentials, StringM, VecM,
    },
    Address, BytesN, ContractExecutable, Env, IntoVal, String, TryFromVal, Val, Vec,
};

use crate::{Factory, FactoryClient, FactoryError};

mod tipjar_v1 {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/tipjar_v1.wasm");
}
mod tipjar_v2 {
    soroban_sdk::contractimport!(file = "../../target/wasm32v1-none/release/tipjar_v2.wasm");
}

const DELAY: u64 = 259_200;

struct Setup<'a> {
    env: Env,
    spring: SpringClient<'a>,
    tag: String,
    token: Address,
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
    let token = env
        .register_stellar_asset_contract_v2(Address::generate(&env))
        .address();

    Setup {
        env,
        spring,
        tag,
        token,
        v2,
    }
}

fn factory<'a>(s: &Setup, admin: Option<Address>) -> FactoryClient<'a> {
    let id = s.env.register(Factory, (&s.spring.address, &s.tag, admin));
    FactoryClient::new(&s.env, &id)
}

fn args(s: &Setup, owner: &Address) -> Vec<Val> {
    (owner.clone(), s.token.clone()).into_val(&s.env)
}

fn salt(env: &Env, n: u8) -> BytesN<32> {
    BytesN::from_array(env, &[n; 32])
}

#[test]
fn open_deploy_runs_from_spring_tag() {
    let s = setup();
    let f = factory(&s, None);
    let user = Address::generate(&s.env);

    let jar = f.deploy(&user, &salt(&s.env, 1), &args(&s, &user));
    let client = tipjar_v1::Client::new(&s.env, &jar);
    assert_eq!(client.version(), 1);
    assert_eq!(client.owner(), user);
    assert_eq!(f.fleet(), (s.spring.address.clone(), s.tag.clone()));
}

#[test]
fn deployed_instances_follow_spring_upgrade() {
    let s = setup();
    let f = factory(&s, None);
    let user = Address::generate(&s.env);
    let jar = f.deploy(&user, &salt(&s.env, 1), &args(&s, &user));

    s.spring.propose(&s.tag, &s.v2);
    s.env.ledger().with_mut(|l| l.timestamp += DELAY);
    s.spring.execute(&s.tag);

    assert_eq!(tipjar_v2::Client::new(&s.env, &jar).version(), 2);
}

#[test]
fn admin_only_rejects_other_deployers() {
    let s = setup();
    let admin = Address::generate(&s.env);
    let f = factory(&s, Some(admin.clone()));
    let stranger = Address::generate(&s.env);

    assert_eq!(
        f.try_deploy(&stranger, &salt(&s.env, 1), &args(&s, &stranger)),
        Err(Ok(FactoryError::NotAuthorized))
    );
    f.deploy(&admin, &salt(&s.env, 1), &args(&s, &admin));
    assert_eq!(f.count(), 1);
}

#[test]
fn deployed_address_matches_deploy() {
    let s = setup();
    let f = factory(&s, None);
    let user = Address::generate(&s.env);

    let predicted = f.deployed_address(&user, &salt(&s.env, 9));
    let actual = f.deploy(&user, &salt(&s.env, 9), &args(&s, &user));
    assert_eq!(predicted, actual);
}

#[test]
fn same_salt_from_two_deployers_gives_two_addresses() {
    let s = setup();
    let f = factory(&s, None);
    let alice = Address::generate(&s.env);
    let bob = Address::generate(&s.env);

    let a = f.deploy(&alice, &salt(&s.env, 1), &args(&s, &alice));
    let b = f.deploy(&bob, &salt(&s.env, 1), &args(&s, &bob));
    assert_ne!(a, b);
}

#[test]
fn enumerates_instances_in_order() {
    let s = setup();
    let f = factory(&s, None);
    assert_eq!(f.count(), 0);

    let mut deployed = std::vec::Vec::new();
    for n in 0..3u8 {
        let user = Address::generate(&s.env);
        deployed.push(f.deploy(&user, &salt(&s.env, n), &args(&s, &user)));
    }

    assert_eq!(f.count(), 3);
    for (i, addr) in deployed.iter().enumerate() {
        assert_eq!(&f.instance(&(i as u32)), addr);
    }
    assert_eq!(f.try_instance(&3), Err(Ok(FactoryError::IndexOutOfRange)));
}

#[test]
fn deploy_requires_deployer_auth() {
    let s = setup();
    let f = factory(&s, None);
    let user = Address::generate(&s.env);
    let ctor = args(&s, &user);

    s.env.set_auths(&[]);
    assert!(f.try_deploy(&user, &salt(&s.env, 1), &ctor).is_err());
}

/// An account written before Protocol 28: it refuses to authorize the
/// creation of any contract whose executable is not plain Wasm. CAP-85 calls
/// this out as a breaking case for accounts that deploy directly.
#[contract]
struct PreP28Account;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
enum PreP28AccountError {
    UnknownExecutable = 1,
}

#[contractimpl]
impl CustomAccountInterface for PreP28Account {
    type Signature = ();
    type Error = PreP28AccountError;

    fn __check_auth(
        _env: Env,
        _signature_payload: Hash<32>,
        _signatures: (),
        auth_contexts: Vec<Context>,
    ) -> Result<(), PreP28AccountError> {
        for ctx in auth_contexts.iter() {
            let executable = match ctx {
                Context::Contract(_) => continue,
                Context::CreateContractHostFn(c) => c.executable,
                Context::CreateContractWithCtorHostFn(c) => c.executable,
            };
            if !matches!(executable, ContractExecutable::Wasm(_)) {
                return Err(PreP28AccountError::UnknownExecutable);
            }
        }
        Ok(())
    }
}

/// The factory, not the user, is the deployer, so the user's account only
/// authorizes a plain `deploy` call and never sees the external-ref executable.
#[test]
fn pre_p28_custom_account_can_deploy_through_factory() {
    let s = setup();
    let f = factory(&s, None);
    let account = s.env.register(PreP28Account, ());
    let ctor = args(&s, &account);
    let salt = salt(&s.env, 7);

    let call_vals: Vec<Val> = (account.clone(), salt.clone(), ctor.clone()).into_val(&s.env);
    let call_args: std::vec::Vec<ScVal> = call_vals
        .iter()
        .map(|v: Val| ScVal::try_from_val(&s.env, &v).unwrap())
        .collect();

    s.env.set_auths(&[SorobanAuthorizationEntry {
        credentials: SorobanCredentials::Address(SorobanAddressCredentials {
            address: (&account).into(),
            nonce: 1,
            signature_expiration_ledger: 100,
            signature: ScVal::Void,
        }),
        root_invocation: SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::ContractFn(InvokeContractArgs {
                contract_address: (&f.address).into(),
                function_name: StringM::try_from("deploy").unwrap().into(),
                args: call_args.try_into().unwrap(),
            }),
            sub_invocations: VecM::default(),
        },
    }]);

    let jar = f.deploy(&account, &salt, &ctor);
    assert_eq!(tipjar_v1::Client::new(&s.env, &jar).owner(), account);
}

/// Control for the test above: the same account deploying directly is asked
/// to authorize an external-ref creation, and refuses.
#[test]
fn pre_p28_custom_account_cannot_deploy_external_ref_directly() {
    use soroban_sdk::xdr::{
        ContractExecutable as XdrContractExecutable, ContractExecutableExternalRef,
        ContractIdPreimage, ContractIdPreimageFromAddress, CreateContractArgsV2, ScString, Uint256,
    };
    use soroban_sdk::ContractExecutableRef;

    let s = setup();
    let account = s.env.register(PreP28Account, ());
    let ctor = args(&s, &account);
    let ctor_sc: std::vec::Vec<ScVal> = ctor
        .iter()
        .map(|v: Val| ScVal::try_from_val(&s.env, &v).unwrap())
        .collect();

    s.env.set_auths(&[SorobanAuthorizationEntry {
        credentials: SorobanCredentials::Address(SorobanAddressCredentials {
            address: (&account).into(),
            nonce: 1,
            signature_expiration_ledger: 100,
            signature: ScVal::Void,
        }),
        root_invocation: SorobanAuthorizedInvocation {
            function: SorobanAuthorizedFunction::CreateContractV2HostFn(CreateContractArgsV2 {
                contract_id_preimage: ContractIdPreimage::Address(ContractIdPreimageFromAddress {
                    address: (&account).into(),
                    salt: Uint256([7; 32]),
                }),
                executable: XdrContractExecutable::ExternalRef(ContractExecutableExternalRef {
                    executable_owner: (&s.spring.address).into(),
                    tag: ScString("tipjar".try_into().unwrap()),
                }),
                constructor_args: ctor_sc.try_into().unwrap(),
            }),
            sub_invocations: VecM::default(),
        },
    }]);

    let direct = DirectDeployerClient::new(&s.env, &s.env.register(DirectDeployer, ()));
    let result = direct.try_deploy_for(
        &account,
        &ContractExecutableRef {
            owner: s.spring.address.clone(),
            tag: s.tag.clone(),
        },
        &salt(&s.env, 7),
        &ctor,
    );
    assert!(result.is_err());
}

/// Deploys on behalf of `deployer`, so the deployer's own account must
/// authorize the create-contract call and its executable.
#[contract]
struct DirectDeployer;

#[contractimpl]
impl DirectDeployer {
    pub fn deploy_for(
        env: Env,
        deployer: Address,
        executable: soroban_sdk::ContractExecutableRef,
        salt: BytesN<32>,
        constructor_args: Vec<Val>,
    ) -> Address {
        env.deployer().with_address(deployer, salt).deploy_contract(
            ContractExecutable::ExternalRef(executable),
            constructor_args,
        )
    }
}
