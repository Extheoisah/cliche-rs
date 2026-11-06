# Contributing

Implementation follows the milestones in [the plan](docs/IMPLEMENTATION_PLAN.md).
Until a Cargo workspace exists, there are no Rust build or test commands to run.

## Code style

Use Rust 2024, rustfmt, Clippy, and a documented minimum supported Rust version
(MSRV) verified against dependencies. Use Tokio without creating an internal
runtime. Prefer concrete internal types and small functions; introduce traits
only when required by a real alternative implementation or testing seam.

Public APIs need rustdoc, useful examples, explicit units, and documented timeout,
shutdown, and event-loss behavior. Return typed errors; avoid panics, `unwrap`,
and `expect` in library execution paths. Test setup may use them with clear
context. Do not hold locks across `.await` or invoke user callbacks from the
socket reader. Preserve the original source of errors without printing secrets.

Use Rust `snake_case` internally and explicit serde field renames for cliche's
wire names. Use `Option` for optional parameters and integer types for amounts.
Use typed event enums and payload structs for known notifications; generic JSON
is reserved for command results and explicit forward-compatibility extensions.
Keep core dependencies minimal and add features for optional costs. Avoid
speculative abstractions, unsafe code, and unnecessary allocations or clones.

## Commits and pull requests

Use focused Conventional Commits: `feat(client): correlate websocket replies`,
`fix(events): report slow subscriber gaps`, `docs: explain startup ownership`.
Keep commits buildable after the initial workspace bootstrap. Include tests and
documentation with the behavior they describe; separate unrelated refactors.
Never commit wallet data, seeds, generated build output, or machine-specific paths.

A pull request should explain the user-visible behavior, compatibility impact,
and validation performed. Report checks that could not run. Keep protocol
fixtures synthetic and tied to a documented upstream revision. Attribute any
adapted source code and preserve its license requirements.

## Validation once implemented

Run rustfmt, Clippy with warnings denied, tests, doctests, and rustdoc with warnings
denied. Exercise both default and supported optional feature combinations, not
just `--all-features`. Integration tests use a deterministic fake WebSocket server
and fake child process. Live cliche tests are opt-in, read-only by default, and
use a disposable configuration for any separately authorized mutations.

The proposed project license is Apache-2.0, matching both reference projects.
Add the complete license and Cargo metadata before public distribution; no code
has been copied from either reference project in this planning scaffold.
