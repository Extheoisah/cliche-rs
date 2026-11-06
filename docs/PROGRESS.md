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
3. Event subscriptions/reconnect: pending.
4. Helpers/examples: pending.
5. Optional process management: pending.
6. gRPC gateway/Node example: pending.
7. Compatibility/release checks: pending. Live Nix compatibility needs an isolated
   Linux/Nix environment; source review is not live verification.
