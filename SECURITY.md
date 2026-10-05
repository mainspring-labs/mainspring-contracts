# Security Policy

## Audit status

Mainspring has **not been audited**. It builds on Protocol 28 (CAP-85) and soroban-sdk 28.0.0, both released in September 2026. Treat every deployment as experimental until an audit is published in this file.

## Scope

In scope:

- `contracts/spring`: any way to change a tag's Wasm hash without passing its timelock, to lower a timelock, to make the guardian introduce new code, or to break the version history.
- `contracts/factory`: any way to deploy an instance at an address another deployer derived, to bypass the deploy policy, or to corrupt the instance list.
- `crates/mainspring-instance`: any way `pin` can leave an instance attached to the fleet, or attach it to the wrong code.

Out of scope: the example tip jars, issues that need a compromised admin key (the timelock and pinning exist to limit that case, so report a way around them instead), and bugs in Stellar Core or soroban-sdk themselves. Report those upstream.

## Reporting a vulnerability

Do not open a public issue. Use GitHub's private reporting at **Security → Report a vulnerability** on this repository.

Please include the affected contract and function, a reproduction (a failing test is ideal), and the impact. You'll get an acknowledgement within 72 hours and a status update within 7 days.
