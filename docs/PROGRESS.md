# Milestone progress

1. Workspace/protocol: complete. Typed events, safe serializable errors, parser,
   protocol/roundtrip fixtures, minimal default features, license, CI scaffold.
   Validated with `cargo test` and Clippy (warnings denied) on Rust 1.94.
2. Concurrent WebSocket commands: complete. Bounded admission, session IDs,
   independent socket reader/writer, deadlines, drop-safe pending cleanup, shared
   close. Mock tests cover 100 out-of-order calls, foreign/null replies,
   cancellation, timeout recovery, and disconnect. Clippy passes.
   Design refinement: a short synchronous mutex protects the pending map so
   dropped futures remove waiters immediately without a cancellation queue.
   No locks span await points.
3. Event subscriptions/reconnect: complete. Typed local deliveries with subscription
   IDs/sequences, filters, lag/decode records, terminal status, bounded jittered
   reconnect and optional identity checks. Tests verify slow-consumer isolation
   and no command replay. Tests and Clippy pass.
4. Helpers/examples: complete. MVP command helpers validate params, preserve
   exact wire casing and optional fields, and return JSON. Rust attach/typed-event
   examples compile. Wire tests include monetary integers above 2^53.
5. Optional process management: complete. Native/JAR launch, dedicated-directory
   ownership lock, busy-port preflight, bounded readiness, drained pipes, exit
   monitoring and terminate/kill/reap. Fake-child tests cover pipe pressure, early
   exit, timeout, cancellation, duplicate ownership and occupied ports. All-feature
   tests and Clippy pass on macOS; actual Linux/Nix signaling remains unverified.
6. gRPC gateway/Node example: complete. Separate tonic/prost crate with JSON
   command results, typed-event serialization, bearer auth, method/event policy,
   deadline details, bounded subscriptions, stall termination and health. Real
   HTTP/2 tests cover concurrent callers, integer precision, auth and timeouts.
   Node lossless parser and mock-backed gRPC roundtrip tested separately.
7. Compatibility/release preparation: local checks complete; external release
   gates remain. Added opt-in read-only/live managed smoke tests, CI feature/MSRV/
   platform jobs, package READMEs/licenses, changelog, baseline and compatibility
   notes. Default, TLS, process and full workspace tests pass locally; Clippy,
   rustdoc and doctests pass with warnings denied. Node mock gRPC roundtrip passes.
   Archive content checks and unpacked-source compilation pass. Cargo 1.94's
   offline workspace verifier hit a temporary-registry checksum bug; a repeatable
   local-patch package checker covers both unpacked crates instead.
   Managed Nix/Linux/JAR checks, hosted CI runs, final registry names/repository URLs,
   and publication remain open release gates (COMPATIBILITY.md). Nothing published.

Final validation also fixed explicit TLS crypto-provider selection (tested trusted
and untrusted local certificates) and immediately stops shared clients when managed
ownership drops. Fixture startup allows macOS first-execution assessment time.
Runtime refinements and unsupported options are listed in COMPATIBILITY.md.

Read-only live attachment update (2026-11-06): ping/get-info and two-client
identity/concurrency checks passed against the user-provided localhost:12000
instance. Initial connection timeouts led to bounded internal readiness-ping
retries, with a regression test for a lost first reply. Application calls remain
non-replayed. Actual binary provenance, managed Nix execution and payment-event
emission are separate unverified checks; see COMPATIBILITY.md.

Gateway access defaults updated at the user's request: every command is allowed
without authentication. Public listen addresses no longer require a token/proxy
flag. Loopback remains the default listen address; explicit token/method policies
are still supported. Invoice command access is regression-tested using mocks.
