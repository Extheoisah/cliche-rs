# cliche-rs

A planned lightweight Rust client for [cliche](https://github.com/nbd-wtf/cliche),
with WebSocket communication, JSON responses, typed Rust events that serialize
to JSON, optional subprocess
management, and a separate gRPC gateway for other languages.

**Status: implementation planning. No working client is implemented yet.**
The repository name and proposed crate names are provisional; registry
availability has not been checked.

## Intended integrations

- Rust applications depend directly on `cliche-client` and use their Tokio runtime.
- Node.js and other applications use the `cliche-gateway` service through gRPC.
- Either integration can attach to an existing cliche WebSocket endpoint.
- Optional process support launches a configured cliche executable, then uses
  WebSocket for commands and events. A process runs on the host where it is
  launched; remote deployment belongs to the deployment platform.

## Design documents

- [Implementation plan](docs/IMPLEMENTATION_PLAN.md): architecture, API proposals,
  failure semantics, implementation milestones, acceptance criteria, and commits.
- [Protocol findings](docs/PROTOCOL.md): source-verified behavior and compatibility.
- [Proposed gRPC contract](docs/cliche-v1.proto): draft contract, not generated code.
- [Contributing](CONTRIBUTING.md): Rust conventions and contribution workflow.

Start with the WebSocket client. Keep process management optional and the gRPC
stack in a separate crate so embedding applications only pay for what they use.
