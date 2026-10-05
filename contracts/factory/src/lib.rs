//! Mainspring factory: deploys contracts that run from one spring tag and
//! keeps an enumerable list of them.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractevent, contractimpl, contracttype, panic_with_error,
    xdr::ToXdr, Address, Bytes, BytesN, ContractExecutable, ContractExecutableRef, Env, String,
    Val, Vec,
};

const TTL_THRESHOLD: u32 = 518_400;
const TTL_EXTEND_TO: u32 = 3_110_400;

#[contracttype]
pub enum DataKey {
    Spring,
    Tag,
    Admin,
    Count,
    Instance(u32),
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum FactoryError {
    NotAuthorized = 1,
    IndexOutOfRange = 2,
    NotInitialized = 3,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InstanceDeployed {
    #[topic]
    pub spring: Address,
    #[topic]
    pub tag: String,
    pub instance: Address,
    pub deployer: Address,
    pub index: u32,
}

#[contract]
pub struct Factory;

#[contractimpl]
impl Factory {
    /// `admin` set: only the admin may deploy. `None`: anyone may.
    pub fn __constructor(env: Env, spring: Address, tag: String, admin: Option<Address>) {
        let storage = env.storage().instance();
        storage.set(&DataKey::Spring, &spring);
        storage.set(&DataKey::Tag, &tag);
        if let Some(admin) = admin {
            storage.set(&DataKey::Admin, &admin);
        }
        storage.set(&DataKey::Count, &0u32);
        storage.extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    /// Deploy a contract that runs from the spring's tag and record it.
    ///
    /// The address depends on `deployer` and `salt` together, so nobody can
    /// take an address another deployer computed first.
    pub fn deploy(
        env: Env,
        deployer: Address,
        salt: BytesN<32>,
        constructor_args: Vec<Val>,
    ) -> Result<Address, FactoryError> {
        deployer.require_auth();
        let admin: Option<Address> = env.storage().instance().get(&DataKey::Admin);
        if let Some(admin) = admin {
            if admin != deployer {
                return Err(FactoryError::NotAuthorized);
            }
        }

        let (spring, tag) = fleet(&env);
        let instance = env
            .deployer()
            .with_current_contract(effective_salt(&env, &deployer, &salt))
            .deploy_contract(
                ContractExecutable::ExternalRef(ContractExecutableRef {
                    owner: spring.clone(),
                    tag: tag.clone(),
                }),
                constructor_args,
            );

        let index = count(&env);
        let key = DataKey::Instance(index);
        env.storage().persistent().set(&key, &instance);
        env.storage()
            .persistent()
            .extend_ttl(&key, TTL_THRESHOLD, TTL_EXTEND_TO);
        env.storage().instance().set(&DataKey::Count, &(index + 1));
        env.storage()
            .instance()
            .extend_ttl(TTL_THRESHOLD, TTL_EXTEND_TO);

        InstanceDeployed {
            spring,
            tag,
            instance: instance.clone(),
            deployer,
            index,
        }
        .publish(&env);
        Ok(instance)
    }

    /// The address `deploy` would return for this deployer and salt.
    pub fn deployed_address(env: Env, deployer: Address, salt: BytesN<32>) -> Address {
        env.deployer()
            .with_current_contract(effective_salt(&env, &deployer, &salt))
            .deployed_address()
    }

    pub fn count(env: Env) -> u32 {
        count(&env)
    }

    pub fn instance(env: Env, index: u32) -> Result<Address, FactoryError> {
        if index >= count(&env) {
            return Err(FactoryError::IndexOutOfRange);
        }
        env.storage()
            .persistent()
            .get(&DataKey::Instance(index))
            .ok_or(FactoryError::IndexOutOfRange)
    }

    /// The spring and tag every instance runs from.
    pub fn fleet(env: Env) -> (Address, String) {
        fleet(&env)
    }
}

fn effective_salt(env: &Env, deployer: &Address, salt: &BytesN<32>) -> BytesN<32> {
    let mut preimage = Bytes::new(env);
    preimage.append(&deployer.clone().to_xdr(env));
    preimage.append(&Bytes::from(salt.clone()));
    env.crypto().sha256(&preimage).into()
}

fn count(env: &Env) -> u32 {
    env.storage()
        .instance()
        .get(&DataKey::Count)
        .unwrap_or_else(|| panic_with_error!(env, FactoryError::NotInitialized))
}

fn fleet(env: &Env) -> (Address, String) {
    let storage = env.storage().instance();
    let spring = storage
        .get(&DataKey::Spring)
        .unwrap_or_else(|| panic_with_error!(env, FactoryError::NotInitialized));
    let tag = storage
        .get(&DataKey::Tag)
        .unwrap_or_else(|| panic_with_error!(env, FactoryError::NotInitialized));
    (spring, tag)
}
