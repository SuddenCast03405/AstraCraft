# AstraCraft

A Rust-based experimental client for Minecraft Java Edition 1.21.8 (protocol 772).

## Current status

This repository is an early protocol client foundation. It can query server status and attempt an **offline-mode login** through the Login and Configuration states, including negotiated zlib compression and basic configuration keep-alive/ping replies. It is **not yet a playable client**: Microsoft/Mojang online authentication, the complete play-state protocol, registry interpretation, chunk/world decoding, entity and inventory interaction, and GUI/GPU rendering remain to be implemented.

## Build

```sh
cargo test
cargo run --release -- ping <host:port>
```

## Commands\n\nQuery a server status:\n\n```sh\ncargo run --release -- ping play.example.net:25565\n```\n\nAttempt an offline-mode login (the server must allow offline-mode clients):\n\n```sh\ncargo run --release -- login play.example.net:25565 PlayerName\n```\n\nThe login command reports success only after the server sends Finish Configuration and AstraCraft acknowledges it. That means the protocol reached the Play state boundary; it does not mean world rendering or gameplay is available. Online-mode servers will be rejected until official session authentication and encryption are implemented.\n\n## Design principles

- Keep networking, protocol, session state, world state, and rendering separate.
- Treat packet IDs as connection-state-specific.
- Bound allocations and parser depth for untrusted server data.
- Do not claim offline-mode support means bypassing online authentication; servers decide which authentication modes they accept.

Target protocol: Java Edition 1.21.8, protocol version 772. Verify packet IDs and field layouts against a version-pinned protocol reference before enabling live packet handling.
