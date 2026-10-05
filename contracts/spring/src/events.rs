use soroban_sdk::{contractevent, Address, BytesN, String};

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagCreated {
    #[topic]
    pub tag: String,
    pub wasm_hash: BytesN<32>,
    pub min_delay: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeProposed {
    #[topic]
    pub tag: String,
    pub wasm_hash: BytesN<32>,
    pub eta: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeExecuted {
    #[topic]
    pub tag: String,
    pub version: u32,
    pub wasm_hash: BytesN<32>,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UpgradeCancelled {
    #[topic]
    pub tag: String,
    pub by: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RolledBack {
    #[topic]
    pub tag: String,
    pub version: u32,
    pub wasm_hash: BytesN<32>,
    pub by: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MinDelayIncreased {
    #[topic]
    pub tag: String,
    pub min_delay: u64,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminTransferStarted {
    pub current: Address,
    pub pending: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AdminTransferred {
    pub previous: Address,
    pub admin: Address,
}

#[contractevent]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GuardianChanged {
    pub previous: Address,
    pub guardian: Address,
}
