# Implementation plan

Design baseline: 2026-11-06. See [protocol evidence](PROTOCOL.md) for inspected
revisions and exact upstream behavior. This document specifies proposed behavior;
none of the APIs below are implemented yet.

## 1. Product boundary and decisions

Build a Tokio-based Rust library, `cliche-client`, that attaches to cliche over
WebSocket, correlates JSON replies, and exposes asynchronous typed Rust events
that serialize to JSON.
An optional `process` feature starts and supervises a local cliche process before
connecting through the same WebSocket client. A separate `cliche-gateway` crate
exposes the client through gRPC for Node.js, Go, Python, and other languages.

| Concern | Decision |
| --- | --- |
| Primary transport | WebSocket JSON-RPC at a configurable URL |
| Rust integration | Direct async crate API; no service required |
| Other languages | Standalone gRPC gateway with a versioned protobuf contract |
| JSON output | Generic command results use `Value`; typed event structs/enums serialize to JSON inside protobuf strings |
| Process ownership | Explicit attach versus managed launch; never infer ownership |
| Concurrency | Shared client handle; bounded queues, one correlation owner |
| Reconnection | Explicit opt-in bounded retry; never replay application calls |
| Events | Typed enum and payload structs; live, bounded delivery with explicit gap reporting |
| cliche distribution | Operator supplies executable or JAR; no bundled cliche or Nix dependency |
| Initial scope | One cliche endpoint per client/gateway; generic method access plus common helpers |
| License proposal | Apache-2.0; add complete license before distributing code |

Default Cargo features should enable only plain WebSocket attachment. Optional
`tls` adds rustls for `wss://`; optional `process` adds subprocess functionality.
A `wss://` URL without TLS support should fail clearly. Keep tonic, prost, CLI,
process signaling dependencies, and protobuf generation out of the default client.

Node integration through gRPC means running a service or sidecar. It is not a
native npm import of the Rust crate. An npm package wrapping the generated gRPC
client can follow; N-API bindings are deferred until an embedded Node use case
justifies a second integration surface. Browser support would require a separate
gRPC-Web or HTTP bridge and is outside the initial scope.

## 2. Architecture and workspace

```text
Rust application ─────── cliche-client ── WebSocket ── cliche
                            │                          ▲
Node / other application    │                          │
        └── gRPC ── cliche-gateway                     │
                            └── optional process supervisor
                                starts local cliche, then uses WebSocket
```

Planned layout:

```text
Cargo.toml                         workspace and shared metadata/lints
Cargo.lock                         committed for gateway and reproducible CI
crates/cliche-client/
  src/lib.rs                       small documented public surface
  src/client.rs                    builder, cloneable handle, command admission
  src/connection.rs                socket tasks and connection state machine
  src/protocol.rs                  request/response/notification decoding
  src/events.rs                    typed events, subscriptions, lag reporting
  src/error.rs                     structured errors and execution certainty
  src/commands.rs                  common helpers and parameter structs
  src/process.rs                   optional owned-child lifecycle
  examples/attach.rs
  examples/start.rs
  examples/events.rs
  tests/                          fake server and fake process integration tests
crates/cliche-gateway/
  src/lib.rs                       embeddable service implementation
  src/main.rs                      configuration, startup, graceful shutdown
  src/service.rs                   authorization, deadlines, JSON conversion
  proto/cliche/v1/cliche.proto     promoted version of the design draft
  tests/
examples/node/                     minimal gRPC client and event subscription
.github/workflows/ci.yml
README.md
CONTRIBUTING.md
LICENSE
```

Start with internal concrete WebSocket code. Factor a small transport interface
only when implementing a second transport. Defer stdin/stdout command transport
until WebSocket behavior is stable and a user needs it; supervision still drains
process output from the first managed-launch release.

## 3. Rust API contract

Proposed API shape, to be refined in the first implementation milestone:

```rust,ignore
let client = Client::builder("ws://127.0.0.1:12000/")
    .request_timeout(Duration::from_secs(30))
    .connect().await?;

// Subscribe before issuing commands whose notifications matter.
let mut events = client.subscribe();
let info: serde_json::Value = client.call("get-info", json!({})).await?;
let invoice: serde_json::Value = client.create_invoice(params).await?;
while let Some(record) = events.recv().await? {
    match record {
        EventRecord::Notification(envelope) => match envelope.event {
            ClicheEvent::PaymentReceived(payment) => {
                handle_payment(payment.payment_hash, payment.msatoshi).await?;
            }
            ClicheEvent::PaymentFailed(payment) => {
                handle_failure(payment.payment_hash, payment.failure).await?;
            }
            _ => {}
        },
        EventRecord::Gap(gap) => reconcile_payments(gap).await?,
        EventRecord::Connection(status) => handle_connection(status),
        EventRecord::InvalidNotification(error) => handle_decode_error(error),
    }
}
```

- `Client` is cheap to clone, `Send + Sync`, and shares a single connection.
- Use the caller's Tokio runtime; never create a hidden runtime or thread pool.
- `call` returns the upstream result as `Value`; cliche errors are typed Rust
  errors carrying the original code/message. Serialization is available for
  consumers that need complete JSON responses and errors.
- Helpers validate parameters and map wire field names, then delegate to `call`.
  Keep command results as JSON in v0.1; typed result helpers can follow. Event
  payloads are typed from v0.1 and do not require callers to index JSON fields.
- Public parameter structs implement serde where useful. Use `Option` to retain
  omitted versus explicit values. Validate positive amounts, ports, hash/preimage
  lengths, counts, and signed 64-bit upstream bounds without duplicating every
  upstream domain rule. Allow negative resize deltas where supported.
- Prefer explicit per-call options for exceptional deadlines; avoid forcing
  all operations to share one fixed timeout.
- `close().await` shuts down the shared connection and resolves all callers;
  document that it affects every clone. Dropping the final handle signals cleanup.
- Managed launch returns a distinct `ManagedCliche` owner with `client()` and
  `shutdown().await`; ordinary `Client` clones never own the child process.

Define errors including `InvalidArgument`, `Connection`, `Protocol`, `Upstream`,
`Timeout`, `Cancelled`, `Overloaded`, `Closed`, `ProcessExited`, and startup errors.
Expose an execution-state field (`not_sent` or `unknown`) for command failures.
Once transmission begins, timeout/disconnect/cancellation cannot prove nonexecution.
Cancellation only stops waiting; cliche has no remote cancellation protocol.

### Typed event contract

Use a public `#[non_exhaustive] ClicheEvent` enum with a dedicated public struct
for each known notification, modeled after the Go client's event structs. All
payload structs implement `Clone`, `Serialize`, and `Deserialize`. Known fields
are typed; JSON is the serialization format, not the event-facing Rust API.
The following are proposed API sketches, not implemented Rust code:

```rust,ignore
#[non_exhaustive]
pub enum ClicheEvent {
    Ready(ReadyEvent),
    PaymentSucceeded(PaymentSucceededEvent),
    PaymentFailed(PaymentFailedEvent),
    PaymentReceived(PaymentReceivedEvent),
    LnurlPayParams(LnurlPayParamsEvent),
    LnurlPaySuccess(LnurlPaySuccessEvent),
    Unknown(UnknownEvent),
}

pub struct PaymentReceivedEvent {
    pub payment_hash: String,
    pub preimage: String,
    pub msatoshi: Option<i64>,
    // Extension fields retained separately; known fields above stay typed.
    pub extra: ExtensionFields,
}

pub struct PaymentSucceededEvent {
    pub payment_hash: String,
    pub preimage: String,
    pub msatoshi: i64,
    pub fee_msatoshi: i64,
    pub parts: Option<u64>,
    pub extra: ExtensionFields,
}

pub struct PaymentFailedEvent {
    pub payment_hash: String,
    pub parts: u64,
    pub failure: Vec<String>,
    pub routes: Option<Vec<String>>,
    pub extra: ExtensionFields,
}
```

Use `i64` for upstream monetary integers and `u64` for nonnegative counts. Hashes
and preimages start as documented hex strings to keep the SDK lightweight.
`ExtensionFields` is a flattened map of unknown JSON fields used only for forward
compatibility. `UnknownEvent` carries the original method and raw params only for
unrecognized future notification methods. Neither escape hatch replaces a typed
payload for a known method. Do not derive `Debug` on secret-bearing payloads
without an implementation that redacts preimages and other sensitive values.

| Upstream method | Rust payload | Required/optional fields |
| --- | --- | --- |
| `ready` | `ReadyEvent` | Empty object plus retained extensions |
| `payment_received` | `PaymentReceivedEvent` | Required hash/preimage; optional `msatoshi` |
| `payment_succeeded` | `PaymentSucceededEvent` | Required hash/preimage/amount/fee; optional `parts` for compatibility with the Go schema |
| `payment_failed` | `PaymentFailedEvent` | Required hash, `parts`, `failure`; optional `routes` |
| `lnurlpay_params` | `LnurlPayParamsEvent` | `min: i64`, `max: i64`, `callback: String`, `description: String`, `comment: i64`, optional typed `payerdata` |
| `lnurlpay_success` | `LnurlPaySuccessEvent` | Optional `success_action: String` |

`LnurlPayerData` uses named `auth`, `name`, and `pubkey` boolean fields. Confirm
LNURL serialization against pinned fixtures before freezing these public types;
preserve extension fields on every payload, including nested payer data.
Do not replace missing required amounts with zero or add defaults that hide a
malformed payload. Optional fields use `Option` and skip serialization when absent
so omitted upstream fields remain omitted. The pinned source omits succeeded
`parts` and can omit received `msatoshi`, unlike the Go structs' required fields.

`EventRecord` is a typed enum separating `Notification(EventEnvelope)`,
`Connection(ConnectionEvent)`, `Gap(EventGap)`, and
`InvalidNotification(EventDecodeError)`. `EventEnvelope` carries local stream
metadata and `event: ClicheEvent`; connection/gap/decode types use typed reason
and state enums with structured fields. Avoid generic JSON for bridge records too.

Decode once before fan-out. Dispatch on `method`, then deserialize the selected
payload struct. A known method with invalid fields yields a typed decode-error
record and is never relabeled `Unknown` or emitted as a successful payment.
Continue reading and resolving unrelated calls. Decode-error records identify
the method and safe field path, omit raw secret values, and tell consumers to
reconcile if business events may have been skipped. They bypass method filters,
as gap/status records do. Truly unknown methods retain raw params and are delivered
through `Unknown`; do not use a serde catch-all that discards their contents.

Serialize event enums as the existing `method`/`params` shape and bridge records
with a `kind` tag. Custom dispatch/serialization can handle `Unknown` and flattened
metadata without losing fields. The gateway serializes these same typed records;
it must not maintain a separate unvalidated JSON event path. Typed protobuf event
messages would be a separate API decision; the initial gRPC wire remains JSON.

### JSON envelopes

Keep cliche payloads unchanged. Bridge-added metadata lives outside `result` or
`params`. Proposed serialized examples:

```json
{"schema_version":1,"kind":"response","request_id":"app-42","upstream_id":"session:1","result":{"ping":"pong"}}
```

```json
{"schema_version":1,"kind":"error","request_id":"app-42","error":{"kind":"timeout","message":"response deadline exceeded","execution":"unknown"}}
```

```json
{"schema_version":1,"kind":"notification","stream_id":"subscription-session","sequence":12,"method":"payment_received","params":{"payment_hash":"example","msatoshi":12000}}
```

```json
{"schema_version":1,"kind":"gap","stream_id":"subscription-session","sequence":13,"reason":"upstream_disconnected","missed_count":null}
```

Use tagged serializable enums to prevent contradictory shapes; event payloads
come from the typed event contract above.
Upstream error envelopes additionally retain `upstream_code` and the original
message. Sequence numbers describe local observation order, not payment order,
upstream offsets, or proof of complete delivery. Every subscription has an opaque
stream ID; sequence numbers restart when a new subscription is created. Filtered
notifications do not create apparent gaps in that subscription's sequence.

Rust `Value` preserves signed 64-bit integer values. Do not route JSON through
floating-point protobuf `Struct`/`Value`. Node examples must use a parser that
preserves large integer tokens (BigInt/lossless numbers) and must not pass monetary
values through `Number` beyond its safe range. Include tests above `2^53` for
responses, events, and outgoing params. JSON string fields must contain actual
JSON, rather than debug text or unquoted arbitrary output.

## 4. Connection, correlation, and failure semantics

Use one connection supervisor to own the pending map and a bounded command queue.
A dedicated read loop continually drains the socket; a bounded writer path sends
frames without preventing reads. Internal channels must not create a deadlock
between response processing and writes. Heartbeats and shutdown need prompt
access even under load. Keep this topology small and document its ownership.

1. Validate configuration and input before admitting a command.
2. Reserve an in-flight permit and a command queue slot under the call deadline.
3. Generate an ID from a random 128-bit client-session token plus a monotonic
   counter. Renew the session token on reconnect; guard counter overflow.
4. Register the pending sender before writing. Send JSON with a string ID,
   `jsonrpc: "2.0"`, a method, and object params.
5. Match only owned pending IDs. Resolve each at most once. Ignore foreign,
   duplicate, and late replies, with payload-free diagnostics.
6. Decode notifications without IDs into `ClicheEvent` by method. Preserve
   unknown methods/fields and report invalid known payloads as typed decode errors.
   Reject malformed or ambiguous response shapes; a `null` result is valid.
7. Remove pending entries on every exit path: reply, write failure, timeout,
   cancellation, connection loss, and shutdown. Caller-drop cleanup must not
   depend on finding spare room in the ordinary command queue.

On disconnect, immediately fail all pending requests with execution certainty
appropriate to whether transmission started. Do not queue application calls while
disconnected; fail them promptly. Never replay payments, channel commands, invoice
creation, or generic calls, even if reconnect succeeds. New requests require an
active connection. Document that a gRPC deadline can expire after cliche has
accepted a command; the caller must reconcile state rather than blindly retry.

Initial defaults to measure and tune: connect timeout 10 seconds; per-call deadline
30 seconds including admission/write/wait; maximum in-flight 128; outgoing queue
128; notification ring 256; WebSocket message maximum 4 MiB. Bound fragments,
message assembly, and outgoing payloads as well as queue counts. Large graph or
payment responses may require higher configured limits. Record why limits exist.

Optional reconnect uses exponential backoff with jitter, e.g. 250 ms to 30 seconds,
and a finite configured elapsed-time/attempt budget. Stop on explicit shutdown,
nonretryable handshake/protocol errors, or budget exhaustion. Emit connection and
gap records. A fresh `ping` establishes command readiness after each connection;
validate optional expected chain hash/node pubkey using `get-info`. Do not silently
attach to a replacement wallet after a detected identity mismatch.

Separate WebSocket ping/pong handling from the cliche `ping` command. Verify
protocol heartbeats against the actual server before promising them; a bounded
JSON-RPC ping can provide idle liveness if needed. Failed heartbeats follow the
same disconnect semantics. No automatic restart of a crashed cliche child in v0.1.

## 5. Event delivery

Use a bounded fan-out channel, such as Tokio broadcast, carrying shared typed
`EventRecord` values. The socket reader never waits on a callback or slow subscriber. Provide
a wrapper subscription API rather than exposing Tokio channel types publicly.

- Multiple consumers receive notifications independently; no subscription is
  required for command replies to keep working.
- A slow receiver gets an explicit gap record with a known dropped count where
  available. A socket disconnect produces a gap of unknown size.
- For gRPC, a subscriber with a full output queue must receive a gap before the
  next notification when it resumes. Terminate persistently stalled streams with
  `RESOURCE_EXHAUSTED`; never silently resume without reporting loss.
- Reserve or retain gap state independently of the notification queue, since a
  full queue cannot reliably accept its own gap notification.
- Emit connection status separately from upstream notification methods. Optional
  method filters do not filter bridge gap/status/decode-error records.
- Expose current connection status through a snapshot/watch mechanism; a newly
  subscribed consumer receives a local snapshot rather than waiting for `ready`.
- Payment completion may precede the corresponding command reply. Match payment
  events by hash, never by order. LNURL intermediate events remain uncorrelated.

There is no exactly-once or durable event guarantee. Applications requiring durable
business processing persist their own payment state and reconcile known hashes
with `check-payment`, using explicit `list-payments` counts where useful. The
recent list has no guaranteed full-history cursor. Do not add a wallet database
reader or pretend local sequence numbers support upstream replay.

## 6. Managed process lifecycle and Nix

Support two modes:

- Attach: connect to a configured URL. Closing only closes our connection.
- Managed: start an explicitly configured local native binary (Nix included),
  wait for WebSocket command readiness, and retain an owned process handle.

Model launch configuration as an enum for `Native { executable }` and later
`Jar { java, jar }`; build argument arrays with `tokio::process::Command`.
Require an explicit data directory, network, host, and port for managed mode.
Configuration must derive both launch properties and connection URL from one
validated source. Do not accept conflicting duplicate `-D` properties silently.

Use `-Dcliche.datadir=...`, `-Dcliche.websocket.host=127.0.0.1`,
`-Dcliche.websocket.port=...`, `-Dcliche.network=...`, and
`-Dcliche.json.compact=true` for native launch. Put Java `-D` options before `-jar`
for JAR launch. Seed provisioning remains an operator-controlled configuration
file; never embed seed strings in CLI examples, logs, or the library API.

Check that the requested local port is unused before launch, then watch child
exit while attempting bounded connection and `ping` readiness. Preflight alone
cannot prove endpoint ownership due to races. If startup sees an existing
listener, fail rather than adopting it. Use an application ownership lock for
managed data directories, optional expected identity, and a dedicated port;
explain residual races against independently started processes. Retain the lock
until child exit. Never claim that a lock is honored by cliche itself.

Keep stdin open and drain both output pipes throughout startup and runtime.
Treat stdout/stderr as untrusted, potentially secret diagnostics, not a second
event feed. Default to discarding content with byte/line counters; explicit raw
diagnostic capture must be bounded and documented as sensitive. Report exit
status and safe diagnostic categories without dumping raw output in errors.

`shutdown().await` stops admission, resolves callers, closes the socket, requests
termination of the owned child, waits a configurable grace period, then kills
and reaps if necessary. Verify actual SIGTERM behavior on Linux with disposable
data; there is no inspected upstream shutdown RPC. Do not promise wallet-level
clean shutdown merely because a signal was delivered. Define platform-specific
fallbacks and clean up processes on startup timeout/cancellation too. Final-owner
drop should initiate best-effort cleanup; explicit async shutdown is required
for guaranteed wait/reap while the runtime is alive.

The Linux Nix build remains a separate deployment input. The library accepts
`/path/to/cliche-nix/result/bin/cliche`; production can supply a stable Nix profile
or store path. Never run `nix build` inside the SDK. Do not load the user's existing
wallet to test process management. Live managed tests need an isolated directory,
seed configured out of band, and a dedicated unused port.

## 7. gRPC gateway contract

Promote [the draft proto](cliche-v1.proto) to `cliche.v1` after review. Initial RPCs:

| RPC | Behavior |
| --- | --- |
| `Call` | Generic method and JSON-object params; returns response/upstream-error JSON envelope |
| `SubscribeEvents` | Server stream of JSON serialized typed notification/gap/status/decode-error records with optional method filters |
| `GetStatus` | JSON connection status and safe capability/config metadata |

The gateway shares one client connection across callers and creates unique
upstream IDs itself. `request_id` is an optional caller label echoed to that
caller; it is neither upstream routing identity nor an idempotency guarantee.
Enforce limits on label length, method names, params, messages, subscribers, and
concurrent calls. gRPC does not turn cliche into a transactionally idempotent API.

Keep expected cliche command failures as JSON application envelopes so clients
retain upstream error details. Use gRPC statuses for gateway failures:
`INVALID_ARGUMENT`, `UNAVAILABLE`, `DEADLINE_EXCEEDED`, `CANCELLED`,
`RESOURCE_EXHAUSTED`, `UNAUTHENTICATED`, `PERMISSION_DENIED`, or `INTERNAL`.
Attach a bounded serialized error envelope in status details so timeout and
execution certainty survive non-OK responses. Node examples decode those details.
Use the smaller of caller deadline and configured gateway limits; cancellation
removes the pending waiter without claiming to cancel upstream execution.

Launch and attach are gateway startup configurations, not remotely supplied
filesystem paths or executable commands. Keep arbitrary process start/stop RPCs
out of v0.1. `GetStatus` reports attach/managed mode but never secrets or raw
process arguments. One gateway serves one wallet trust boundary.

Bind to loopback by default. Require an explicit authenticated deployment policy
for nonloopback exposure (TLS/mTLS or a trusted TLS-terminating proxy plus gateway
credentials). Apply authorization to event subscriptions as well as commands.
Provide method allowlists; generic calls must pass the same checks as helpers.
Events are wallet-wide, so this gateway cannot provide tenant isolation simply by
filtering method names. TLS to a proxy does not add authentication to cliche's
own plain socket. Keep the cliche endpoint private.

Implement standard gRPC health, reporting not-serving while disconnected or
shutting down. Reflection is opt-in for development. Log safe method names,
latency, queue counts, reconnects, and dropped-event counts; omit payloads,
preimages, tokens, seeds, and full URLs containing credentials. Use `tracing`
without installing a subscriber from the library. Prometheus export is optional
and belongs to the gateway, not the default client.

## 8. Dependencies and lightweight budget

Client candidates: Tokio with minimal runtime/network/sync/time features,
`tokio-tungstenite` with explicitly selected connection features, `futures-util`
with only required sink/stream support, serde/serde_json, thiserror, a small random
session-ID source, and tracing. `process` adds Tokio process/io support and a
minimal OS signaling/locking solution selected after lifecycle tests. `tls` adds
rustls through the WebSocket dependency. Avoid `tokio/full`, HTTP clients, wallet
crates, custom runtimes, and dynamic plugin systems.

Gateway candidates: tonic, tonic-prost/prost and matching code-generation tools,
Tokio streaming adapters, tonic-health, and a small CLI/config surface. Keep
code generation in the gateway build/development path; ordinary client consumers
must never need protoc. Before pinning versions, verify the current matching
Tonic/Prost code-generation workflow and supported MSRV.

Measure `cargo tree -e features`, clean build time, example binary size, idle
RSS, and sustained pending/event memory at the first working baseline. Record
OS, compiler, release profile, TLS/process features, and workload. Use the
baseline to set justified regression thresholds; do not promise a fabricated
binary-size target before measurement. Verify queue limits prevent unbounded
memory growth under stalled consumers and disconnected clients.

Useful current primary documentation:
[WebSocket integration](https://docs.rs/tokio-tungstenite/latest/tokio_tungstenite/),
[Tokio broadcast and lagging](https://docs.rs/tokio/latest/tokio/sync/broadcast/),
[Tonic](https://docs.rs/tonic/latest/tonic/),
[serde_json Value](https://docs.rs/serde_json/latest/serde_json/value/enum.Value.html).

## 9. Implementation milestones and commit sequence

Each numbered item is a reviewable milestone with focused commits. Keep tests
with their corresponding behavior. Prefer completing the client before the
process supervisor or gateway multiplies failure paths.

1. **Workspace and protocol contract**
   - `chore: initialize rust workspace and contributor tooling`
   - `test(protocol): capture pinned cliche wire behavior`
   - Add full license, verified MSRV, minimal manifests, CI, synthetic fixtures,
     error/envelope types, parser, and serialization tests.
   - Done when string IDs, exact parameter casing, null results, errors, unknown
     notifications, optional fields, and signed 64-bit integers are covered.

2. **WebSocket attachment and concurrent commands**
   - `feat(client): connect and correlate websocket commands`
   - `feat(client): bound calls and clean up failed requests`
   - Add builder, connect/readiness ping, actor/task ownership, request IDs,
     bounded admission, deadlines, cancellation, and deterministic close.
   - Done when parallel out-of-order calls route correctly, foreign replies
     cannot satisfy calls, and every terminal path releases pending capacity.

3. **Event subscriptions and connection lifecycle**
   - `feat(events): expose typed notifications and delivery records`
   - `feat(client): add opt-in reconnect without command replay`
   - Done when zero subscribers and stalled subscribers cannot block commands;
     disconnects/lag are observable; reconnect never resends any admitted call;
     every known notification has a typed payload; unknown notifications remain
     intact; invalid known payloads report decode errors; shutdown leaves no tasks.

4. **Command helpers and Rust examples**
   - `feat(commands): add common cliche command helpers`
   - `docs: document rust integration and payment reconciliation`
   - Implement MVP helpers in PROTOCOL.md plus generic access to all commands.
   - Done when exact outgoing JSON is verified and examples compile; invoice
     initiation and final payment completion are documented separately.

5. **Optional process supervision**
   - `feat(process): launch and supervise owned native cliche instances`
   - `test(process): cover startup races shutdown and pipe pressure`
   - Add isolated fake-child tests before opt-in Nix smoke tests. Native binary
     support first; JAR support can be a separate follow-up commit.
   - Done when early exit, missing seed, busy port, startup timeout/cancellation,
     pipe flooding, and shutdown escalation clean up only the owned process.

6. **gRPC gateway and cross-language example**
   - `feat(gateway): expose json calls status and event streams`
   - `feat(gateway): enforce deadlines access controls and health`
   - `docs(node): add grpc integration and lossless json examples`
   - Done when Node can get status, call ping/get-info, and consume notifications;
     upstream errors, gRPC status details, large integers, stream lag, auth,
     cancellation, and concurrent caller isolation pass contract tests.

7. **Compatibility and first public release**
   - `test(integration): verify pinned nix cliche compatibility`
   - `docs: publish deployment compatibility and release guidance`
   - Record tested revisions and toolchains; measure dependency/size/memory baseline;
     complete package metadata, changelog, API docs, and release checklist.
   - Done when default/TLS/process configurations and gateway checks pass,
     packages pass dry-run validation, and a reviewer can reproduce the examples.
   - Actual publication, releases, and remote repository setup are separate actions.

## 10. Validation strategy and release gates

Use a fake WebSocket server for fast deterministic tests; it must reproduce
broadcast replies, out-of-order responses, malformed messages, missing IDs,
late replies, abrupt closes, large frames, and optional fields. Use Tokio virtual
time where appropriate for deadlines/backoff. Test cleanup by observable pending
capacity and task completion, not by sleeps or implementation-mirroring assertions.

Key acceptance scenarios:

- Hundreds of interleaved calls and events preserve ID routing and bounded memory.
- A timed-out call is never replayed, including after successful reconnect.
- Call future cancellation during admission, writing, and waiting releases resources.
- A response `result: null` succeeds; foreign/duplicate IDs never become notifications.
- Every known event decodes to its corresponding struct; missing received amounts
  and succeeded parts remain `None`, never fabricated zeros.
- Typed events serialize back to the documented JSON shape, preserving extensions
  and unknown methods; malformed known events cannot become successful payments.
- A slow event consumer reports loss while unrelated commands still finish.
- Drop/close/reconnect races cannot strand waiters or owned child processes.
- A process that prints unlimited output cannot block startup or grow memory forever.
- A preexisting listener is never adopted as a successfully spawned child.
- gRPC callers using the same request label cannot receive each other's replies.
- Lossless JSON survives Rust -> protobuf -> Node -> protobuf -> Rust above `2^53`.
- Gateway policies reject unauthorized commands and event subscriptions consistently.

Once implemented, CI should run formatting, Clippy with warnings denied, tests,
doctests, and rustdoc with warnings denied on current stable. Build/test the client
on the declared MSRV. Exercise default, TLS, process, and combined supported
features explicitly; also build/test the gateway. Linux is the managed-process
and Nix compatibility target; test attach mode on macOS and Windows where CI
permits. Run locked dependency builds for the gateway and periodically review
advisories, licensing, and feature growth. Do not gate unimplemented features.

Live compatibility tests are opt-in and read-only by default (`ping`, `get-info`,
and explicitly scoped queries). Validate root path, readiness, compact frames,
field casing, known/unknown notifications, and current binary provenance. Use a
separate disposable testnet setup for payment/channel tests; ordinary CI must
never depend on the user's running wallet or real funds. Financial mutations
need a purpose-built, explicitly configured test environment.

Before release: run `cargo package`/publish dry-run validation in dependency order,
verify no wallet artifacts are packaged, prove the default client dependency tree
excludes gRPC and process-only crates, document license/MSRV/support policy, and
publish a compatibility matrix distinguishing source-reviewed from live-tested
versions. Avoid promising compatibility with arbitrary future Nix patches.

## 11. Scope deferred from v0.1

Stdin command transport, N-API bindings, browser support, durable event storage,
exactly-once delivery, automatic command retries, remote deployment orchestration,
multi-wallet routing, arbitrary remote process control, and automatic child
restarts remain separate proposals. These are scope boundaries, not required
extensions of the initial implementation.

The first implementation step is workspace creation and protocol fixtures, followed
by a working attach/ping/call/event path. The current repository intentionally
contains planning documents and a draft contract only.
