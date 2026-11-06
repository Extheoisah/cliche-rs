# Node gRPC example

Node 20+ and a running cliche-gateway are required. From this directory:

```sh
npm ci
npm start -- 127.0.0.1:12001
npm test
```

Set `CLICHE_GATEWAY_TOKEN` in the environment if the gateway requires a token.
The example's default channel is plaintext for loopback. For remote deployment,
pass `grpc.credentials.createSsl(...)` to `connect` and use a trusted authenticated
TLS endpoint. Never expose the plain cliche WebSocket publicly.

`call` returns a versioned JSON document, parsed with lossless-json. Numbers are
`LosslessNumber` objects, including small integers. Use `.toString()` or explicit
safe conversions; do not turn millisatoshi values into JavaScript Number without
checking range. Use the exported lossless `stringify` for outgoing parameters.
`call` resolves cliche application failures with `kind: "error"`; gateway failures
reject with a gRPC status. JSON status details, when present, are attached as
`error.cliche`. Client/proxy-generated deadline failures may have no details.

Notifications are JSON serialized from the Rust client's typed event structs.
The example subscribes before ping, and prints selected received-payment fields
without printing preimages. Handle gap/decode-error records by reconciling your
own persisted payment hashes. An event stream is live and has no replay cursor.

A real Node -> gRPC -> Rust -> mock WebSocket roundtrip test is provided:

```sh
# From the repository root, after npm ci in examples/node:
cargo test -p cliche-gateway --test gateway node_lossless_roundtrip -- --ignored --nocapture
```

It spawns disposable loopback mocks and checks exact command/event integers above
2^53. `test.mjs` is for that harness and must not target a live wallet.
