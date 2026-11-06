# AstraCraft

A Rust-based experimental client for Minecraft Java Edition 1.21.8 (protocol 772).

## Current status

AstraCraft can query server status, perform offline-mode login, negotiate zlib compression, complete Configuration, and maintain a Play session. The Play loop handles keep-alive/ping, Join Game, player teleports, chunk batch acknowledgements, chunk loading and chunk unloading. Decoded block states are stored in a bounded in-memory world model.

Not implemented yet: Microsoft/Mojang online authentication, full dimension-registry interpretation (custom dimensions currently use fallback bounds), block entities and lighting storage, entities, inventory interaction, input, GUI, and GPU rendering.

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


### Play packet decoding

The protocol 772 Play loop now decodes the Join Game packet (world list, dimension registry key, game mode and view distances) and Synchronize Player Position packets. Position teleports are acknowledged with the matching serverbound Teleport Confirm packet. Chunk Data packet decoding now validates heightmap NBT, section lengths, block/biome palettes, packed-long counts and per-section non-air counts. Block-state IDs are imported into the world model; biome values and the remaining block-entity/light payload are not yet retained. Dimension bounds are selected for vanilla Overworld/Nether/End names; custom dimension registry decoding is still pending. This remains a networking/world-data foundation rather than a playable renderer.
