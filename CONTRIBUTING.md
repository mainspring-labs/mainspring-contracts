# Contributing to Mainspring

## Before you start

- Comment on the issue you want so it can be assigned to you. Unassigned PRs for assigned issues will be closed.
- If an issue's acceptance criteria are unclear, ask on the issue before writing code.

## Setup

You need Rust 1.91 or newer with the `wasm32v1-none` target and stellar-cli 25.2.0 or newer.

```bash
make test   # builds example Wasm, then runs all tests
make lint   # rustfmt check + clippy with warnings as errors
```

Tests import `target/wasm32v1-none/release/tipjar_v{1,2}.wasm`. If a test fails to compile with a missing-file error, run `make examples`.

## Code rules

- No `unwrap()` or `expect()` outside tests. Return a `contracterror` variant, or use `panic_with_error!` for storage that the constructor guarantees.
- No floating point. Amounts are `i128`.
- Every state change emits a `#[contractevent]`.
- Every new public function gets tests for its success path and each error it can return.
- Persistent entries get their TTL extended when written.

## Commits and PRs

- One logical change per commit: one function, one type, one test block.
- Conventional commits: `type(scope): description`. Types: `feat`, `fix`, `test`, `docs`, `refactor`, `build`, `ci`, `chore`, `style`. Scopes: `spring`, `factory`, `instance`, `examples`, `workspace`.
- Stage files by name. Don't `git add .`.
- `make lint test` must pass locally before you open the PR. CI runs the same steps.
- Reference the issue in the PR description (`Closes #12`).
