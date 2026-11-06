# cliche-client

Lightweight async Rust client for cliche. WebSocket commands return JSON; known
notifications are Rust enums and structs that serialize to JSON. Requires Tokio
and Rust 1.94. Optional `tls` and `process` features are disabled by default.

```rust,no_run
use cliche_client::{Client, events::{ClicheEvent, EventRecord}};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = Client::connect("ws://127.0.0.1:12000/").await?;
let mut events = client.subscribe();
let info = client.get_info().await?;
while let Some(delivery) = events.recv().await {
    match delivery.record {
        EventRecord::Notification { event: ClicheEvent::PaymentReceived(payment) } => {
            let amount_msat: Option<i64> = payment.msatoshi;
        }
        EventRecord::Gap { .. } => { /* reconcile persisted hashes */ }
        _ => {}
    }
}
client.close().await;
# Ok(())
# }
```

Use `call` for arbitrary methods. Reconnect is explicitly configured with
`ReconnectPolicy` and never replays commands. Timeouts/cancellation can occur
after execution has begun; reconcile payment hashes before retrying. Events are
live, bounded, and cannot be replayed upstream. Slow receivers get typed gap
records. Payload debug output is redacted; explicit JSON serialization includes
sensitive fields such as preimages. Keep cliche's wallet-wide WebSocket private.

With `process`, use `process::{ProcessConfig, ManagedCliche}` to launch an
operator-supplied native executable or JAR with dedicated data and port settings.
Seed configuration stays operator-owned. Explicitly call `shutdown()` to terminate
and reap the owned child. Runtime teardown can prevent asynchronous drop cleanup.
No cliche binary, wallet implementation, Nix build tooling, gRPC, or protobuf
compiler is bundled with this crate. Run `cargo doc --open` for the public API.
