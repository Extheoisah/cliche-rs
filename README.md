# cliche-rs

Async Rust client for [cliche](https://github.com/nbd-wtf/cliche): WebSocket commands,
typed events, JSON serialization, bounded concurrency, and optional process ownership.
Rust 1.94 or later; Tokio runtime required. Apache-2.0.

## Rust integration

```toml
[dependencies]
cliche-client = { path = "/path/to/cliche-rs/crates/cliche-client" }
serde_json = "1"
tokio = { version = "1", features = ["macros", "rt"] }
```

```rust,no_run
use cliche_client::{Client, events::{ClicheEvent, EventRecord}};

# async fn example() -> Result<(), Box<dyn std::error::Error>> {
let client = Client::connect("ws://127.0.0.1:12000/").await?;
let mut events = client.subscribe();
let info = client.get_info().await?;
while let Some(delivery) = events.recv().await {
    match delivery.record {
        EventRecord::Notification { event: ClicheEvent::PaymentReceived(payment) } => {
            // Optional because upstream can omit the amount.
            let amount_msat = payment.msatoshi;
        }
        EventRecord::Gap { .. } => { /* reconcile persisted hashes */ }
        _ => {}
    }
}
client.close().await;
# Ok(())
# }
```

Known events have named Rust payload structs. `serde_json::to_string(&delivery)`
produces JSON with version, subscription ID, sequence, kind, method and params.
Only unknown methods and extension fields use raw JSON. Commands return JSON;
`call(method, json_object)` supports every upstream method. Common helpers cover
ping, get-info, invoice creation/payment, check-payment, recent payments, and address.

`Client::builder` configures timeouts, size/capacity limits, optional expected node
pubkey/chain, and opt-in `ReconnectPolicy`. Reconnect never replays commands.
All clones share a connection; `close()` closes it for every clone. Last-handle
drop signals cleanup. Subscribe before sending commands whose events matter.

Events are live and best effort. Slow consumers get a typed gap record; disconnects
have unknown loss counts. Local sequence numbers do not provide replay. A payment
initiation response is not final settlement. Timeouts and cancellation can happen
after execution begins: reconcile with `check_payment` before retrying. Unknown
payment hashes are upstream errors. Recent payments default to five entries.

## Examples and validation

```sh
cargo run -p cliche-client --example attach -- ws://127.0.0.1:12000/
cargo run -p cliche-client --example events -- ws://127.0.0.1:12000/
cargo test --workspace --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Tests use disposable local mock servers, never your running wallet. The `tls`
feature enables `wss://`; it is disabled by default. The process supervisor and
gRPC gateway are tracked in the remaining milestones.

## Design and progress

- [Milestone progress](docs/PROGRESS.md)
- [Implementation plan](docs/IMPLEMENTATION_PLAN.md)
- [Source-verified protocol findings](docs/PROTOCOL.md)
- [Contributing](CONTRIBUTING.md)

Crate names are provisional and have not been checked for registry availability.
Source-reviewed compatibility is pinned in PROTOCOL.md; live Nix compatibility
has not yet been verified. cliche's WebSocket broadcasts wallet-wide replies and
events; keep it on a trusted private interface. Debug output redacts event payloads,
but explicitly serialized JSON contains secrets such as payment preimages.
