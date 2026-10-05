//! Example fleet instance, version 2: adds `withdraw_to`.
//!
//! Same storage as `tipjar-v1`, so a spring can move a fleet between the two
//! versions without a data migration.
#![no_std]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Env,
};

const VERSION: u32 = 2;
const TTL_THRESHOLD: u32 = 518_400;
const TTL_EXTEND_TO: u32 = 3_110_400;

#[contracttype]
pub enum DataKey {
    Owner,
    Token,
}

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum TipJarError {
    InvalidAmount = 1,
    NotInitialized = 2,
}

#[contract]
pub struct TipJar;

#[contractimpl]
impl TipJar {
    pub fn __constructor(env: Env, owner: Address, token: Address) {
        env.storage().instance().set(&DataKey::Owner, &owner);
        env.storage().instance().set(&DataKey::Token, &token);
    }

    pub fn tip(env: Env, from: Address, amount: i128) -> Result<(), TipJarError> {
        from.require_auth();
        if amount <= 0 {
            return Err(TipJarError::InvalidAmount);
        }
        token_client(&env).transfer(&from, &env.current_contract_address(), &amount);
        Ok(())
    }

    pub fn withdraw(env: Env, amount: i128) -> Result<(), TipJarError> {
        let owner = owner(&env);
        owner.require_auth();
        if amount <= 0 {
            return Err(TipJarError::InvalidAmount);
        }
        token_client(&env).transfer(&env.current_contract_address(), &owner, &amount);
        Ok(())
    }

    /// Added in v2: withdraw to any address.
    pub fn withdraw_to(env: Env, to: Address, amount: i128) -> Result<(), TipJarError> {
        owner(&env).require_auth();
        if amount <= 0 {
            return Err(TipJarError::InvalidAmount);
        }
        token_client(&env).transfer(&env.current_contract_address(), &to, &amount);
        Ok(())
    }

    /// Detach this jar from the fleet onto the code it runs now.
    pub fn pin(env: Env) {
        owner(&env).require_auth();
        mainspring_instance::pin(&env);
    }

    pub fn extend_ttl(env: Env) {
        mainspring_instance::extend_ttl(&env, TTL_THRESHOLD, TTL_EXTEND_TO);
    }

    pub fn owner(env: Env) -> Address {
        owner(&env)
    }

    pub fn version(_env: Env) -> u32 {
        VERSION
    }
}

fn owner(env: &Env) -> Address {
    env.storage()
        .instance()
        .get(&DataKey::Owner)
        .unwrap_or_else(|| panic_with_error!(env, TipJarError::NotInitialized))
}

fn token_client(env: &Env) -> token::Client<'_> {
    let token: Address = env
        .storage()
        .instance()
        .get(&DataKey::Token)
        .unwrap_or_else(|| panic_with_error!(env, TipJarError::NotInitialized));
    token::Client::new(env, &token)
}
