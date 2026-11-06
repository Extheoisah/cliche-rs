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
feature enables `wss://`; it is disabled by default. The `process` feature enables native/JAR supervision. The gateway is a separate
crate; direct client users do not build its gRPC stack.

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

## Managed cliche

```sh
cargo run -p cliche-client --features process --example start -- \
  /path/to/cliche-nix/result/bin/cliche /path/to/dedicated-data 12001 testnet
```

Use `process::{ProcessConfig, ManagedCliche}` to launch a native binary or JAR.
Configure the seed in the data directory out of band. The manager never runs Nix,
installs cliche, or reads seed/database files. It retains stdin and drains/discards
stdout/stderr because diagnostic output can contain secrets. Call `shutdown()`
to terminate and reap the child; drop signals best-effort cleanup while the Tokio
runtime is alive. Cloning the client does not keep the owned process alive.

The supervisor binds cliche to IPv4 loopback, rejects occupied ports, and locks
its data directory against other SDK managers. cliche itself does not honor that
lock; unrelated processes can still race startup. Use a dedicated port/directory
and optional expected pubkey. Never run two instances against the same wallet.
On Unix shutdown sends SIGTERM, then force-kills after the configured grace period;
other platforms use forceful termination. No wallet-level shutdown RPC is assumed.

## gRPC gateway and Node

The gateway build requires `protoc` (e.g. `protobuf-compiler` on Debian or
`protobuf` in Homebrew). The client crate has no protoc dependency.

```sh
cargo run -p cliche-gateway -- --cliche-url ws://127.0.0.1:12000/ --listen 127.0.0.1:12001
# Explicitly allow payment initiation if your application needs it:
cargo run -p cliche-gateway -- --allow-methods ping,get-info,create-invoice,pay-invoice,check-payment,list-payments
```

The default method allowlist is read-only; `--allow-methods '*'` grants full
command access. Set `CLICHE_GATEWAY_TOKEN` for bearer authentication. Event access
is wallet-wide and separately disabled with `--disable-events`; method allowlists
do not provide tenant isolation for events. The standalone server exposes standard
gRPC health at service name `cliche.v1.ClicheService`. Health reports connection
readiness, not channel liquidity or settlement readiness.

The gateway supports `Call`, `GetStatus`, and streaming `SubscribeEvents` using
[the protobuf contract](crates/cliche-gateway/proto/cliche/v1/cliche.proto).
Responses/events are JSON documents inside protobuf strings. Upstream errors are
JSON application envelopes; gateway errors are gRPC statuses with JSON details
where generated locally. Client/proxy deadline failures may omit these details.
A stalled event stream terminates with RESOURCE_EXHAUSTED; resubscribe and reconcile.

[The Node example](examples/node/README.md) includes bearer metadata, streaming,
and lossless JSON handling. To launch owned cliche from the gateway, use
`--executable PATH --data-dir PATH --cliche-port PORT --network testnet`.
No RPC accepts executable paths or remotely starts arbitrary processes.

Plain gRPC binds to loopback by default. For nonloopback binds the CLI requires
`--trusted-proxy` and a nonempty token: supply TLS termination/access controls
in the deployment proxy and keep its backend network private. Direct server TLS
and reflection are not implemented. The embeddable `Gateway` service can be added
to a caller-configured Tonic server; its caller owns transport security.
