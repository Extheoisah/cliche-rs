# Repository guidance

This repository currently contains an implementation plan, not a working client.
Read `docs/IMPLEMENTATION_PLAN.md` and `docs/PROTOCOL.md` before implementation.

- Keep the default Rust client independent of gRPC, process launching, and Nix.
- Use WebSocket as the primary command/event transport.
- Expose known events through typed Rust enums/structs; serialize them to JSON.
- Preserve integer precision and unknown fields/methods through explicit extensions.
- Report malformed known events as typed decode errors; never silently default fields.
- Never replay an application command automatically after a disconnect.
- Keep queues and pending requests bounded; never block socket reads on consumers.
- Only terminate processes owned by this client. Never stop an attached instance.
- Never read local wallet databases or seed files as part of routine discovery.
- Never log seed phrases, preimages, invoices, auth tokens, or raw payloads by default.
- Do not send payment or channel-changing commands to a live instance for tests.
- Prefer small modules, explicit ownership, documented public APIs, and focused commits.
- Keep examples and documentation honest about what is actually implemented.
- Use the validation gates in the plan when their corresponding code exists.
