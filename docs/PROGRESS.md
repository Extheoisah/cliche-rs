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
7. Compatibility/release checks: pending. Live Nix compatibility needs an isolated
   Linux/Nix environment; source review is not live verification.
