# cliche-rs

A lightweight Rust library for sending commands to [cliche](https://github.com/nbd-wtf/cliche)
over WebSocket and receiving typed events. Requires Rust 1.94 or later.

- **cliche-client** is the library for Rust applications.
- **cliche-gateway** exposes the library through gRPC for Node.js and other languages.

The examples below connect to cliche already running at `ws://127.0.0.1:12000/`.

## Use it in a Rust project

Add this to your project's `Cargo.toml`:

```toml
[dependencies]
cliche-client = { git = "https://github.com/Extheoisah/cliche-rs.git" }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
serde_json = "1"
```

Cargo fetches the crate directly from GitHub. You can add `rev = "COMMIT_SHA"`
to pin a specific commit. See [Cargo's Git dependency documentation](https://doc.rust-lang.org/cargo/reference/specifying-dependencies.html#specifying-dependencies-from-git-repositories).

### Send commands

Put this in `src/main.rs`, then run `cargo run`:

```rust,no_run
use cliche_client::{Client, CreateInvoiceParams};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::connect("ws://127.0.0.1:12000/").await?;

    let info = client.get_info().await?;
    println!("{info}");

    // Use call() for any cliche command. Results are JSON values.
    let payments = client
        .call("list-payments", json!({"count": 10}))
        .await?;
    println!("{payments}");

    let invoice = client
        .create_invoice(CreateInvoiceParams {
            msatoshi: Some(120_000),
            description: Some("Example invoice".into()),
            ..Default::default()
        })
        .await?;
    println!("{invoice}");

    client.close().await;
    Ok(())
}
```

Amounts are integer numbers in millisatoshis. Creating an invoice requires
receiving capacity in cliche. Other helpers include `ping`, `pay_invoice`,
`check_payment`, `list_payments`, and `get_address`.

### Listen for typed events

This is a separate runnable example. Subscribe before the activity you want to
observe; the stream waits for events as they happen.

```rust,no_run
use cliche_client::{Client, events::{ClicheEvent, EventRecord}};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::connect("ws://127.0.0.1:12000/").await?;
    let mut events = client.subscribe();

    while let Some(delivery) = events.recv().await {
        match delivery.record {
            EventRecord::Notification {
                event: ClicheEvent::PaymentReceived(payment),
            } => {
                println!(
                    "Received payment {}: {:?} msat",
                    payment.payment_hash, payment.msatoshi
                );
            }
            EventRecord::Gap { .. } => {
                // Check your persisted payment hashes with check_payment().
                eprintln!("Missed events; reconcile payment state.");
            }
            _ => {}
        }
    }

    client.close().await;
    Ok(())
}
```

`msatoshi` is optional because cliche sometimes omits it from received-payment
events. Run the event loop in a separate Tokio task if your application also needs
to keep sending commands. `Client` is cloneable; clones share the same connection.
Calling `close()` closes that connection for all clones.

Events can also be serialized with `serde_json::to_string(&delivery)`. Payment
payloads can contain sensitive preimages. Events are live and cannot be replayed;
a timeout does not prove a payment failed, so check its state before retrying.

### Optional features

Enable `tls` for `wss://` connections or `process` to start cliche from your app:

```toml
cliche-client = { git = "https://github.com/Extheoisah/cliche-rs.git", features = ["tls", "process"] }
```

With `process`, use `process::{ProcessConfig, ManagedCliche}` and call
`shutdown().await` when finished. See the [startup example](crates/cliche-client/examples/start.rs).
Connecting to an existing instance does not give the library ownership of it.

## Use it from Node.js

The Node client connects through the gRPC gateway. It is an example JavaScript
client, not a native Rust binding or a published npm package.

First install `protoc` (`brew install protobuf` on macOS or
`apt install protobuf-compiler` on Debian/Ubuntu), then start the gateway:

```sh
git clone https://github.com/Extheoisah/cliche-rs.git
cd cliche-rs
cargo run -p cliche-gateway -- --cliche-url ws://127.0.0.1:12000/
```

In a second terminal, from the cloned repository:

```sh
cd examples/node
npm ci
npm start -- 127.0.0.1:12001
```

The example sends a ping and listens for received payments. To use its client
helper from a JavaScript file in the repository root:

```js
import { connect, parse, stringify } from './examples/node/client.mjs';

const client = connect('127.0.0.1:12001');
const events = client.subscribe(['payment_received']);

events.on('data', ({ json }) => {
  const delivery = parse(json);
  if (delivery.method === 'payment_received') {
    console.log(stringify({
      payment_hash: delivery.params.payment_hash,
      msatoshi: delivery.params.msatoshi,
    }));
  }
});
events.on('error', error => console.error(`Event stream ended (${error.code}).`));

console.log(stringify(await client.call('get-info')));

process.on('SIGINT', () => {
  events.cancel();
  client.close();
});
```

The helper uses lossless JSON to preserve large monetary integers. Read the
[Node examples README](examples/node/README.md) for request results, error handling,
authentication options, and tests.

All gateway commands are available without a token by default. It listens on
`127.0.0.1:12001`; use `--listen 0.0.0.0:12001` for access from other machines.
`--allow-methods` and `CLICHE_GATEWAY_TOKEN` optionally restrict access.

## More information

- [Node examples](examples/node/README.md)
- [gRPC contract](crates/cliche-gateway/proto/cliche/v1/cliche.proto)
- [Protocol and supported commands](docs/PROTOCOL.md)
- [Compatibility and validation](docs/COMPATIBILITY.md)
- [Contributing](CONTRIBUTING.md)

Licensed under [Apache-2.0](LICENSE).
