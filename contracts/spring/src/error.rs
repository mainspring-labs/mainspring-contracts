use soroban_sdk::contracterror;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum SpringError {
    NotAuthorized = 1,
    TagExists = 2,
    TagNotFound = 3,
    ProposalPending = 4,
    NoProposal = 5,
    TooEarly = 6,
    SameHash = 7,
    NothingToRollBack = 8,
    DelayTooLong = 9,
    DelayNotIncreased = 10,
    NoPendingAdmin = 11,
    InvalidTag = 12,
}
