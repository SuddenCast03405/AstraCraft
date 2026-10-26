# AstraCraft

A Rust-based experimental client for Minecraft Java Edition 1.21.8 (protocol 772).

## Current status

This repository starts with protocol primitives and defensive parsing foundations. It is **not yet a playable client**. The graphical frontend, full login/configuration flow, compression/encryption, registry and chunk decoding, entity/inventory interaction, and GPU rendering remain to be implemented.

## Build

```sh
cargo test
cargo run --release
```

## Design principles

- Keep networking, protocol, session state, world state, and rendering separate.
- Treat packet IDs as connection-state-specific.
- Bound allocations and parser depth for untrusted server data.
- Do not claim offline-mode support means bypassing online authentication; servers decide which authentication modes they accept.

Target protocol: Java Edition 1.21.8, protocol version 772. Verify packet IDs and field layouts against a version-pinned protocol reference before enabling live packet handling.
