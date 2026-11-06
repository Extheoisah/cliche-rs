# Initial lightweight baseline

Measured 2026-11-06, macOS arm64, rustc 1.94.0, default release profile, no client
TLS/process features, current-thread Tokio runtime. Dependencies were already
downloaded; release artifacts were initially absent. Example builds also compile
dev-dependencies, so build duration is not a minimal consumer-library measurement.

The self-contained `measure` example runs the client and mock server in the same
process. These numbers include both, and are a reproducible local comparison
point rather than production capacity claims or isolated client allocation counts.

| Measurement | Observed |
| --- | --- |
| First release example build | 15.60 seconds wall time |
| Unstripped release executable | 1,960,416 bytes |
| Idle maximum resident set size | 2,981,888 bytes |
| Load maximum resident set size | 5,029,888 bytes |
| Load calls | 12,800 calls with up to 128 concurrent callers |
| Load payload | 1 KiB string echoed as a result and an unknown notification per call |
| Event consumer | Intentionally stalled; 256-record buffer, observable lag verified |
| Measured load loop | 141 milliseconds |

Reproduce:

```sh
cargo build -p cliche-client --example measure --release --no-default-features
/usr/bin/time -l target/release/examples/measure --idle
/usr/bin/time -l target/release/examples/measure
```

Use `/usr/bin/time -v` on Linux. macOS first-execution assessment added startup
wall time in the idle invocation, outside the measured loop. Timings and resident
memory depend on hardware/OS and need a stable runner before regression thresholds
are enforced. The workload tests bounded pending/event buffers; it is not a long
soak test or a comparison against a competing SDK.

`cargo tree -p cliche-client --no-default-features --edges normal` shows no tonic,
prost, fs2, nix, or rustls. Those costs remain outside the default runtime dependency
graph. TLS test certificate tooling is dev-only; gateway/process/TLS features add
their documented dependencies when enabled.
