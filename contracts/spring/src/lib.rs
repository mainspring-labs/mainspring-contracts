//! Mainspring spring: the owner contract for CAP-85 executable references.
#![no_std]

mod contract;
mod error;
mod events;
mod storage;

#[cfg(test)]
mod test;

pub use contract::{Spring, SpringClient};
pub use error::SpringError;
pub use storage::{Proposal, TagState, VersionKind, VersionRecord, MAX_DELAY};
