# Changelog

## 0.1.0 (unreleased)

- Async WebSocket client with bounded concurrent commands, string request IDs,
  deadlines, cancellation cleanup, optional identity checks, and no command replay.
- Typed payment/readiness/LNURL events, forward-compatible extension fields,
  JSON serialization, independent subscriptions, filters and explicit event gaps.
- Opt-in bounded reconnect and TLS with explicit trust configuration.
- Common command helpers, Rust examples, optional native/JAR process ownership
  with bounded startup and terminate/kill/reap shutdown.
- Separate gRPC gateway with bearer auth, method/event policies, JSON errors,
  bounded event streams, health reporting, and a lossless JSON Node example.
- Protocol fixtures, mock integration tests, feature CI, opt-in live smoke tests,
  source compatibility notes and a reproducible local footprint baseline.

Read-only live attachment and concurrent-client checks passed against the
user-provided instance. Internal readiness probes retry within the connect deadline
to tolerate a missed first broadcast reply; application calls are never replayed.

Not yet published. Managed Nix/JAR and hosted cross-platform validation remain
release gates; see docs/COMPATIBILITY.md. No wallet-mutating tests were run.
