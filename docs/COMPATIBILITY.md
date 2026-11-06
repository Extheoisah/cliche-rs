# Compatibility and validation

Recorded 2026-11-06 on macOS arm64, rustc/cargo 1.94.0, protoc 24.3,
Node 24.6.0. Rust 1.94 is the currently tested minimum, not a promise that older
compilers cannot work. CI is configured for stable and 1.94 plus attach-mode tests
on macOS/Windows; those hosted CI runs have not occurred yet.

| Target | Status |
| --- | --- |
| cliche `75ac2cadc77290127781c345cea823b13957320d` | Source-reviewed protocol; synthetic fixtures and mock integration tested |
| go-cliche `f2a5e32695ac6d76b3d3ea75cb89af7b492153d8` | Event schema compared against actual cliche source; optional differences retained |
| Local cliche-nix HEAD `00e9d8e12a14f01205e10ef6e3d450a954bddbfc` | Lockfile/patches reviewed; same cliche revision, no API patches found |
| User-reported Nix cliche at `ws://127.0.0.1:12000/` | Read-only live attachment tested from this macOS host; binary revision/host provenance not independently verified |
| Managed Nix native binary on Linux | Not live-tested; no local Nix/executable available for isolated process tests |
| Mock WebSocket and verified local TLS | Tested, including readiness, heartbeat, identity mismatch, limits, disconnect and reconnect |
| Owned mock subprocess on macOS | Tested with pipe pressure, termination escalation, drop/cancellation, timeout, early exit and locks |
| Real gRPC HTTP/2 and Node client | Tested against mock cliche, including monetary integers above 2^53 |

## Live attachment result

On 2026-11-06 the user authorized read-only tests against their running endpoint
at `ws://127.0.0.1:12000/`. The library passed ping/get-info, identity consistency
checks using observed pubkey/chain values, and 32 concurrent pings distributed
across two clients. Closing one attached client left the other client and cliche
usable. Wallet responses were not printed or stored as fixtures.

Initial repeated connection tests exposed intermittent readiness timeouts. The
server starts input handling and its output-topic subscription independently, so
a reply can plausibly precede subscription. Bounded retries of the internal
side-effect-free readiness ping fixed the observed failure; the precise upstream
scheduling cause was inferred from source rather than instrumented inside cliche.
A deterministic mock test drops the first probe response to cover this behavior.
Application commands are still never replayed. Timeout errors distinguish
handshake, readiness, and identity-check stages.

This verifies WebSocket attachment to the provided instance, not its binary
provenance, Linux process supervision, or actual payment/LNURL notifications.
Payment events remain covered by pinned source fixtures and mock integration tests.
No invoices, payments, channel changes, restarts, or wallet database reads were
performed.

## Run the opt-in smoke checks

These checks never run in ordinary CI. They only send ping/get-info; they do not
create invoices, move funds, or change channels. Configuration must be explicit.
Do not log get-info output if it contains wallet-sensitive fields.

```sh
CLICHE_TEST_URL=ws://127.0.0.1:12000/ \
  cargo test -p cliche-client --test live read_only_live -- --ignored
```

Optionally set CLICHE_TEST_PUBKEY and CLICHE_TEST_CHAIN to validate identity.
To verify managed Nix startup on a suitable Linux/Nix host, provision a completely
separate non-mainnet data directory with its seed configured out of band:

```sh
CLICHE_TEST_BINARY=/path/to/cliche-nix/result/bin/cliche \
CLICHE_TEST_DATA_DIR=/path/to/disposable-data \
CLICHE_TEST_PORT=12002 \
CLICHE_TEST_NETWORK=testnet \
  cargo test -p cliche-client --features process --test live managed_nix_smoke -- --ignored
```

Record the binary/store provenance, upstream/Nix revisions, OS, network, and
results before marking that combination live-tested. Notification delivery for
actual payments additionally needs a dedicated controlled test environment.

## Packaging

The client passed `cargo package` verification. Both workspace archives were
assembled and checked for license text and accidental wallet/env/node_modules
files. Both unpacked packages compiled together using a local crates.io patch
for the not-yet-published client dependency. The gateway tests carry their own
mock helpers and no longer rely on sibling source files.

Cargo 1.94's offline workspace package verification hit an internal temporary
registry checksum error (`no hash listed for cliche-client`). Independent gateway
packaging also cannot resolve an unpublished client on crates.io. The unpacked
source check validates the code and included files; a registry publication dry run
must still be repeated in dependency order after the final crate names and public
repository/documentation URLs are chosen. No package was published. Reproduce archive/content/compilation checks with
`python3 scripts/check-packages.py --offline` (Python 3.12+ and protoc required).

## Implementation refinements from the original design

- `EventDelivery { schema_version, stream_id, sequence, record }` wraps a typed
  EventRecord. Notification records carry ClicheEvent directly, rather than a
  second nested EventEnvelope. The JSON method/params shape remains unchanged.
- A short synchronous mutex protects pending requests; an RAII guard performs
  immediate caller-drop cleanup without a cancellation queue. No await holds it.
- Malformed known-event diagnostics identify method/category and omit the raw
  serde error (which can quote secrets). Field paths are not currently exposed.
- Generic Rust calls return results; upstream IDs remain internal rather than
  appearing in gateway response envelopes. Application request labels are echoed.
- The gateway implements a trusted TLS-proxy deployment mode. Direct server TLS,
  reflection, custom metrics exporters and idle JSON-RPC heartbeat timers are
  not implemented. WebSocket protocol ping/pong handling is tested.
- Managed gateway startup uses process-exit monitoring. Reconnect and chain-pin
  CLI options currently apply only to attach mode; managed mode accepts pubkey
  pinning. Native and JAR argument construction are supported; JAR execution has
  not been live-tested.
- Process output is discarded with fixed-size draining buffers. Raw diagnostic
  retention and diagnostic byte counters are not exposed in this release.
