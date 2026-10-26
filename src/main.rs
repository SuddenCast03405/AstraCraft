mod chat;
mod metrics;
mod nbt;
mod net;
mod protocol;
mod session;

fn main() {
    println!("AstraCraft — Minecraft Java Edition 1.21.8 client foundation");
    println!("Protocol target: {}", session::PROTOCOL_VERSION);
    println!("Implemented: VarInt framing, bounded TCP packet transport, session state machine, NBT/chat helpers.");
    println!("Not yet implemented: compression/encryption negotiation, full login/configuration flow, GUI and 3D rendering.");
}

#[cfg(test)]
mod tests {
    #[test]
    fn project_smoke_test() {
        assert_eq!(2 + 2, 4);
    }
}
