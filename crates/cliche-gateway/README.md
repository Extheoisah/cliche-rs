# cliche-gateway

JSON gRPC adapter for the cliche-client WebSocket library. Rust 1.94 and protoc
are required to build this package. The standalone binary defaults to loopback,
all command methods without authentication, and wallet-wide event access.

```sh
cliche-gateway --cliche-url ws://127.0.0.1:12000/ --listen 127.0.0.1:12001
```

Authentication is optional; set `CLICHE_GATEWAY_TOKEN` only to enable it. Use `--allow-methods` to
configure commands and `--disable-events` to disable wallet-wide subscriptions.
Use `--listen 0.0.0.0:12001` for public network access without a token requirement.
The binary does not implement direct TLS. The `Gateway` service is also embeddable
in your own Tonic server.

The `cliche.v1.ClicheService` RPCs are Call, SubscribeEvents, and GetStatus. JSON
is carried inside protobuf strings, preserving integer tokens. Event JSON comes
from typed Rust notifications. Upstream errors are JSON envelopes; local gateway
failures are gRPC statuses with JSON details when available. Client/proxy-generated
timeout statuses may omit details. Events are live with no replay guarantee;
reconcile payments after a gap or an uncertain command result. JavaScript clients
must use lossless JSON parsing for signed 64-bit monetary integers.
