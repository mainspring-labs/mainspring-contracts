//! Mainspring spring: the owner contract for CAP-85 executable references.
#![no_std]

mod error;
mod events;
mod storage;

pub use error::SpringError;
pub use storage::{Proposal, TagState, VersionKind, VersionRecord, MAX_DELAY};
