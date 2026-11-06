# Protocol findings

Inspected on 2026-11-06. Findings below come from source, not a live wallet session.
The original inspection used source only. Subsequent read-only live attachment
tests are recorded in COMPATIBILITY.md; no wallet databases/configuration were read.

## Revisions and evidence

| Source | Inspected revision | Relevant files |
| --- | --- | --- |
| nbd-wtf/cliche | `75ac2cadc77290127781c345cea823b13957320d` | `ServerApp.scala`, `Handler.scala`, `Commands.scala`, `Main.scala`, `Config.scala`, `reference.conf` |
| fiatjaf/go-cliche | `f2a5e32695ac6d76b3d3ea75cb89af7b492153d8` | `cliche.go`, `commands.go`, `types.go` |
| Local cliche-nix checkout | HEAD `00e9d8e12a14f01205e10ef6e3d450a954bddbfc` | `flake.lock`, `flake.nix`, patches, README |

The local Nix lock pins cliche to the inspected upstream revision. The checkout's
build applies testnet sync selection, Immortan sync nodes, updated checkpoints,
and native-image resource/build changes. Inspection found no WebSocket protocol
patches. This establishes a source baseline, not proof of the running binary's
build provenance. Verify that binary separately during the live compatibility step.

Permanent source links:

- [WebSocket server](https://github.com/nbd-wtf/cliche/blob/75ac2cadc77290127781c345cea823b13957320d/src/main/scala/ServerApp.scala)
- [Request decoding and parameter names](https://github.com/nbd-wtf/cliche/blob/75ac2cadc77290127781c345cea823b13957320d/src/main/scala/Handler.scala)
- [Response rendering and command behavior](https://github.com/nbd-wtf/cliche/blob/75ac2cadc77290127781c345cea823b13957320d/src/main/scala/Commands.scala)
- [Startup and event publishers](https://github.com/nbd-wtf/cliche/blob/75ac2cadc77290127781c345cea823b13957320d/src/main/scala/Main.scala)
- [Default configuration](https://github.com/nbd-wtf/cliche/blob/75ac2cadc77290127781c345cea823b13957320d/src/main/resources/reference.conf)
- [Go event types](https://github.com/fiatjaf/go-cliche/blob/f2a5e32695ac6d76b3d3ea75cb89af7b492153d8/types.go)
- [Go controller](https://github.com/fiatjaf/go-cliche/blob/f2a5e32695ac6d76b3d3ea75cb89af7b492153d8/cliche.go)
- [Nix packaging project](https://github.com/takinbrrrr/cliche-nix)

## WebSocket and JSON

The default endpoint is `ws://127.0.0.1:12000/`. `reference.conf` and `Config.scala`
allow `cliche.websocket.host` and `cliche.websocket.port` overrides. The README's
claim that the endpoint is hardcoded is outdated for this revision.

The server upgrades GET `/`; POST `/` returns a marker and is not a JSON-RPC HTTP
endpoint. Each WebSocket text message is passed to the command handler. Outgoing
messages are compact JSON independently of the stdout formatting setting.

Send string IDs and object parameters:

```json
{"jsonrpc":"2.0","id":"<random-session>:1","method":"ping","params":{}}
```

```json
{"jsonrpc":"2.0","id":"<random-session>:1","result":{"ping":"pong"}}
```

```json
{"jsonrpc":"2.0","id":"<random-session>:2","error":{"message":"unhandled example","code":1}}
```

```json
{"jsonrpc":"2.0","method":"payment_received","params":{"payment_hash":"<hash>","preimage":"<secret>","msatoshi":12000}}
```

The request decoder casts IDs to strings and reads `method` and `params`; it does
not enforce a complete JSON-RPC 2.0 implementation. Batch calls, numeric IDs,
notification requests, and protocol cancellation must not be assumed supported.
Some parsing errors lose the original ID and return an empty string ID; callers
may time out instead of receiving a correlated error. Some command exceptions
may also fail without a correlated reply. Deadlines are essential.

Every connected WebSocket subscribes to the SAME topic as stdout. Therefore all
clients receive all command replies and notifications, including replies to
commands issued through stdin or other clients. Correlate only IDs owned by the
current client session. Never forward other clients' replies as events. There is
no per-user event isolation, authentication, or TLS in the inspected server code.

The server subscription buffer is 25 messages. Keep socket reads progressing;
a stalled connection can pressure upstream publication. There are no durable
event offsets or replay facilities. Notifications can be missed during disconnects.

## Commands supported by the inspected handler

These are JSON parameter spellings from Scala case classes and Spray converters.
CLI flag spellings in the README are not the JSON contract.

| Method | JSON params | Wrapper priority |
| --- | --- | --- |
| `ping` | `{}` | Initial connection/readiness |
| `get-info` | `{}` | MVP |
| `request-hc` | `pubkey`, `host`, `port`, optional `label`, `secret` | Next |
| `remove-hc` | `channelId` | Next |
| `create-invoice` | optional `description`, `description_hash`, `msatoshi`, `preimage`, `label` | MVP |
| `pay-invoice` | `invoice`, optional `msatoshi` | MVP |
| `pay-lnurl` | `lnurl`, `msatoshi`, optional `comment`, `name`, `attachAuth` | Next |
| `check-payment` | `hash` | MVP |
| `list-payments` | optional `count` (upstream default 5) | MVP |
| `list-txs` | optional `count` | Next |
| `get-address` | `{}` | MVP |
| `send-to-address` | `address`, optional `satoshi` | Next |
| `open-nc` | `pubkey`, `host`, `port`, optional `satoshi` | Next |
| `close-nc` | `channelId` | Next |
| `accept-override` | `channelId` | Next |
| `resize-hc` | `channelId`, `satoshiDelta` | Next |

Verify omitted `attachAuth` against the pinned decoder in a contract test; typed
helpers should send it explicitly. Signed `satoshiDelta` supports resize changes.
Amounts enter Scala `Long`; validate values against signed 64-bit limits. The
library's generic call remains available for every method from the first release.

A successful `pay-invoice` reply reports command initiation, not final settlement.
Final state arrives through notifications or `check-payment`. `check-payment` for
an unknown hash is an upstream error rather than a clean "not found" variant.
`list-payments` is a limited recent list, so a default query cannot prove recovery
of every missed event. Persist known payment hashes in the consuming application.

## Notifications and readiness

Known methods are `ready`, `payment_succeeded`, `payment_failed`,
`payment_received`, `lnurlpay_params`, and `lnurlpay_success`. Preserve additional
methods and unknown fields. Do not use the old README's `{"event":"ready"}` form
as the primary decoder: the inspected renderer uses `method` and `params`.

`ready` is published once after a three-second scheduled delay after initialization.
It is not replayed for new connections and is not proof that channel synchronization
or payments are usable. Attach/readiness uses a correlated `ping` (internal probes may repeat within the
connect deadline to tolerate output-subscription startup), with
optional `get-info` identity/network validation. Events begin only after subscription;
never claim to deliver events that occurred before connecting.

`payment_received` can omit `msatoshi` when its payment lookup fails.
`payment_succeeded` does not emit the Go type's `parts` field in this revision.
LNURL notifications do not provide dependable originating request IDs; concurrent
LNURL calls cannot be reliably matched to those intermediate notifications.
Do not invent correlation by arrival order.

The public Rust API will expose dedicated typed event structs through a
`ClicheEvent` enum, using these source-verified optional fields rather than
copying Go zero-value behavior. See the typed event contract in
[the implementation plan](IMPLEMENTATION_PLAN.md#typed-event-contract).

## Process interface and Go reference

Native launch: `cliche -Dcliche.datadir=/path -Dcliche.json.compact=true`.
JAR launch: `java -Dcliche.datadir=/path -Dcliche.json.compact=true -jar cliche.jar`.
Additional host/port properties follow the same `-D` convention. Provide explicit
argument arrays, never interpolate a shell command. A valid seed must already be
configured by the operator. Missing-seed startup can print a generated mnemonic;
raw process output is sensitive and must not be logged automatically.

The Go client demonstrates subprocess launch, compact JSON lines, pending request
IDs, typed commands, and separate payment event channels. Do not copy its shared
unprotected pending map, missing pending cleanup, indefinite waits, or synchronous
event-channel sends inside the reader. Those choices can race, leak, or stall replies.

Keep owned child stdin open even when commands use WebSocket: upstream runs a
continuous stdin reader, and EOF behavior must be validated. Drain stdout and
stderr separately with bounded diagnostic retention. Do not decode stdout events
and WebSocket events together, which would duplicate delivery.

The local Nix build targets `x86_64-linux` and depends on its Nix runtime closure.
The Rust library should not embed or rebuild cliche, install Nix, or pretend the
Nix executable is a portable binary. Accept a configured executable path. Build
and package the Rust client independently of cliche's Scala/GraalVM toolchain.
