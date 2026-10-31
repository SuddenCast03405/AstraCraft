# AstraCraft

A Rust-based experimental client for Minecraft Java Edition 1.21.8 (protocol 772).

## Current status

AstraCraft can query server status, perform offline-mode login, negotiate zlib compression, complete the Configuration state, and keep a TCP session alive in Play. The Play loop currently answers protocol 772 keep-alive and ping packets and reports other packet IDs; it does not yet decode the world or provide playable controls.

Not implemented yet: Microsoft/Mojang online authentication, complete Play packet decoding, registry interpretation, chunk/world state, entity and inventory interaction, input, GUI, and GPU rendering.

## Build and run

```sh
cargo test
cargo run --release -- ping <host:port>
cargo run --release -- login <host:port> <username>
```

The login command requires a server that permits offline-mode clients. It remains connected in Play until the server disconnects or the process is stopped (Ctrl+C).

The connectivity workflow sets `ASTRACRAFT_LOGIN_PROBE=1` so CI verifies the Login and Configuration handshake without holding the job open.

## Design principles

- Keep networking, protocol, session state, world state, and rendering separate.
- Treat packet IDs as connection-state-specific and version-pinned.
- Bound allocations and parser depth for untrusted server data.
- Offline-mode support is not a way to bypass online authentication; servers decide which authentication modes they accept.

Target protocol: Java Edition 1.21.8, protocol version 772.
